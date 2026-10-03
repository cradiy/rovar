use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub role: String,
}
#[derive(Serialize, Deserialize)]
pub struct Registration {
    pub username: String,
    pub password: String,
    pub team_name: Option<String>,
    #[serde(default)]
    pub device: crate::SessionDevice,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RegistrationPolicy {
    pub personal: bool,
    pub teams: bool,
}
#[derive(Serialize, Deserialize)]
pub struct ServerInfo {
    pub server_id: String,
    pub api_version: u32,
    pub registration: RegistrationPolicy,
}
#[derive(Serialize, Deserialize)]
pub struct CreateTeam {
    pub name: String,
}
#[derive(Serialize, Deserialize)]
pub struct JoinTeam {
    pub code: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub username: String,
    pub role: String,
}
#[derive(Serialize, Deserialize)]
pub struct Invitation {
    pub code: String,
    pub expires_at: i64,
}
