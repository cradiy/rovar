use crate::domain::{
    document::{DocumentKind, DocumentSnapshot, DocumentTransfer},
    error::{Error, Result},
};
use rovar_api::MAX_METADATA_BYTES;

impl super::DocumentService {
    pub async fn download(
        &self,
        actor: &str,
        space: &str,
        id: &str,
        revision: i64,
        hash: [u8; 32],
    ) -> Result<DocumentTransfer> {
        if revision <= 0 {
            return Err(Error::Invalid("Invalid download baseline revision".into()));
        }
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let _lease = self.storage.lease().await?;
        let current = self.documents.current(actor, space, id).await?;
        let context = format!("{space}/{id}/{}", current.document.revision);
        let bytes = self.storage.read(&current.blob, context).await?;
        let mut snapshot = DocumentSnapshot {
            document: current.document,
            content: bytes,
            media: current.media,
        };
        let base = if snapshot.document.kind == DocumentKind::ColorStyle
            || revision > snapshot.document.revision
        {
            None
        } else if revision == snapshot.document.revision {
            Some(snapshot.content.clone())
        } else if let Some(blob) = self
            .documents
            .version_blob(actor, space, id, revision)
            .await?
        {
            // An expired or unavailable historical blob only removes the delta
            // optimization. Current content must still be readable and valid.
            self.storage
                .read(&blob, format!("{space}/{id}/{revision}"))
                .await
                .ok()
        } else {
            None
        };
        let Some(base) = base else {
            return Ok(DocumentTransfer {
                snapshot,
                delta: false,
            });
        };
        tokio::task::spawn_blocking(move || {
            let content = std::mem::take(&mut snapshot.content);
            let patch = download_delta(&base, &content, hash).ok().flatten();
            let delta = patch.is_some();
            snapshot.content = patch.unwrap_or(content);
            DocumentTransfer { snapshot, delta }
        })
        .await
        .map_err(|error| anyhow::Error::from(error).into())
    }
}

fn download_delta(base: &[u8], current: &[u8], hash: [u8; 32]) -> anyhow::Result<Option<Vec<u8>>> {
    let file = tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), current)?;
    let reader = rovar_format::Reader::open(file.path())?;
    // Legacy snapshots with inline media need the full container: those blobs
    // may not have been published to the detached-media store yet.
    if reader
        .entries()
        .any(|(key, block)| key.starts_with("media/") || block.kind == "media")
    {
        return Ok(None);
    }
    let base = rovar_format::delta::Snapshot::from_bytes(base, MAX_METADATA_BYTES)?;
    if base.hash()? != hash {
        return Ok(None);
    }
    let next = rovar_format::delta::Snapshot::read(&reader, MAX_METADATA_BYTES)?;
    let bytes = serde_json::to_vec(&base.difference(&next)?)?;
    Ok((bytes.len() <= rovar_api::MAX_DELTA_BYTES
        && bytes.len().saturating_add(256) < current.len())
    .then_some(bytes))
}

pub(super) async fn expand(base: Vec<u8>, delta: Vec<u8>) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let delta: rovar_format::delta::Delta = serde_json::from_slice(&delta)
            .map_err(|_| Error::Invalid("Invalid metadata delta".into()))?;
        let base = rovar_format::delta::Snapshot::from_bytes(&base, MAX_METADATA_BYTES)
            .map_err(|_| Error::DeltaBase)?;
        if base.hash()? != delta.base {
            return Err(Error::DeltaBase);
        }
        base.apply(&delta, MAX_METADATA_BYTES)
            .and_then(|snapshot| snapshot.to_bytes(MAX_METADATA_BYTES))
            .map_err(|_| Error::Invalid("Invalid metadata delta or result".into()))
    })
    .await
    .map_err(anyhow::Error::from)?
}
