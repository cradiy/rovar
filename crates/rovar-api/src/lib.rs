mod account;
mod spaces;
pub use account::*;
use serde::{Deserialize, Serialize};
pub use spaces::*;

pub const VERSION: u32 = 3;
// Independent resource budgets, not limits of the .rovar file format. Keep
// these conservative while snapshots and encrypted blobs still use buffers.
pub const MAX_METADATA_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_DELTA_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_MEDIA_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_MEDIA_REFERENCES: usize = 4096;
/// Durable upload units; HTTP requests and browser upload buffers stay bounded.
pub const MEDIA_UPLOAD_CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Base64 payload plus bounded JSON metadata and media references.
pub const MAX_DOCUMENT_REQUEST_BYTES: usize = MAX_METADATA_BYTES.div_ceil(3) * 4 + 512 * 1024;
pub const MAX_DELTA_REQUEST_BYTES: usize = MAX_DELTA_BYTES.div_ceil(3) * 4 + 512 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device: SessionDevice,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Identity {
    pub server_id: String,
    pub user_id: String,
    pub username: String,
    pub api_version: u32,
    pub spaces: Vec<Space>,
    pub registration: RegistrationPolicy,
}

#[derive(Serialize, Deserialize)]
pub struct Login {
    pub identity: Identity,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Document,
    Component,
    ColorStyle,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Object {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub revision: i64,
    pub created: u64,
    pub modified: u64,
    pub deleted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Save {
    pub kind: Kind,
    pub title: String,
    pub base_revision: i64,
    pub request_id: String,
    /// Base64-encoded document container or color-style JSON. When `media` is
    /// nonempty, its blocks are omitted from this container and referenced below.
    /// On PUT /objects/{id}/delta this instead contains a base64-encoded
    /// rovar-format metadata Delta; the other fields retain their meaning.
    pub content: String,
    #[serde(default)]
    pub media: Vec<Media>,
    pub deleted: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub object: Object,
    pub content: String,
    #[serde(default)]
    pub media: Vec<Media>,
}

/// POST /objects/{id}/transfer advertises an immutable confirmed metadata base.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadBase {
    pub revision: i64,
    pub hash: [u8; 32],
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferEncoding {
    #[default]
    Full,
    Delta,
}

#[derive(Serialize, Deserialize)]
pub struct Transfer {
    #[serde(flatten)]
    pub snapshot: Snapshot,
    /// Full responses share the GET snapshot representation. Delta content is
    /// a base64-encoded rovar-format Delta with base and result checksums.
    #[serde(default)]
    pub encoding: TransferEncoding,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Media {
    pub hash: String,
    pub length: u64,
}

#[derive(Serialize, Deserialize)]
pub struct MediaUpload {
    pub offset: u64,
    pub complete: bool,
}

/// Latest object states after a workspace-local, transactionally committed cursor.
/// Deleted objects remain in the feed. A cursor of zero starts a full scan.
#[derive(Serialize, Deserialize)]
pub struct Changes {
    pub objects: Vec<Object>,
    pub cursor: i64,
    pub has_more: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
