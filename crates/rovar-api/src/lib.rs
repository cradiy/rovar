mod account;
mod spaces;
pub use account::*;
use serde::{Deserialize, Serialize};
pub use spaces::*;

pub const VERSION: u32 = 2;
pub const MAX_CONTENT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
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

#[derive(Serialize, Deserialize)]
pub struct Save {
    pub kind: Kind,
    pub title: String,
    pub base_revision: i64,
    pub request_id: String,
    /// Base64-encoded complete .rovar snapshot, including its media and preview.
    pub content: String,
    pub deleted: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub object: Object,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
