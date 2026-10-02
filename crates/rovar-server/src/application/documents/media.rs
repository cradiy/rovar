use super::*;
use crate::application::ports::MediaDownload;
use crate::domain::{
    document::{Media, valid_hash, validate_media},
    error::Error,
};
use futures_util::StreamExt;

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

    pub async fn download_media(
        &self,
        actor: &str,
        space: &str,
        hash: &str,
    ) -> Result<MediaDownload> {
        if !valid_hash(hash) {
            return Err(Error::Invalid("Invalid media hash".into()));
        }
        let permit = self
            .media_transfers
            .clone()
            .acquire_owned()
            .await
            .map_err(anyhow::Error::from)?;
        let (blob, length) = self
            .documents
            .media(actor, space, hash)
            .await?
            .ok_or(Error::NotFound)?;
        let body = self
            .storage
            .read_media(
                &blob,
                format!("{space}/media/{hash}"),
                Media {
                    hash: hash.into(),
                    length,
                },
            )
            .await?;
        // Keep capacity reserved until the body completes or the client drops it.
        Ok(MediaDownload {
            length,
            body: Box::pin(futures_util::stream::unfold(
                (body, permit),
                |(mut body, permit)| async move {
                    body.next().await.map(|chunk| (chunk, (body, permit)))
                },
            )),
        })
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
        let content = std::mem::take(&mut snapshot.content);
        let (file, mut writer) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let file = tempfile::NamedTempFile::new()?;
            std::fs::write(file.path(), content)?;
            let writer = rovar_format::Writer::open(file.path())?;
            Ok((file, writer))
        })
        .await
        .map_err(anyhow::Error::from)??;
        for item in &snapshot.media {
            let mut download = self.download_media(actor, space, &item.hash).await?;
            let mut bytes = Vec::new();
            while let Some(chunk) = download.body.next().await {
                bytes.extend_from_slice(&chunk.map_err(anyhow::Error::from)?);
            }
            let hash = item.hash.clone();
            writer = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                writer.put_bytes(&format!("media/{hash}"), "media", &bytes)?;
                Ok(writer)
            })
            .await
            .map_err(anyhow::Error::from)??;
        }
        snapshot.content = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
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
