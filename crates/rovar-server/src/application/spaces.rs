use super::ports::Spaces;
use crate::domain::{
    error::{Error, Result, now},
    space::{Invitation, Member, RegistrationPolicy, Space, name},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct SpaceService {
    repository: Arc<dyn Spaces>,
    policy: RegistrationPolicy,
}
impl SpaceService {
    pub fn new(repository: Arc<dyn Spaces>, policy: RegistrationPolicy) -> Self {
        Self { repository, policy }
    }
    pub async fn list(&self, actor: &str) -> Result<Vec<Space>> {
        self.repository.list(actor).await
    }
    pub async fn create(&self, actor: &str, value: &str) -> Result<Space> {
        if !self.policy.teams {
            return Err(Error::Forbidden);
        }
        self.repository.create(actor, name(value)?).await
    }
    pub async fn invite(&self, actor: &str, space: &str) -> Result<Invitation> {
        let code = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expires_at = now() + 7 * 86400;
        self.repository
            .invite(actor, space, &Sha256::digest(code.as_bytes()), expires_at)
            .await?;
        Ok(Invitation { code, expires_at })
    }
    pub async fn join(&self, actor: &str, code: &str) -> Result<Space> {
        let code = code.trim();
        if code.len() != 64 {
            return Err(Error::Invalid("Invalid invitation code".into()));
        }
        self.repository
            .join(actor, &Sha256::digest(code.as_bytes()))
            .await
    }
    pub async fn members(&self, actor: &str, space: &str) -> Result<Vec<Member>> {
        self.repository.members(actor, space).await
    }
    pub async fn remove(&self, actor: &str, space: &str, user: &str) -> Result<()> {
        self.repository.remove(actor, space, user).await
    }
}
