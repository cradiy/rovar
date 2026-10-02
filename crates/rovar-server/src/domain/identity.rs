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
pub struct AccountSession {
    pub id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub current: bool,
    pub device: SessionDevice,
}

#[derive(Clone, Default)]
pub struct SessionDevice {
    pub system: String,
    pub name: String,
    pub client: String,
}

impl SessionDevice {
    pub fn validate(&self) -> super::error::Result<()> {
        if [&self.system, &self.name, &self.client]
            .into_iter()
            .any(|value| value.chars().count() > 128 || value.chars().any(char::is_control))
        {
            return Err(super::error::Error::Invalid(
                "Invalid session device information".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_metadata_accepts_unicode_and_rejects_unbounded_or_control_text() {
        let mut device = SessionDevice {
            system: "Linux".into(),
            name: "设计工作站".into(),
            client: "Rovar Desktop".into(),
        };
        assert!(device.validate().is_ok());
        device.name = "机".repeat(129);
        assert!(device.validate().is_err());
        device.name = "work\nstation".into();
        assert!(device.validate().is_err());
        assert!(SessionDevice::default().validate().is_ok());
    }
}
