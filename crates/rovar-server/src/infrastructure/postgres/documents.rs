use crate::{
    application::ports::{DocumentWrite, Documents, Preparation},
    domain::{
        document::{Document, DocumentKind, SaveDocument, StoredVersion},
        error::{Error, Result, now},
    },
};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};

pub struct DocumentRepository(PgPool);

/// Owns the row lock until commit/drop. SQL transactions stay in this module.
pub struct PreparedWrite {
    transaction: Transaction<'static, Postgres>,
    current: Document,
    space_id: String,
    fingerprint: Vec<u8>,
}

impl DocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self(pool)
    }
}

#[async_trait]
impl Documents for DocumentRepository {
    async fn list(&self, actor: &str, space_id: &str) -> Result<Vec<Document>> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space_id, false).await?;
        Ok(sqlx::query(
            "SELECT * FROM objects WHERE space_id=$1 AND revision>0 ORDER BY created DESC,id",
        )
        .bind(space_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(object)
        .collect())
    }

    async fn current(&self, actor: &str, space_id: &str, id: &str) -> Result<StoredVersion> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space_id, false).await?;
        let row = sqlx::query("SELECT o.*,r.blob FROM objects o JOIN revisions r ON r.space_id=o.space_id AND r.object_id=o.id AND r.revision=o.revision WHERE o.space_id=$1 AND o.id=$2 AND NOT o.deleted")
            .bind(space_id).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        Ok(StoredVersion {
            document: object(&row),
            blob: row.get("blob"),
        })
    }

    async fn prepare(
        &self,
        actor: &str,
        space_id: &str,
        input: &SaveDocument,
        fingerprint: Vec<u8>,
    ) -> Result<Preparation> {
        let id = &input.id;
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space_id, false).await?;
        sqlx::query("INSERT INTO objects(space_id,id,kind,title,created,modified) VALUES($1,$2,$3,$4,$5,$5) ON CONFLICT DO NOTHING")
            .bind(space_id).bind(id).bind(if input.kind == DocumentKind::Document { "document" } else { "component" })
            .bind(&input.title).bind(now()).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT * FROM objects WHERE space_id=$1 AND id=$2 FOR UPDATE")
            .bind(space_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let current = object(&row);
        if let Some(previous) = sqlx::query("SELECT request_hash,revision FROM revisions WHERE space_id=$1 AND object_id=$2 AND request_id=$3")
            .bind(space_id).bind(id).bind(&input.request_id).fetch_optional(&mut *tx).await? {
            if previous.get::<Vec<u8>,_>("request_hash") != fingerprint || previous.get::<i64,_>("revision") != current.revision { return Err(Error::Conflict); }
            return Ok(Preparation::AlreadyCommitted(current));
        }
        if current.revision != input.base_revision || current.deleted || current.kind != input.kind
        {
            return Err(Error::Conflict);
        }
        Ok(Preparation::Write(Box::new(PreparedWrite {
            transaction: tx,
            current,
            space_id: space_id.into(),
            fingerprint,
        })))
    }
}

#[async_trait]
impl DocumentWrite for PreparedWrite {
    fn revision(&self) -> i64 {
        self.current.revision + 1
    }

    async fn commit(self: Box<Self>, input: &SaveDocument, blob: &str) -> Result<Document> {
        let mut this = *self;
        let revision = this.revision();
        sqlx::query("INSERT INTO revisions(space_id,object_id,revision,request_id,request_hash,blob) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&this.space_id).bind(&this.current.id).bind(revision).bind(&input.request_id).bind(&this.fingerprint).bind(blob)
            .execute(&mut *this.transaction).await?;
        let row = sqlx::query("UPDATE objects SET title=$3,revision=$4,modified=$5,deleted=$6 WHERE space_id=$1 AND id=$2 RETURNING *")
            .bind(&this.space_id).bind(&this.current.id).bind(&input.title).bind(revision).bind(now()).bind(input.deleted)
            .fetch_one(&mut *this.transaction).await?;
        this.transaction.commit().await?;
        Ok(object(&row))
    }
}

fn object(row: &PgRow) -> Document {
    Document {
        id: row.get("id"),
        kind: if row.get::<String, _>("kind") == "component" {
            DocumentKind::Component
        } else {
            DocumentKind::Document
        },
        title: row.get("title"),
        revision: row.get("revision"),
        created: row.get::<i64, _>("created") as u64,
        modified: row.get::<i64, _>("modified") as u64,
        deleted: row.get("deleted"),
    }
}
