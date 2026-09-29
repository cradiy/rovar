mod crypto;

use crate::{application::ports::ContentStorage, domain::error};
use anyhow::Result;
use async_trait::async_trait;
use crypto::StorageKey;
use std::path::{Path, PathBuf};

pub struct ContentStore {
    directory: PathBuf,
    key: StorageKey,
}

impl ContentStore {
    pub fn open(root: &Path) -> Result<Self> {
        let key = StorageKey::load(root)?;
        let directory = root.join("blobs");
        std::fs::create_dir_all(&directory)?;
        Ok(Self { directory, key })
    }
}

#[async_trait]
impl ContentStorage for ContentStore {
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
