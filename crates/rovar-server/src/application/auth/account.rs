use super::*;
use crate::domain::identity::AccountSession;

impl AuthService {
    pub async fn sessions(&self, token: &str) -> Result<Vec<AccountSession>> {
        let actor = self.authenticate(token).await?;
        self.accounts
            .sessions(&actor.user_id, &token_hash(token))
            .await
    }

    /// Keep this session so the current editor can continue saving its work.
    pub async fn revoke_sessions(&self, token: &str, id: Option<&str>) -> Result<()> {
        let actor = self.authenticate(token).await?;
        self.accounts
            .revoke_sessions(&actor.user_id, &token_hash(token), id)
            .await
    }

    pub async fn change_password(&self, token: &str, current: String, next: String) -> Result<()> {
        if next.chars().count() < 6 || next.len() > 1024 || current.len() > 1024 {
            return Err(Error::Invalid(
                "Use a password of at least 6 characters (up to 1024 bytes)".into(),
            ));
        }
        let actor = self.authenticate(token).await?;
        let user = self
            .accounts
            .find_user(&actor.username)
            .await?
            .ok_or(Error::Unauthorized)?;
        // Reuse the account's login attempt budget for password verification.
        {
            let mut attempts = self.attempts.lock().await;
            attempts.retain(|_, (started, _)| now() - *started < 60);
            let entry = attempts.entry(actor.username).or_insert((now(), 0));
            entry.1 += 1;
            if entry.1 > 8 {
                return Err(Error::RateLimited);
            }
        }
        if !self
            .passwords
            .verify(current, user.password_hash.clone())
            .await?
        {
            return Err(Error::Invalid("Current password is incorrect".into()));
        }
        let hash = self.passwords.hash(next).await?;
        self.accounts
            .change_password(&user.id, &token_hash(token), &user.password_hash, &hash)
            .await
    }
}
