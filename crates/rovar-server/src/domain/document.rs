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
    pub media: Vec<Media>,
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
        validate_media(&self.media, self.content.len())?;
        if self.deleted && !self.media.is_empty() {
            return Err(Error::Invalid(
                "Deleted documents cannot reference media".into(),
            ));
        }
        Ok(())
    }
}

pub struct DocumentSnapshot {
    pub document: Document,
    pub content: Vec<u8>,
    pub media: Vec<Media>,
}

pub struct DocumentTransfer {
    pub snapshot: DocumentSnapshot,
    pub delta: bool,
}

pub struct StoredVersion {
    pub document: Document,
    pub blob: String,
    pub media: Vec<Media>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Media {
    pub hash: String,
    pub length: u64,
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn validate_media(media: &[Media], content_length: usize) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    let mut total = content_length as u64;
    for item in media {
        if !valid_hash(&item.hash)
            || !seen.insert(&item.hash)
            || item.length > MAX_CONTENT_BYTES as u64
        {
            return Err(Error::Invalid("Invalid media reference".into()));
        }
        total = total.saturating_add(item.length);
    }
    if media.len() > 4096 || total > MAX_CONTENT_BYTES as u64 {
        return Err(Error::Invalid("Document exceeds the size limit".into()));
    }
    Ok(())
}

pub struct Changes {
    pub documents: Vec<Document>,
    pub cursor: i64,
    pub has_more: bool,
}
