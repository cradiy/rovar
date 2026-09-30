use super::ports::{Accounts, Passwords, Spaces};
use crate::domain::{
    error::{Error, Result, now},
    identity::{Identity, Session},
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
mod account;

pub const SESSION_SECONDS: i64 = 30 * 86400;

pub struct AuthService {
    accounts: Arc<dyn Accounts>,
    passwords: Arc<dyn Passwords>,
    server_id: String,
    spaces: Arc<dyn Spaces>,
    pub registration: crate::domain::space::RegistrationPolicy,
    attempts: Mutex<HashMap<String, (i64, u32)>>,
}

impl AuthService {
    pub fn new(
        accounts: Arc<dyn Accounts>,
        passwords: Arc<dyn Passwords>,
        server_id: String,
        spaces: Arc<dyn Spaces>,
        registration: crate::domain::space::RegistrationPolicy,
    ) -> Self {
        Self {
            spaces,
            registration,
            accounts,
            passwords,
            server_id,
            attempts: Mutex::default(),
        }
    }

    pub async fn create_user(&self, username: &str, password: &str) -> Result<()> {
        self.create_account(username, password, None).await
    }

    pub async fn register(
        &self,
        username: &str,
        password: &str,
        team: Option<&str>,
    ) -> Result<Session> {
        if if team.is_some() {
            !self.registration.teams
        } else {
            !self.registration.personal
        } {
            return Err(Error::Forbidden);
        }
        let team = team.map(crate::domain::space::name).transpose()?;
        self.create_account(username, password, team).await?;
        self.login(username.trim().into(), password.into()).await
    }

    async fn create_account(
        &self,
        username: &str,
        password: &str,
        team: Option<&str>,
    ) -> Result<()> {
        if username.trim().is_empty()
            || username.len() > 80
            || password.chars().count() < 6
            || password.len() > 1024
        {
            return Err(Error::Invalid(
                "Use a nonempty username and a password of at least 6 characters (up to 1024 bytes)".into(),
            ));
        }
        let hash = self.passwords.hash(password.into()).await?;
        self.accounts
            .create_user(username.trim(), &hash, team)
            .await
    }

    pub async fn login(&self, username: String, password: String) -> Result<Session> {
        if username.len() > 80 || password.len() > 1024 {
            return Err(Error::Unauthorized);
        }
        {
            let mut attempts = self.attempts.lock().await;
            attempts.retain(|_, (started, _)| now() - *started < 60);
            if attempts.len() >= 10000 {
                return Err(Error::RateLimited);
            }
            let entry = attempts.entry(username.clone()).or_insert((now(), 0));
            entry.1 += 1;
            if entry.1 > 8 {
                return Err(Error::RateLimited);
            }
        }
        let user = self
            .accounts
            .find_user(&username)
            .await?
            .ok_or(Error::Unauthorized)?;
        if !self
            .passwords
            .verify(password, user.password_hash.clone())
            .await?
        {
            return Err(Error::Unauthorized);
        }
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.accounts
            .create_session(
                &user.id,
                &token_hash(&token),
                now() + SESSION_SECONDS,
                &user.password_hash,
            )
            .await?;
        Ok(Session {
            identity: self.identity(user.id, user.username).await?,
            token,
        })
    }

    pub async fn authenticate(&self, token: &str) -> Result<Identity> {
        let (id, username) = self
            .accounts
            .session_user(&token_hash(token), now())
            .await?
            .ok_or(Error::Unauthorized)?;
        self.identity(id, username).await
    }

    pub async fn logout(&self, token: &str) -> Result<()> {
        self.accounts.delete_session(&token_hash(token)).await
    }

    async fn identity(&self, user_id: String, username: String) -> Result<Identity> {
        Ok(Identity {
            spaces: self.spaces.list(&user_id).await?,
            registration: self.registration,
            server_id: self.server_id.clone(),
            user_id,
            username,
        })
    }
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
