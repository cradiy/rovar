use crate::{
    application::ports::Spaces,
    domain::{
        error::{Error, Result, now},
        space::{Member, Space},
    },
};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};

pub struct SpaceRepository(PgPool);
impl SpaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self(pool)
    }
}

pub async fn require(
    tx: &mut Transaction<'_, Postgres>,
    actor: &str,
    space: &str,
    owner: bool,
) -> Result<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM memberships WHERE space_id=$1 AND user_id=$2 FOR SHARE",
    )
    .bind(space)
    .bind(actor)
    .fetch_optional(&mut **tx)
    .await?;
    if role.is_none() || (owner && role.as_deref() != Some("owner")) {
        return Err(Error::Forbidden);
    }
    Ok(())
}

pub async fn create_space(
    tx: &mut Transaction<'_, Postgres>,
    actor: &str,
    name: &str,
    kind: &str,
) -> Result<Space> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO spaces(id,name,kind,personal_owner) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(name)
        .bind(kind)
        .bind((kind == "personal").then_some(actor))
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO memberships(space_id,user_id,role) VALUES($1,$2,'owner')")
        .bind(&id)
        .bind(actor)
        .execute(&mut **tx)
        .await?;
    Ok(Space {
        id,
        name: name.into(),
        kind: kind.into(),
        role: "owner".into(),
    })
}

fn space(row: &PgRow) -> Space {
    Space {
        id: row.get("id"),
        name: row.get("name"),
        kind: row.get("kind"),
        role: row.get("role"),
    }
}

async fn team(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<()> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM spaces WHERE id=$1 AND kind='team' FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    exists.ok_or(Error::NotFound)?;
    Ok(())
}

#[async_trait]
impl Spaces for SpaceRepository {
    async fn list(&self, actor: &str) -> Result<Vec<Space>> {
        Ok(sqlx::query("SELECT s.*,m.role FROM spaces s JOIN memberships m ON s.id=m.space_id WHERE m.user_id=$1 ORDER BY s.kind,s.name,s.id")
            .bind(actor).fetch_all(&self.0).await?.iter().map(space).collect())
    }
    async fn create(&self, actor: &str, name: &str) -> Result<Space> {
        let mut tx = self.0.begin().await?;
        let result = create_space(&mut tx, actor, name, "team").await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn invite(&self, actor: &str, id: &str, hash: &[u8], expires: i64) -> Result<()> {
        let mut tx = self.0.begin().await?;
        team(&mut tx, id).await?;
        require(&mut tx, actor, id, true).await?;
        // A newly generated invitation replaces the team's previous unclaimed code.
        sqlx::query("DELETE FROM invitations WHERE space_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO invitations(token_hash,space_id,expires_at) VALUES($1,$2,$3)")
            .bind(hash)
            .bind(id)
            .bind(expires)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    async fn join(&self, actor: &str, hash: &[u8]) -> Result<Space> {
        let mut tx = self.0.begin().await?;
        let id: String = sqlx::query_scalar("SELECT space_id FROM invitations WHERE token_hash=$1")
            .bind(hash)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::Invalid("Invalid or expired invitation".into()))?;
        team(&mut tx, &id).await?;
        let row = sqlx::query(
            "SELECT used_by FROM invitations WHERE token_hash=$1 AND expires_at>$2 FOR UPDATE",
        )
        .bind(hash)
        .bind(now())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Invalid("Invalid or expired invitation".into()))?;
        if row.get::<Option<String>, _>("used_by").is_some() {
            return Err(Error::Invalid("Invitation already used".into()));
        }
        let existing: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM memberships WHERE space_id=$1 AND user_id=$2)",
        )
        .bind(&id)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await?;
        if existing {
            return Err(Error::Invalid("You already belong to this team".into()));
        }
        sqlx::query("INSERT INTO memberships(space_id,user_id,role) VALUES($1,$2,'member')")
            .bind(&id)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE invitations SET used_by=$2 WHERE token_hash=$1")
            .bind(hash)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query("SELECT s.*,m.role FROM spaces s JOIN memberships m ON m.space_id=s.id WHERE s.id=$1 AND m.user_id=$2")
            .bind(&id).bind(actor).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(space(&row))
    }
    async fn members(&self, actor: &str, id: &str) -> Result<Vec<Member>> {
        let mut tx = self.0.begin().await?;
        team(&mut tx, id).await?;
        require(&mut tx, actor, id, false).await?;
        Ok(sqlx::query("SELECT u.id,u.username,m.role FROM memberships m JOIN users u ON u.id=m.user_id WHERE m.space_id=$1 ORDER BY m.role DESC,u.username")
            .bind(id).fetch_all(&mut *tx).await?.iter().map(|row| Member { user_id: row.get("id"), username: row.get("username"), role: row.get("role") }).collect())
    }
    async fn remove(&self, actor: &str, id: &str, user: &str) -> Result<()> {
        let mut tx = self.0.begin().await?;
        team(&mut tx, id).await?;
        require(&mut tx, actor, id, actor != user).await?;
        let affected = sqlx::query(
            "DELETE FROM memberships WHERE space_id=$1 AND user_id=$2 AND role<>'owner'",
        )
        .bind(id)
        .bind(user)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected == 0 {
            return Err(Error::Invalid("The team owner cannot be removed".into()));
        }
        tx.commit().await?;
        Ok(())
    }
}
