#[derive(Clone)]
pub struct Identity {
    pub spaces: Vec<super::space::Space>,
    pub registration: super::space::RegistrationPolicy,
    pub server_id: String,
    pub user_id: String,
    pub username: String,
}

pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
}

pub struct Session {
    pub identity: Identity,
    pub token: String,
}
