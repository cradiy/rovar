use crate::{
    application::ports::Accounts,
    domain::{
        error::{Error, Result, now},
        identity::{AccountSession, User},
    },
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

    async fn create_session(
        &self,
        user_id: &str,
        hash: &[u8],
        expires_at: i64,
        password_hash: &str,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let current: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1 FOR UPDATE")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?;
        if current != password_hash {
            return Err(Error::Unauthorized);
        }
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

    async fn sessions(&self, user: &str, current: &[u8]) -> Result<Vec<AccountSession>> {
        Ok(sqlx::query("SELECT id,created_at,expires_at,token_hash=$2 AS current FROM sessions WHERE user_id=$1 AND expires_at>$3 ORDER BY created_at DESC,id")
            .bind(user).bind(current).bind(now()).fetch_all(&self.0).await?.into_iter().map(|row| AccountSession {
                id: row.get("id"), created_at: row.get("created_at"), expires_at: row.get("expires_at"), current: row.get("current"),
            }).collect())
    }

    async fn revoke_sessions(&self, user: &str, current: &[u8], id: Option<&str>) -> Result<()> {
        let mut tx = self.0.begin().await?;
        lock_account(&mut tx, user, current).await?;
        let result = sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash<>$2 AND ($3::text IS NULL OR id=$3)")
            .bind(user).bind(current).bind(id).execute(&mut *tx).await?;
        if id.is_some() && result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    async fn change_password(
        &self,
        user: &str,
        current: &[u8],
        previous: &str,
        next: &str,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        lock_account(&mut tx, user, current).await?;
        let result =
            sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1 AND password_hash=$3")
                .bind(user)
                .bind(next)
                .bind(previous)
                .execute(&mut *tx)
                .await?;
        if result.rows_affected() != 1 {
            return Err(Error::Invalid("Password changed; try again".into()));
        }
        sqlx::query("DELETE FROM sessions WHERE user_id=$1 AND token_hash<>$2")
            .bind(user)
            .bind(current)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn lock_account(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: &str,
    token: &[u8],
) -> Result<()> {
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(user)
        .fetch_one(&mut **tx)
        .await?;
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE user_id=$1 AND token_hash=$2 AND expires_at>$3)")
        .bind(user).bind(token).bind(now()).fetch_one(&mut **tx).await?;
    if !valid {
        return Err(Error::Unauthorized);
    }
    Ok(())
}
