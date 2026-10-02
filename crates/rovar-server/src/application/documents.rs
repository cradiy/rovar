use super::ports::{ContentStorage, Documents, Preparation};
use crate::domain::{
    document::{Document, DocumentSnapshot, SaveDocument},
    error::Result,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Semaphore;
mod delta;
mod media;
#[cfg(test)]
mod tests;

pub struct DocumentService {
    documents: Arc<dyn Documents>,
    storage: Arc<dyn ContentStorage>,
    transfers: Semaphore,
    media_transfers: Arc<Semaphore>,
}

impl DocumentService {
    pub async fn changes(
        &self,
        actor: &str,
        space: &str,
        after: i64,
    ) -> Result<crate::domain::document::Changes> {
        self.documents.changes(actor, space, after).await
    }

    pub async fn metadata(&self, actor: &str, space: &str, id: &str) -> Result<Document> {
        self.documents.metadata(actor, space, id).await
    }
    pub fn new(documents: Arc<dyn Documents>, storage: Arc<dyn ContentStorage>) -> Self {
        Self {
            documents,
            storage,
            transfers: Semaphore::new(2),
            media_transfers: Arc::new(Semaphore::new(2)),
        }
    }

    pub async fn list(&self, actor: &str, space: &str) -> Result<Vec<Document>> {
        self.documents.list(actor, space).await
    }

    pub async fn read(&self, actor: &str, space: &str, id: &str) -> Result<DocumentSnapshot> {
        let snapshot = self.transfer(actor, space, id).await?;
        self.expand_media(actor, space, snapshot).await
    }

    pub async fn transfer(&self, actor: &str, space: &str, id: &str) -> Result<DocumentSnapshot> {
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let version = self.documents.current(actor, space, id).await?;
        let context = format!("{space}/{id}/{}", version.document.revision);
        let bytes = self.storage.read(&version.blob, context).await?;
        Ok(DocumentSnapshot {
            document: version.document,
            content: bytes,
            media: version.media,
        })
    }

    pub async fn save(&self, actor: &str, space: &str, command: SaveDocument) -> Result<Document> {
        self.save_encoded(actor, space, command, false).await
    }

    pub async fn save_delta(
        &self,
        actor: &str,
        space: &str,
        command: SaveDocument,
    ) -> Result<Document> {
        self.save_encoded(actor, space, command, true).await
    }

    async fn save_encoded(
        &self,
        actor: &str,
        space: &str,
        mut command: SaveDocument,
        delta: bool,
    ) -> Result<Document> {
        command.validate_payload(delta)?;
        if delta
            && (command.deleted
                || command.base_revision == 0
                || command.kind == crate::domain::document::DocumentKind::ColorStyle)
        {
            return Err(crate::domain::error::Error::Invalid(
                "Delta requires a live document baseline".into(),
            ));
        }
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        if !delta && !command.media.is_empty() {
            media::validate_container(command.content.clone()).await?;
        }
        let mut fingerprint = fingerprint(&command);
        if delta {
            let mut hash = Sha256::new();
            hash.update(b"rovar/delta-request/v1");
            hash.update(fingerprint);
            fingerprint = hash.finalize().to_vec();
        }
        match self
            .documents
            .prepare(actor, space, &command, fingerprint)
            .await?
        {
            Preparation::AlreadyCommitted(document) => Ok(document),
            Preparation::Write(mut write) => {
                if delta {
                    let blob = write.base_blob().await?;
                    let context = format!("{space}/{}/{}", command.id, command.base_revision);
                    let base = self.storage.read(&blob, context).await?;
                    command.content =
                        delta::expand(base, std::mem::take(&mut command.content)).await?;
                    command.validate()?;
                }
                let context = format!("{space}/{}/{}", command.id, write.revision());
                let blob = self
                    .storage
                    .write(std::mem::take(&mut command.content), context)
                    .await?;
                // Retain content on ambiguous commit errors: the transaction may
                // already have committed even when its acknowledgement was lost.
                write.commit(&command, &blob).await
            }
        }
    }
}

fn fingerprint(command: &SaveDocument) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(command.base_revision.to_le_bytes());
    // Keep existing full-snapshot retry fingerprints stable; detached media has
    // a distinct domain and explicit lengths to avoid ambiguous concatenation.
    let kind = command.kind as u8 | if command.media.is_empty() { 0 } else { 0x80 };
    hash.update([kind, u8::from(command.deleted)]);
    hash.update((command.title.len() as u64).to_le_bytes());
    hash.update(command.title.as_bytes());
    if !command.media.is_empty() {
        hash.update((command.content.len() as u64).to_le_bytes());
        hash.update((command.media.len() as u64).to_le_bytes());
    }
    hash.update(&command.content);
    for item in &command.media {
        hash.update(item.hash.as_bytes());
        hash.update(item.length.to_le_bytes());
    }
    hash.finalize().to_vec()
}
