mod crypto;
mod media;
mod upload;

use crate::{application::ports::ContentStorage, domain::error};
use anyhow::Result;
use async_trait::async_trait;
use crypto::StorageKey;
use std::path::{Path, PathBuf};

pub struct ContentStore {
    directory: PathBuf,
    uploads: PathBuf,
    key: StorageKey,
}

impl ContentStore {
    #[cfg(test)]
    pub async fn create_media(
        &self,
        context: String,
    ) -> error::Result<Box<dyn crate::application::ports::MediaWriter>> {
        Ok(Box::new(
            media::Writer::create(&self.directory, &self.key, &context).await?,
        ))
    }
    pub fn open(root: &Path) -> Result<Self> {
        let key = StorageKey::load(root)?;
        let directory = root.join("blobs");
        std::fs::create_dir_all(&directory)?;
        let uploads = root.join("uploads");
        std::fs::create_dir_all(&uploads)?;
        Ok(Self {
            directory,
            uploads,
            key,
        })
    }
}

#[async_trait]
impl ContentStorage for ContentStore {
    async fn media_upload(
        &self,
        context: String,
        expected: crate::domain::document::Media,
    ) -> error::Result<Box<dyn crate::application::ports::MediaUpload>> {
        let root = self.uploads.clone();
        let blobs = self.directory.clone();
        let key = self.key.clone();
        Ok(Box::new(
            tokio::task::spawn_blocking(move || {
                upload::Upload::open(root, blobs, key, context, expected)
            })
            .await
            .map_err(anyhow::Error::from)??,
        ))
    }
    async fn read_media(
        &self,
        blob: &str,
        context: String,
        expected: crate::domain::document::Media,
    ) -> error::Result<crate::application::ports::ContentStream> {
        uuid::Uuid::parse_str(blob).map_err(anyhow::Error::from)?;
        let reader =
            media::Reader::open(&self.directory.join(blob), &self.key, &context, expected).await?;
        Ok(Box::pin(futures_util::stream::try_unfold(
            reader,
            |mut reader| async move {
                reader
                    .next()
                    .await
                    .map(|bytes| bytes.map(|bytes| (bytes, reader)))
                    .map_err(std::io::Error::other)
            },
        )))
    }

    async fn write(&self, bytes: Vec<u8>, context: String) -> error::Result<String> {
        let key = self.key.clone();
        let directory = self.directory.clone();
        Ok(tokio::task::spawn_blocking(move || -> Result<String> {
            use std::io::Write;
            let blob = uuid::Uuid::new_v4().to_string();
            let output = directory.join(&blob);
            let encrypted = key.encrypt(&bytes, context.as_bytes())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&encrypted)?;
            file.sync_all()?;
            #[cfg(unix)]
            std::fs::File::open(directory)?.sync_all()?;
            Ok(blob)
        })
        .await
        .map_err(anyhow::Error::from)??)
    }

    async fn read(&self, blob: &str, context: String) -> error::Result<Vec<u8>> {
        uuid::Uuid::parse_str(blob).map_err(anyhow::Error::from)?;
        let bytes = tokio::fs::read(self.directory.join(blob))
            .await
            .map_err(anyhow::Error::from)?;
        let key = self.key.clone();
        Ok(
            tokio::task::spawn_blocking(move || key.decrypt(&bytes, context.as_bytes()))
                .await
                .map_err(anyhow::Error::from)??,
        )
    }
}
