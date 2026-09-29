use super::ports::{ContentStorage, Documents, Preparation};
use crate::domain::{
    document::{Document, DocumentSnapshot, SaveDocument},
    error::Result,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct DocumentService {
    documents: Arc<dyn Documents>,
    storage: Arc<dyn ContentStorage>,
    transfers: Semaphore,
}

impl DocumentService {
    pub fn new(documents: Arc<dyn Documents>, storage: Arc<dyn ContentStorage>) -> Self {
        Self {
            documents,
            storage,
            transfers: Semaphore::new(2),
        }
    }

    pub async fn list(&self, actor: &str, space: &str) -> Result<Vec<Document>> {
        self.documents.list(actor, space).await
    }

    pub async fn read(&self, actor: &str, space: &str, id: &str) -> Result<DocumentSnapshot> {
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
        })
    }

    pub async fn save(
        &self,
        actor: &str,
        space: &str,
        mut command: SaveDocument,
    ) -> Result<Document> {
        command.validate()?;
        let _permit = self
            .transfers
            .acquire()
            .await
            .map_err(anyhow::Error::from)?;
        let fingerprint = fingerprint(&command);
        match self
            .documents
            .prepare(actor, space, &command, fingerprint)
            .await?
        {
            Preparation::AlreadyCommitted(document) => Ok(document),
            Preparation::Write(write) => {
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
    hash.update([command.kind as u8, u8::from(command.deleted)]);
    hash.update((command.title.len() as u64).to_le_bytes());
    hash.update(command.title.as_bytes());
    hash.update(&command.content);
    hash.finalize().to_vec()
}
