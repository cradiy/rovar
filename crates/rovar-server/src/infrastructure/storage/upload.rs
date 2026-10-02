use super::{crypto::StorageKey, media::Writer};
use crate::{
    application::ports::{MediaUpload, MediaWriter},
    domain::{
        document::{Media, validate_media},
        error::{Error, Result},
    },
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

const CHUNK: u64 = rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64;

pub(super) struct Upload {
    // The stable lock file lives outside the removable staging directory. It
    // must not be unlinked while another process may hold an open descriptor.
    _lock: std::sync::Arc<File>,
    directory: PathBuf,
    blobs: PathBuf,
    key: StorageKey,
    context: String,
    expected: Media,
    offset: u64,
}

impl Upload {
    pub fn open(
        root: PathBuf,
        blobs: PathBuf,
        key: StorageKey,
        context: String,
        expected: Media,
    ) -> Result<Self> {
        validate_media(std::slice::from_ref(&expected), 0)?;
        let identity = format!("{context}/{}", expected.length);
        let id = format!("{:x}", Sha256::digest(identity.as_bytes()));
        // Serialize opening and unlinking lock files, so a collector cannot
        // replace an inode that an uploader is about to lock.
        let namespace = super::cleanup::namespace(&root).map_err(anyhow::Error::from)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(format!("{id}.lock")))
            .map_err(anyhow::Error::from)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::RateLimited,
            std::fs::TryLockError::Error(error) => Error::Internal(error.into()),
        })?;
        lock.set_modified(std::time::SystemTime::now())
            .map_err(anyhow::Error::from)?;
        drop(namespace);
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).map_err(anyhow::Error::from)?;
        // An acknowledged part is an atomically installed, fsynced file. Partial
        // temporary files never advance the offset, including after a crash.
        let mut offset = 0;
        while offset < expected.length {
            match std::fs::metadata(directory.join(offset.to_string())) {
                Ok(info)
                    if info.is_file()
                        && info.len() == (expected.length - offset).min(CHUNK) + 96 =>
                {
                    offset += (expected.length - offset).min(CHUNK)
                }
                Ok(_) => {
                    // A damaged acknowledged part cannot be resumed. Reset only
                    // this unpublished upload while holding its exclusive lock.
                    std::fs::remove_dir_all(&directory).map_err(anyhow::Error::from)?;
                    std::fs::create_dir(&directory).map_err(anyhow::Error::from)?;
                    offset = 0;
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(Error::Internal(error.into())),
            }
        }
        #[cfg(unix)]
        File::open(root)
            .and_then(|file| file.sync_all())
            .map_err(anyhow::Error::from)?;
        Ok(Self {
            _lock: std::sync::Arc::new(lock),
            directory,
            blobs,
            key,
            context,
            expected,
            offset,
        })
    }

    fn part_context(&self, offset: u64) -> String {
        format!(
            "rovar/media-upload/v1/{}/{}/{offset}",
            self.context, self.expected.length
        )
    }

    async fn read_part(&self, offset: u64) -> Result<Vec<u8>> {
        let path = self.directory.join(offset.to_string());
        let context = self.part_context(offset);
        let key = self.key.clone();
        let length = (self.expected.length - offset).min(CHUNK);
        let lock = self._lock.clone();
        Ok(
            tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
                let _lock = lock;
                let mut encrypted = Vec::new();
                File::open(path)?
                    .take(length + 97)
                    .read_to_end(&mut encrypted)?;
                anyhow::ensure!(
                    encrypted.len() as u64 == length + 96,
                    "Invalid upload part length"
                );
                let bytes = key.decrypt(&encrypted, context.as_bytes())?;
                anyhow::ensure!(bytes.len() as u64 == length, "Invalid upload part length");
                Ok(bytes)
            })
            .await
            .map_err(anyhow::Error::from)??,
        )
    }
}

#[async_trait]
impl MediaUpload for Upload {
    fn offset(&self) -> u64 {
        self.offset
    }

    async fn append(&mut self, offset: u64, bytes: Vec<u8>) -> Result<()> {
        if offset > self.offset || offset >= self.expected.length || !offset.is_multiple_of(CHUNK) {
            return Err(Error::MediaOffset);
        }
        if bytes.len() as u64 != (self.expected.length - offset).min(CHUNK) {
            return Err(Error::Invalid(
                "Incomplete or oversized media upload part".into(),
            ));
        }
        if offset < self.offset {
            if self.read_part(offset).await? != bytes {
                return Err(Error::MediaOffset);
            }
            return Ok(());
        }
        let context = self.part_context(offset);
        let key = self.key.clone();
        let directory = self.directory.clone();
        let count = bytes.len() as u64;
        let lock = self._lock.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let _lock = lock;
            let encrypted = key.encrypt(&bytes, context.as_bytes())?;
            let mut file = tempfile::Builder::new()
                .prefix(".part-")
                .tempfile_in(&directory)?;
            file.write_all(&encrypted)?;
            file.as_file().sync_all()?;
            file.persist(directory.join(offset.to_string()))?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
            Ok(())
        })
        .await
        .map_err(anyhow::Error::from)??;
        self.offset += count;
        Ok(())
    }

    async fn finish(&mut self) -> Result<String> {
        if self.offset != self.expected.length {
            return Err(Error::MediaOffset);
        }
        let mut writer = Box::new(Writer::create(&self.blobs, &self.key, &self.context).await?);
        let mut digest = Sha256::new();
        let mut offset = 0;
        while offset < self.expected.length {
            let bytes = match self.read_part(offset).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.discard().await?;
                    return Err(error);
                }
            };
            digest.update(&bytes);
            offset += bytes.len() as u64;
            writer.write(&bytes).await?;
        }
        if format!("{:x}", digest.finalize()) != self.expected.hash {
            self.discard().await?;
            return Err(Error::Invalid(
                "Media checksum mismatch; upload must restart".into(),
            ));
        }
        writer.finish().await
    }

    async fn discard(&mut self) -> Result<()> {
        let directory = self.directory.clone();
        let lock = self._lock.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let _lock = lock;
            match std::fs::remove_dir_all(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            #[cfg(unix)]
            File::open(directory.parent().unwrap())?.sync_all()?;
            Ok(())
        })
        .await
        .map_err(anyhow::Error::from)??;
        self.offset = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
