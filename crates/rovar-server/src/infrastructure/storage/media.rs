use super::crypto::{
    StorageKey,
    stream::{CHUNK_BYTES, Decryptor, Encryptor, HEADER_BYTES, TAG_BYTES},
};
use crate::{
    application::ports::MediaWriter,
    domain::{document::Media, error},
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

pub(super) struct Writer {
    file: File,
    temporary: tempfile::NamedTempFile,
    directory: PathBuf,
    cipher: Encryptor,
    pending: Vec<u8>,
    length: u64,
}

impl Writer {
    pub async fn create(directory: &Path, key: &StorageKey, context: &str) -> Result<Self> {
        let temporary = tempfile::Builder::new()
            .prefix(".upload-")
            .tempfile_in(directory)?;
        let mut file = File::from_std(temporary.reopen()?);
        let (cipher, header) = key.media_encryptor(context.as_bytes())?;
        file.write_all(&header).await?;
        Ok(Self {
            file,
            temporary,
            directory: directory.into(),
            cipher,
            pending: Vec::with_capacity(CHUNK_BYTES),
            length: 0,
        })
    }

    async fn flush_chunk(&mut self) -> Result<()> {
        if !self.pending.is_empty() {
            let encrypted = self.cipher.next(&self.pending)?;
            self.file.write_u32(encrypted.len() as u32).await?;
            self.file.write_all(&encrypted).await?;
            self.pending.clear();
        }
        Ok(())
    }
}

#[async_trait]
impl MediaWriter for Writer {
    async fn write(&mut self, mut bytes: &[u8]) -> error::Result<()> {
        if bytes.len() as u64 > (rovar_api::MAX_MEDIA_BYTES as u64).saturating_sub(self.length) {
            return Err(error::Error::Invalid("Media exceeds the size limit".into()));
        }
        self.length += bytes.len() as u64;
        while !bytes.is_empty() {
            let n = bytes.len().min(CHUNK_BYTES - self.pending.len());
            self.pending.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.pending.len() == CHUNK_BYTES {
                self.flush_chunk().await?;
            }
        }
        Ok(())
    }

    async fn finish(mut self: Box<Self>) -> error::Result<String> {
        self.flush_chunk().await?;
        let Self {
            mut file,
            temporary,
            directory,
            cipher,
            ..
        } = *self;
        let last = cipher.finish()?;
        file.write_u32(last.len() as u32)
            .await
            .map_err(anyhow::Error::from)?;
        file.write_all(&last).await.map_err(anyhow::Error::from)?;
        file.sync_all().await.map_err(anyhow::Error::from)?;
        drop(file);
        Ok(tokio::task::spawn_blocking(move || -> Result<String> {
            let blob = uuid::Uuid::new_v4().to_string();
            temporary.persist_noclobber(directory.join(&blob))?;
            #[cfg(unix)]
            std::fs::File::open(directory)?.sync_all()?;
            Ok(blob)
        })
        .await
        .map_err(anyhow::Error::from)??)
    }
}

pub(super) struct Reader {
    file: File,
    cipher: Option<Decryptor>,
    expected: Media,
    length: u64,
    digest: Sha256,
}

impl Reader {
    pub async fn open(
        path: &Path,
        key: &StorageKey,
        context: &str,
        expected: Media,
    ) -> Result<Self> {
        ensure!(
            expected.length <= rovar_api::MAX_MEDIA_BYTES as u64,
            "Stored media exceeds the size limit"
        );
        let mut file = File::open(path).await?;
        let mut header = [0; HEADER_BYTES];
        file.read_exact(&mut header).await?;
        let cipher = key.media_decryptor(&header, context.as_bytes())?;
        let mut reader = Self {
            file,
            cipher: Some(cipher),
            expected,
            length: 0,
            digest: Sha256::new(),
        };
        if reader.expected.length == 0 {
            reader.finish().await?;
        }
        Ok(reader)
    }

    async fn segment(&mut self) -> Result<Vec<u8>> {
        let length = self.file.read_u32().await? as usize;
        ensure!(
            (TAG_BYTES..=CHUNK_BYTES + TAG_BYTES).contains(&length),
            "Invalid encrypted media segment length"
        );
        let mut bytes = vec![0; length];
        self.file.read_exact(&mut bytes).await?;
        Ok(bytes)
    }

    async fn finish(&mut self) -> Result<()> {
        let last = self.segment().await?;
        self.cipher
            .take()
            .ok_or_else(|| anyhow::anyhow!("Media already completed"))?
            .finish(&last)?;
        ensure!(
            self.file
                .read_u8()
                .await
                .err()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::UnexpectedEof),
            "Trailing data after encrypted media"
        );
        ensure!(
            self.length == self.expected.length
                && format!("{:x}", self.digest.clone().finalize()) == self.expected.hash,
            "Stored media checksum mismatch"
        );
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Option<Vec<u8>>> {
        if self.cipher.is_none() {
            return Ok(None);
        }
        let encrypted = self.segment().await?;
        let bytes = self.cipher.as_mut().unwrap().next(&encrypted)?;
        ensure!(
            bytes.len() as u64 <= self.expected.length.saturating_sub(self.length),
            "Stored media length mismatch"
        );
        self.digest.update(&bytes);
        self.length += bytes.len() as u64;
        // Authenticate the terminator and full hash before releasing the final
        // bytes, so a Content-Length response cannot complete on corrupt data.
        if self.length == self.expected.length {
            self.finish().await?;
        }
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests;
