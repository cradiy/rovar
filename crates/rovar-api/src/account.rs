use serde::{Deserialize, Serialize};

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
}
