#[derive(Clone)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub role: String,
}
pub struct Member {
    pub user_id: String,
    pub username: String,
    pub role: String,
}
pub struct Invitation {
    pub code: String,
    pub expires_at: i64,
}
#[derive(Clone, Copy, Default)]
pub struct RegistrationPolicy {
    pub personal: bool,
    pub teams: bool,
}

pub fn name(value: &str) -> super::error::Result<&str> {
    let name = value.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(super::error::Error::Invalid(
            "Use a team name of 1–120 bytes".into(),
        ));
    }
    Ok(name)
}
