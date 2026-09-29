use crate::domain::{
    document::{Document, SaveDocument, StoredVersion},
    error::Result,
    identity::User,
};
use async_trait::async_trait;

#[async_trait]
pub trait Accounts: Send + Sync {
    async fn create_user(&self, username: &str, hash: &str, team: Option<&str>) -> Result<()>;
    async fn find_user(&self, username: &str) -> Result<Option<User>>;
    async fn create_session(&self, user_id: &str, hash: &[u8], expires_at: i64) -> Result<()>;
    async fn session_user(&self, hash: &[u8], now: i64) -> Result<Option<(String, String)>>;
    async fn delete_session(&self, hash: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait Passwords: Send + Sync {
    async fn hash(&self, password: String) -> Result<String>;
    async fn verify(&self, password: String, hash: String) -> Result<bool>;
}

#[async_trait]
pub trait Documents: Send + Sync {
    async fn list(&self, actor: &str, space: &str) -> Result<Vec<Document>>;
    async fn current(&self, actor: &str, space: &str, id: &str) -> Result<StoredVersion>;
    async fn prepare(
        &self,
        actor: &str,
        space: &str,
        command: &SaveDocument,
        fingerprint: Vec<u8>,
    ) -> Result<Preparation>;
}

pub enum Preparation {
    AlreadyCommitted(Document),
    Write(Box<dyn DocumentWrite>),
}

/// The adapter owns locking and rollback; the application sees a write permit.
#[async_trait]
pub trait DocumentWrite: Send {
    fn revision(&self) -> i64;
    async fn commit(self: Box<Self>, command: &SaveDocument, blob: &str) -> Result<Document>;
}

#[async_trait]
pub trait ContentStorage: Send + Sync {
    async fn write(&self, bytes: Vec<u8>, context: String) -> Result<String>;
    async fn read(&self, blob: &str, context: String) -> Result<Vec<u8>>;
}
#[async_trait]
pub trait Spaces: Send + Sync {
    async fn list(&self, actor: &str) -> Result<Vec<crate::domain::space::Space>>;
    async fn create(&self, actor: &str, name: &str) -> Result<crate::domain::space::Space>;
    async fn invite(&self, actor: &str, space: &str, hash: &[u8], expires: i64) -> Result<()>;
    async fn join(&self, actor: &str, hash: &[u8]) -> Result<crate::domain::space::Space>;
    async fn members(&self, actor: &str, space: &str) -> Result<Vec<crate::domain::space::Member>>;
    async fn remove(&self, actor: &str, space: &str, user: &str) -> Result<()>;
}
