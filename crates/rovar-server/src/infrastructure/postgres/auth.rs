use crate::{
    application::ports::Accounts,
    domain::{error::Result, identity::User},
};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

pub struct AuthRepository(PgPool);

impl AuthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self(pool)
    }
}

#[async_trait]
impl Accounts for AuthRepository {
    async fn create_user(&self, username: &str, hash: &str, team: Option<&str>) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id,username,password_hash) VALUES($1,$2,$3)")
            .bind(&id)
            .bind(username)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
        super::spaces::create_space(&mut tx, &id, "Personal", "personal").await?;
        if let Some(name) = team {
            super::spaces::create_space(&mut tx, &id, name, "team").await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn find_user(&self, username: &str) -> Result<Option<User>> {
        Ok(
            sqlx::query("SELECT id,username,password_hash FROM users WHERE username=$1")
                .bind(username)
                .fetch_optional(&self.0)
                .await?
                .map(|row| User {
                    id: row.get("id"),
                    username: row.get("username"),
                    password_hash: row.get("password_hash"),
                }),
        )
    }

    async fn create_session(&self, user_id: &str, hash: &[u8], expires_at: i64) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("DELETE FROM sessions WHERE expires_at<$1")
            .bind(crate::domain::error::now())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO sessions(token_hash,user_id,expires_at) VALUES($1,$2,$3)")
            .bind(hash)
            .bind(user_id)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn session_user(&self, hash: &[u8], now: i64) -> Result<Option<(String, String)>> {
        Ok(sqlx::query("SELECT users.id,users.username FROM sessions JOIN users ON users.id=sessions.user_id WHERE token_hash=$1 AND expires_at>$2")
            .bind(hash).bind(now).fetch_optional(&self.0).await?.map(|row| (row.get("id"), row.get("username"))))
    }

    async fn delete_session(&self, hash: &[u8]) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
            .bind(hash)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
