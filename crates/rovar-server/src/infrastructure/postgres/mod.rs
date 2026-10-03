use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn connect(url: &str) -> Result<(PgPool, String)> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    sqlx::query("INSERT INTO server_info (server_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await?;
    let server_id = sqlx::query_scalar("SELECT server_id FROM server_info")
        .fetch_one(&pool)
        .await?;
    Ok((pool, server_id))
}
pub mod auth;
pub mod documents;
pub(crate) mod retention;
pub mod spaces;

impl From<sqlx::Error> for crate::domain::error::Error {
    fn from(value: sqlx::Error) -> Self {
        if value
            .as_database_error()
            .is_some_and(|e| e.is_unique_violation())
        {
            Self::AlreadyExists
        } else {
            Self::Internal(value.into())
        }
    }
}
