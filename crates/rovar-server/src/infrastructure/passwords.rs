use crate::{application::ports::Passwords, domain::error::Result};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use tokio::sync::Semaphore;

pub struct ArgonPasswords(Semaphore);
impl ArgonPasswords {
    pub fn new() -> Self {
        Self(Semaphore::new(2))
    }
}

#[async_trait]
impl Passwords for ArgonPasswords {
    async fn hash(&self, password: String) -> Result<String> {
        let _permit = self.0.acquire().await.map_err(anyhow::Error::from)?;
        Ok(tokio::task::spawn_blocking(move || {
            Argon2::default()
                .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
                .map(|hash| hash.to_string())
                .map_err(|_| anyhow::anyhow!("Password hashing failed"))
        })
        .await
        .map_err(anyhow::Error::from)??)
    }

    async fn verify(&self, password: String, hash: String) -> Result<bool> {
        let _permit = self.0.acquire().await.map_err(anyhow::Error::from)?;
        Ok(tokio::task::spawn_blocking(move || {
            PasswordHash::new(&hash).is_ok_and(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
        })
        .await
        .map_err(anyhow::Error::from)?)
    }
}
