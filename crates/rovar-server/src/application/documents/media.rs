use super::*;
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
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let bytes = collect_media(body, hash, rovar_api::MAX_MEDIA_BYTES).await?;
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

// The encrypted blob store still requires a contiguous buffer. Bound it before
// extending it; callers acquire a transfer permit before polling any body data.
async fn collect_media<S, B>(body: S, hash: &str, maximum: usize) -> Result<Vec<u8>>
where
    S: Stream<Item = std::io::Result<B>>,
    B: AsRef<[u8]>,
{
    let mut bytes = Vec::new();
    let mut digest = Sha256::new();
    futures_util::pin_mut!(body);
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| Error::Invalid("Incomplete media upload".into()))?;
        let chunk = chunk.as_ref();
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(Error::Invalid("Media exceeds the size limit".into()));
        }
        digest.update(chunk);
        bytes.extend_from_slice(chunk);
    }
    if format!("{:x}", digest.finalize()) != hash {
        return Err(Error::Invalid("Invalid media checksum".into()));
    }
    Ok(bytes)
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
        let hash = format!("{:x}", Sha256::digest(b"abcdef"));
        let chunks = || stream::iter([Ok(b"abc"), Ok(b"def")]);
        assert_eq!(collect_media(chunks(), &hash, 6).await.unwrap(), b"abcdef");
        assert!(
            collect_media(chunks(), &hash, 5)
                .await
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        assert!(
            collect_media(stream::iter([Ok(b"abc")]), &hash, 6)
                .await
                .unwrap_err()
                .to_string()
                .contains("checksum")
        );
        let interrupted = stream::iter([Ok(b"abc"), Err(std::io::ErrorKind::UnexpectedEof.into())]);
        assert!(
            collect_media(interrupted, &hash, 6)
                .await
                .unwrap_err()
                .to_string()
                .contains("Incomplete")
        );
    }
}
