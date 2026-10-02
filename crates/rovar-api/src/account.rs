use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDevice {
    pub system: String,
    pub name: String,
    pub client: String,
}

#[derive(Serialize, Deserialize)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountSession {
    pub id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub current: bool,
    #[serde(default)]
    pub device: SessionDevice,
}
