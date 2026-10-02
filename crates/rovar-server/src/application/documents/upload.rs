use super::*;
use crate::domain::{
    document::{Media, validate_media},
    error::Error,
};
use futures_util::{Stream, StreamExt};

impl DocumentService {
    async fn uploaded(&self, actor: &str, space: &str, item: &Media) -> Result<bool> {
        validate_media(std::slice::from_ref(item), 0)?;
        let _lease = self.storage.lease().await?;
        match self.documents.media(actor, space, &item.hash).await? {
            Some((_, length)) if length != item.length => Err(Error::Invalid(
                "Media length does not match its hash".into(),
            )),
            stored => Ok(stored.is_some()),
        }
    }

    pub async fn media_upload_status(
        &self,
        actor: &str,
        space: &str,
        item: Media,
    ) -> Result<rovar_api::MediaUpload> {
        if self.uploaded(actor, space, &item).await? {
            return Ok(complete(item.length));
        }
        let _permit = self
            .media_transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let mut upload = self
            .storage
            .media_upload(format!("{space}/media/{}", item.hash), item.clone())
            .await?;
        // Another request may have published while we waited for capacity or
        // the staging lock. Its committed record wins over any staged offset.
        if self.uploaded(actor, space, &item).await? {
            let _ = upload.discard().await;
            return Ok(complete(item.length));
        }
        Ok(rovar_api::MediaUpload {
            offset: upload.offset(),
            complete: false,
        })
    }

    pub async fn upload_media_chunk<S, B>(
        &self,
        actor: &str,
        space: &str,
        item: Media,
        offset: u64,
        body: S,
    ) -> Result<rovar_api::MediaUpload>
    where
        S: Stream<Item = std::io::Result<B>>,
        B: AsRef<[u8]>,
    {
        // Authorization and the published-state check precede reading any body.
        if self.uploaded(actor, space, &item).await? {
            return Ok(complete(item.length));
        }
        if offset >= item.length
            || !offset.is_multiple_of(rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64)
        {
            return Err(Error::MediaOffset);
        }
        let _permit = self
            .media_transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let expected_length =
            (item.length - offset).min(rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64) as usize;
        let mut upload = self
            .storage
            .media_upload(format!("{space}/media/{}", item.hash), item.clone())
            .await?;
        if self.uploaded(actor, space, &item).await? {
            let _ = upload.discard().await;
            return Ok(complete(item.length));
        }
        if offset > upload.offset() {
            return Err(Error::MediaOffset);
        }
        let mut bytes = Vec::new();
        futures_util::pin_mut!(body);
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|_| Error::Invalid("Incomplete media upload part".into()))?;
            let chunk = chunk.as_ref();
            if chunk.len() > expected_length.saturating_sub(bytes.len()) {
                return Err(Error::Invalid("Oversized media upload part".into()));
            }
            bytes.extend_from_slice(chunk);
        }
        upload.append(offset, bytes).await?;
        Ok(rovar_api::MediaUpload {
            offset: upload.offset(),
            complete: false,
        })
    }

    pub async fn finish_media_upload(
        &self,
        actor: &str,
        space: &str,
        item: Media,
    ) -> Result<rovar_api::MediaUpload> {
        let _lease = self.storage.lease().await?;
        if self.uploaded(actor, space, &item).await? {
            return Ok(complete(item.length));
        }
        let _permit = self
            .media_transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let mut upload = self
            .storage
            .media_upload(format!("{space}/media/{}", item.hash), item.clone())
            .await?;
        if self.uploaded(actor, space, &item).await? {
            let _ = upload.discard().await;
            return Ok(complete(item.length));
        }
        let blob = upload.finish().await?;
        self.documents
            .store_media(actor, space, &item, &blob)
            .await?;
        // The database acknowledgement is authoritative even if staging cleanup
        // fails. Replaying completion must not publish another resource.
        if let Err(error) = upload.discard().await {
            eprintln!("Media upload staging cleanup failed: {error}");
        }
        Ok(complete(item.length))
    }
}

fn complete(length: u64) -> rovar_api::MediaUpload {
    rovar_api::MediaUpload {
        offset: length,
        complete: true,
    }
}
