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
    async fn create_session(
        &self,
        user_id: &str,
        hash: &[u8],
        expires_at: i64,
        password_hash: &str,
        device: &crate::domain::identity::SessionDevice,
    ) -> Result<()>;
    async fn session_user(&self, hash: &[u8], now: i64) -> Result<Option<(String, String)>>;
    async fn delete_session(&self, hash: &[u8]) -> Result<()>;
    async fn sessions(
        &self,
        user: &str,
        current: &[u8],
    ) -> Result<Vec<crate::domain::identity::AccountSession>>;
    async fn revoke_sessions(&self, user: &str, current: &[u8], id: Option<&str>) -> Result<()>;
    async fn change_password(
        &self,
        user: &str,
        current: &[u8],
        previous: &str,
        next: &str,
    ) -> Result<()>;
}

#[async_trait]
pub trait Passwords: Send + Sync {
    async fn hash(&self, password: String) -> Result<String>;
    async fn verify(&self, password: String, hash: String) -> Result<bool>;
}

#[async_trait]
pub trait Documents: Send + Sync {
    async fn media_lengths(
        &self,
        actor: &str,
        space: &str,
        hashes: &[String],
    ) -> Result<std::collections::BTreeMap<String, u64>>;
    async fn media(&self, actor: &str, space: &str, hash: &str) -> Result<Option<(String, u64)>>;
    async fn store_media(
        &self,
        actor: &str,
        space: &str,
        media: &crate::domain::document::Media,
        blob: &str,
    ) -> Result<()>;
    async fn changes(
        &self,
        actor: &str,
        space: &str,
        after: i64,
    ) -> Result<crate::domain::document::Changes>;
    async fn metadata(&self, actor: &str, space: &str, id: &str) -> Result<Document>;
    async fn list(&self, actor: &str, space: &str) -> Result<Vec<Document>>;
    async fn current(&self, actor: &str, space: &str, id: &str) -> Result<StoredVersion>;
    async fn version_blob(
        &self,
        actor: &str,
        space: &str,
        id: &str,
        revision: i64,
    ) -> Result<Option<String>>;
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
    /// The confirmed base blob, read under the same lock as the pending commit.
    async fn base_blob(&mut self) -> Result<String>;
    async fn commit(self: Box<Self>, command: &SaveDocument, blob: &str) -> Result<Document>;
}

#[async_trait]
pub trait ContentStorage: Send + Sync {
    async fn write(&self, bytes: Vec<u8>, context: String) -> Result<String>;
    async fn read(&self, blob: &str, context: String) -> Result<Vec<u8>>;
    async fn media_upload(
        &self,
        context: String,
        expected: crate::domain::document::Media,
    ) -> Result<Box<dyn MediaUpload>>;
    async fn read_media(
        &self,
        blob: &str,
        context: String,
        expected: crate::domain::document::Media,
    ) -> Result<ContentStream>;
}

/// Exclusive access to durable encrypted upload parts. Dropping the handle
/// releases its lock while retaining acknowledged progress for later requests.
#[async_trait]
pub trait MediaUpload: Send {
    fn offset(&self) -> u64;
    async fn append(&mut self, offset: u64, bytes: Vec<u8>) -> Result<()>;
    async fn finish(&mut self) -> Result<String>;
    async fn discard(&mut self) -> Result<()>;
}

/// Dropping an unfinished writer must discard its unpublished encrypted data.
#[async_trait]
pub trait MediaWriter: Send {
    async fn write(&mut self, bytes: &[u8]) -> Result<()>;
    async fn finish(self: Box<Self>) -> Result<String>;
}

pub type ContentStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = std::io::Result<Vec<u8>>> + Send>>;

pub struct MediaDownload {
    pub length: u64,
    pub body: ContentStream,
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
