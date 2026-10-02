use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeSet;

use crate::infrastructure::storage::retention::{BATCH, Policy};

const PUBLICATION_LOCK: i64 = 0x0052_4f56_4152;

pub(crate) async fn protect_publication(tx: &mut Transaction<'_, Postgres>) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
        .bind(PUBLICATION_LOCK)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Caller holds the store's exclusive lease until reference removal commits
/// and unreferenced files are deleted. Receipts and directory cursors survive.
pub(crate) async fn expire(pool: &PgPool, policy: Policy, now: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    // A cancelled request drops its file lease before PostgreSQL necessarily
    // finishes COMMIT/ROLLBACK. Wait for those publishers before reading refs.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PUBLICATION_LOCK)
        .execute(&mut *tx)
        .await?;
    let revisions = sqlx::query(
        "WITH expired AS (
            SELECT r.space_id,r.object_id,r.revision FROM revisions r
            JOIN objects o ON o.space_id=r.space_id AND o.id=r.object_id
            WHERE r.blob IS NOT NULL AND r.modified<$1
              AND r.revision<=o.revision-$2
            ORDER BY r.modified LIMIT $3
        ) UPDATE revisions r SET blob=NULL,media='[]'::jsonb FROM expired e
          WHERE r.space_id=e.space_id AND r.object_id=e.object_id AND r.revision=e.revision",
    )
    .bind(now.saturating_sub(i64::from(policy.history_days) * 86400))
    .bind(i64::from(policy.history_versions))
    .bind(BATCH as i64)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let media = sqlx::query(
        "WITH expired AS (
            SELECT m.space_id,m.hash FROM media m WHERE m.last_used<$1
            AND NOT EXISTS (SELECT 1 FROM revisions r WHERE r.space_id=m.space_id
                AND r.blob IS NOT NULL AND r.media @> jsonb_build_array(jsonb_build_object('hash',m.hash)))
            ORDER BY m.last_used LIMIT $2
        ) DELETE FROM media m USING expired e WHERE m.space_id=e.space_id AND m.hash=e.hash",
    )
    .bind(now.saturating_sub(i64::from(policy.orphan_days) * 86400))
    .bind(BATCH as i64)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(revisions == BATCH as u64 || media == BATCH as u64)
}

pub(crate) async fn references(pool: &PgPool, blobs: &[String]) -> Result<BTreeSet<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT blob FROM revisions WHERE blob=ANY($1) UNION SELECT blob FROM media WHERE blob=ANY($1)",
    )
    .bind(blobs)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}
