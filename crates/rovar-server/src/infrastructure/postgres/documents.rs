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
    async fn media_lengths(
        &self,
        actor: &str,
        space: &str,
        hashes: &[String],
    ) -> Result<std::collections::BTreeMap<String, u64>> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        media_lengths(&mut tx, space, hashes).await
    }
    async fn media(&self, actor: &str, space: &str, hash: &str) -> Result<Option<(String, u64)>> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        Ok(
            sqlx::query("SELECT blob,length FROM media WHERE space_id=$1 AND hash=$2")
                .bind(space)
                .bind(hash)
                .fetch_optional(&mut *tx)
                .await?
                .map(|row| (row.get("blob"), row.get::<i64, _>("length") as u64)),
        )
    }

    async fn store_media(
        &self,
        actor: &str,
        space: &str,
        media: &crate::domain::document::Media,
        blob: &str,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        sqlx::query("INSERT INTO media(space_id,hash,length,blob) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(space).bind(&media.hash).bind(media.length as i64).bind(blob).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn changes(
        &self,
        actor: &str,
        space: &str,
        after: i64,
    ) -> Result<crate::domain::document::Changes> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        sqlx::query("INSERT INTO sync_cursors(space_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(space)
            .execute(&mut *tx)
            .await?;
        // Writers take this row's exclusive lock before publishing their object
        // state. Holding a shared lock makes the watermark and page consistent.
        let watermark: i64 =
            sqlx::query_scalar("SELECT sequence FROM sync_cursors WHERE space_id=$1 FOR SHARE")
                .bind(space)
                .fetch_one(&mut *tx)
                .await?;
        if after < 0 || after > watermark {
            return Err(Error::Invalid("Invalid sync cursor".into()));
        }
        let mut rows = sqlx::query("SELECT * FROM objects WHERE space_id=$1 AND revision>0 AND change_sequence>$2 ORDER BY change_sequence LIMIT 257")
            .bind(space).bind(after).fetch_all(&mut *tx).await?;
        let has_more = rows.len() > 256;
        rows.truncate(256);
        let cursor = if has_more {
            rows.last().unwrap().get("change_sequence")
        } else {
            watermark
        };
        Ok(crate::domain::document::Changes {
            documents: rows.iter().map(object).collect(),
            cursor,
            has_more,
        })
    }

    async fn metadata(&self, actor: &str, space: &str, id: &str) -> Result<Document> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        let row = sqlx::query("SELECT * FROM objects WHERE space_id=$1 AND id=$2 AND revision>0")
            .bind(space)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(object(&row))
    }
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
        let row = sqlx::query("SELECT o.*,r.blob,r.media FROM objects o JOIN revisions r ON r.space_id=o.space_id AND r.object_id=o.id AND r.revision=o.revision WHERE o.space_id=$1 AND o.id=$2 AND NOT o.deleted")
            .bind(space_id).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        Ok(StoredVersion {
            document: object(&row),
            blob: row.get("blob"),
            media: serde_json::from_value(row.get("media")).map_err(anyhow::Error::from)?,
        })
    }

    async fn version_blob(
        &self,
        actor: &str,
        space: &str,
        id: &str,
        revision: i64,
    ) -> Result<Option<String>> {
        let mut tx = self.0.begin().await?;
        super::spaces::require(&mut tx, actor, space, false).await?;
        Ok(sqlx::query_scalar("SELECT r.blob FROM revisions r JOIN objects o ON o.space_id=r.space_id AND o.id=r.object_id WHERE r.space_id=$1 AND r.object_id=$2 AND r.revision=$3 AND NOT o.deleted")
            .bind(space).bind(id).bind(revision).fetch_optional(&mut *tx).await?)
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
        let hashes: Vec<_> = input.media.iter().map(|media| media.hash.clone()).collect();
        let lengths = media_lengths(&mut tx, space_id, &hashes).await?;
        for media in &input.media {
            if lengths.get(&media.hash) != Some(&media.length) {
                return Err(Error::Invalid(
                    "Upload referenced media before saving the document".into(),
                ));
            }
        }
        sqlx::query("INSERT INTO objects(space_id,id,kind,title,created,modified) VALUES($1,$2,$3,$4,$5,$5) ON CONFLICT DO NOTHING")
            .bind(space_id).bind(id).bind(match input.kind {
                DocumentKind::Document => "document",
                DocumentKind::Component => "component",
                DocumentKind::ColorStyle => "color_style",
            })
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

    async fn base_blob(&mut self) -> Result<String> {
        sqlx::query_scalar(
            "SELECT blob FROM revisions WHERE space_id=$1 AND object_id=$2 AND revision=$3",
        )
        .bind(&self.space_id)
        .bind(&self.current.id)
        .bind(self.current.revision)
        .fetch_optional(&mut *self.transaction)
        .await?
        .ok_or(Error::DeltaBase)
    }

    async fn commit(self: Box<Self>, input: &SaveDocument, blob: &str) -> Result<Document> {
        let mut this = *self;
        let revision = this.revision();
        // Unlike a database sequence, this counter cannot publish out of commit
        // order: the row lock is held until the object transaction completes.
        let sequence: i64 = sqlx::query_scalar("INSERT INTO sync_cursors(space_id,sequence) VALUES($1,1) ON CONFLICT(space_id) DO UPDATE SET sequence=sync_cursors.sequence+1 RETURNING sequence")
            .bind(&this.space_id).fetch_one(&mut *this.transaction).await?;
        sqlx::query("INSERT INTO revisions(space_id,object_id,revision,request_id,request_hash,blob,media) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&this.space_id).bind(&this.current.id).bind(revision).bind(&input.request_id).bind(&this.fingerprint).bind(blob)
            .bind(serde_json::to_value(&input.media).map_err(anyhow::Error::from)?)
            .execute(&mut *this.transaction).await?;
        let row = sqlx::query("UPDATE objects SET title=$3,revision=$4,modified=$5,deleted=$6,change_sequence=$7 WHERE space_id=$1 AND id=$2 RETURNING *")
            .bind(&this.space_id).bind(&this.current.id).bind(&input.title).bind(revision).bind(now()).bind(input.deleted).bind(sequence)
            .fetch_one(&mut *this.transaction).await?;
        this.transaction.commit().await?;
        Ok(object(&row))
    }
}

fn object(row: &PgRow) -> Document {
    Document {
        id: row.get("id"),
        kind: match row.get::<String, _>("kind").as_str() {
            "document" => DocumentKind::Document,
            "component" => DocumentKind::Component,
            "color_style" => DocumentKind::ColorStyle,
            _ => unreachable!("object kind is constrained by the database"),
        },
        title: row.get("title"),
        revision: row.get("revision"),
        created: row.get::<i64, _>("created") as u64,
        modified: row.get::<i64, _>("modified") as u64,
        deleted: row.get("deleted"),
    }
}

async fn media_lengths(
    tx: &mut Transaction<'_, Postgres>,
    space: &str,
    hashes: &[String],
) -> Result<std::collections::BTreeMap<String, u64>> {
    if hashes.is_empty() {
        return Ok(Default::default());
    }
    Ok(
        sqlx::query("SELECT hash,length FROM media WHERE space_id=$1 AND hash=ANY($2)")
            .bind(space)
            .bind(hashes)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .map(|row| (row.get("hash"), row.get::<i64, _>("length") as u64))
            .collect(),
    )
}
