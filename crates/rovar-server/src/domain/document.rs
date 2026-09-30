use super::error::{Error, Result};

pub const MAX_CONTENT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Document,
    Component,
    ColorStyle,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    pub kind: DocumentKind,
    pub title: String,
    pub revision: i64,
    pub created: u64,
    pub modified: u64,
    pub deleted: bool,
}

/// The application command carries bytes, not HTTP/base64 representations.
pub struct SaveDocument {
    pub id: String,
    pub kind: DocumentKind,
    pub title: String,
    pub base_revision: i64,
    pub request_id: String,
    pub content: Vec<u8>,
    pub deleted: bool,
}

impl SaveDocument {
    pub fn validate(&self) -> Result<()> {
        if uuid::Uuid::parse_str(&self.id).is_err()
            || uuid::Uuid::parse_str(&self.request_id).is_err()
            || self.title.trim().is_empty()
            || self.title.len() > 512
            || self.base_revision < 0
            || self.base_revision == i64::MAX
            || self.content.len() > MAX_CONTENT_BYTES
            || (!self.deleted && self.content.is_empty())
        {
            return Err(Error::Invalid("Invalid document metadata or size".into()));
        }
        Ok(())
    }
}

pub struct DocumentSnapshot {
    pub document: Document,
    pub content: Vec<u8>,
}

pub struct StoredVersion {
    pub document: Document,
    pub blob: String,
}
