use super::*;
use crate::domain::{
    document::{Media, valid_hash, validate_media},
    error::Error,
};

impl DocumentService {
    pub async fn missing_media(
        &self,
        actor: &str,
        space: &str,
        media: &[Media],
    ) -> Result<Vec<String>> {
        validate_media(media, 0)?;
        // One authorized batch query, including empty probes.
        let hashes: Vec<_> = media.iter().map(|item| item.hash.clone()).collect();
        let lengths = self.documents.media_lengths(actor, space, &hashes).await?;
        let mut missing = Vec::new();
        for item in media {
            match lengths.get(&item.hash) {
                None => missing.push(item.hash.clone()),
                Some(length) if *length == item.length => {}
                Some(_) => {
                    return Err(Error::Invalid(
                        "Media length does not match its hash".into(),
                    ));
                }
            }
        }
        Ok(missing)
    }

    pub async fn upload_media(
        &self,
        actor: &str,
        space: &str,
        hash: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        if !valid_hash(hash)
            || bytes.len() > rovar_api::MAX_CONTENT_BYTES
            || format!("{:x}", Sha256::digest(&bytes)) != hash
        {
            return Err(Error::Invalid("Invalid media checksum or size".into()));
        }
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        if self.documents.media(actor, space, hash).await?.is_some() {
            return Ok(());
        }
        let item = Media {
            hash: hash.into(),
            length: bytes.len() as u64,
        };
        let blob = self
            .storage
            .write(bytes, format!("{space}/media/{hash}"))
            .await?;
        self.documents.store_media(actor, space, &item, &blob).await
    }

    pub async fn download_media(&self, actor: &str, space: &str, hash: &str) -> Result<Vec<u8>> {
        if !valid_hash(hash) {
            return Err(Error::Invalid("Invalid media hash".into()));
        }
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let (blob, length) = self
            .documents
            .media(actor, space, hash)
            .await?
            .ok_or(Error::NotFound)?;
        let bytes = self
            .storage
            .read(&blob, format!("{space}/media/{hash}"))
            .await?;
        if bytes.len() as u64 != length || format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(anyhow::anyhow!("Stored media checksum mismatch").into());
        }
        Ok(bytes)
    }

    pub(super) async fn expand_media(
        &self,
        actor: &str,
        space: &str,
        mut snapshot: DocumentSnapshot,
    ) -> Result<DocumentSnapshot> {
        if snapshot.media.is_empty() {
            return Ok(snapshot);
        }
        let mut media = Vec::new();
        for item in &snapshot.media {
            media.push((
                item.hash.clone(),
                self.download_media(actor, space, &item.hash).await?,
            ));
        }
        snapshot.content = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
            let file = tempfile::NamedTempFile::new()?;
            std::fs::write(file.path(), snapshot.content)?;
            let mut writer = rovar_format::Writer::open(file.path())?;
            for (hash, bytes) in media {
                writer.put_bytes(&format!("media/{hash}"), "media", &bytes)?;
            }
            writer.commit()?;
            drop(writer);
            Ok(std::fs::read(file.path())?)
        })
        .await
        .map_err(anyhow::Error::from)??;
        snapshot.media.clear();
        Ok(snapshot)
    }
}

pub(super) async fn validate_container(bytes: Vec<u8>) -> Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        std::fs::write(file.path(), bytes)?;
        let reader = rovar_format::Reader::open(file.path())?;
        anyhow::ensure!(
            reader
                .entries()
                .all(|(key, block)| !key.starts_with("media/") && block.kind != "media"),
            "Media must be referenced separately"
        );
        reader.verify()
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|_| Error::Invalid("Invalid document container".into()))
}
