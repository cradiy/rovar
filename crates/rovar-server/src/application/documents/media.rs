use super::*;
use crate::application::ports::{MediaDownload, MediaWriter};
use crate::domain::{
    document::{Media, valid_hash, validate_media},
    error::Error,
};
use futures_util::{Stream, StreamExt};

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

    pub async fn upload_media<S, B>(
        &self,
        actor: &str,
        space: &str,
        hash: &str,
        body: S,
    ) -> Result<()>
    where
        S: Stream<Item = std::io::Result<B>>,
        B: AsRef<[u8]>,
    {
        if !valid_hash(hash) {
            return Err(Error::Invalid("Invalid media hash".into()));
        }
        // Authorize before reading the body or allocating its buffer.
        if self.documents.media(actor, space, hash).await?.is_some() {
            return Ok(());
        }
        let _permit = self
            .media_transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let mut writer = self
            .storage
            .create_media(format!("{space}/media/{hash}"))
            .await?;
        let length = write_media(body, hash, rovar_api::MAX_MEDIA_BYTES, writer.as_mut()).await?;
        let item = Media {
            hash: hash.into(),
            length,
        };
        let blob = writer.finish().await?;
        self.documents.store_media(actor, space, &item, &blob).await
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

async fn write_media<S, B>(
    body: S,
    hash: &str,
    maximum: usize,
    writer: &mut dyn MediaWriter,
) -> Result<u64>
where
    S: Stream<Item = std::io::Result<B>>,
    B: AsRef<[u8]>,
{
    let mut length = 0u64;
    let mut digest = Sha256::new();
    futures_util::pin_mut!(body);
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| Error::Invalid("Incomplete media upload".into()))?;
        let chunk = chunk.as_ref();
        if chunk.len() as u64 > (maximum as u64).saturating_sub(length) {
            return Err(Error::Invalid("Media exceeds the size limit".into()));
        }
        digest.update(chunk);
        writer.write(chunk).await?;
        length += chunk.len() as u64;
    }
    if format!("{:x}", digest.finalize()) != hash {
        return Err(Error::Invalid("Invalid media checksum".into()));
    }
    Ok(length)
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn media_size_is_bounded_across_chunks_before_checksum_validation() {
        use crate::{application::ports::ContentStorage, infrastructure::storage::ContentStore};
        let root = tempfile::tempdir().unwrap();
        let storage = ContentStore::open(root.path()).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"abcdef"));
        let chunks = || stream::iter([Ok(b"abc"), Ok(b"def")]);
        let mut writer = storage.create_media("test".into()).await.unwrap();
        assert_eq!(
            write_media(chunks(), &hash, 6, writer.as_mut())
                .await
                .unwrap(),
            6
        );
        drop(writer);
        let mut writer = storage.create_media("test".into()).await.unwrap();
        assert!(
            write_media(chunks(), &hash, 5, writer.as_mut())
                .await
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        drop(writer);
        let mut writer = storage.create_media("test".into()).await.unwrap();
        assert!(
            write_media(stream::iter([Ok(b"abc")]), &hash, 6, writer.as_mut())
                .await
                .unwrap_err()
                .to_string()
                .contains("checksum")
        );
        drop(writer);
        let mut writer = storage.create_media("test".into()).await.unwrap();
        let interrupted = stream::iter([Ok(b"abc"), Err(std::io::ErrorKind::UnexpectedEof.into())]);
        assert!(
            write_media(interrupted, &hash, 6, writer.as_mut())
                .await
                .unwrap_err()
                .to_string()
                .contains("Incomplete")
        );
        drop(writer);
        assert_eq!(
            std::fs::read_dir(root.path().join("blobs"))
                .unwrap()
                .count(),
            0
        );
    }
}
