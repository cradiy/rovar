use super::*;
use crate::{
    application::{documents::DocumentService, ports::ContentStorage},
    domain::document::{DocumentKind, SaveDocument},
    infrastructure::postgres::documents::DocumentRepository,
};
use std::sync::Arc;

const DOCUMENT: &str = "00000000-0000-0000-0000-000000000001";

#[tokio::test]
async fn recent_files_consume_scan_budget_and_resume_while_publishers_hold_leases() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    for _ in 0..=SCAN_BATCH {
        fs::write(
            store.directory.join(uuid::Uuid::new_v4().to_string()),
            b"recent",
        )
        .unwrap();
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://localhost/unused")
        .unwrap();
    let lease = store.lease().await.unwrap();
    assert_eq!(
        store
            .reclaim(&pool, Policy::default(), SystemTime::now())
            .await
            .unwrap(),
        None
    );
    {
        let scan = store.retention_scan.lock().await;
        assert!(scan.pending.is_empty());
        assert!(
            scan.entries.is_some(),
            "Recent files must also consume the scan budget"
        );
    }
    assert_eq!(
        store
            .reclaim(&pool, Policy::default(), SystemTime::now())
            .await
            .unwrap(),
        None
    );
    assert!(
        store.retention_scan.lock().await.entries.is_none(),
        "The next tick must resume, not restart"
    );
    drop(lease);
    pool.close().await;
}

#[tokio::test]
async fn candidate_batches_survive_contention_and_failed_database_access() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let count = BATCH + 17;
    for _ in 0..count {
        fs::write(
            store.directory.join(uuid::Uuid::new_v4().to_string()),
            b"orphan",
        )
        .unwrap();
    }
    let now = SystemTime::now() + Duration::from_secs(8 * 86400);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://localhost/unused")
        .unwrap();
    let lease = store.lease().await.unwrap();
    assert_eq!(
        store.reclaim(&pool, Policy::default(), now).await.unwrap(),
        None
    );
    let first = store.retention_scan.lock().await.pending.clone();
    assert_eq!(first.len(), BATCH);
    assert_eq!(
        store.reclaim(&pool, Policy::default(), now).await.unwrap(),
        None
    );
    assert_eq!(store.retention_scan.lock().await.pending, first);
    drop(lease);
    pool.close().await;
    assert!(store.reclaim(&pool, Policy::default(), now).await.is_err());
    let mut scan = store.retention_scan.lock().await;
    assert_eq!(scan.pending, first);
    assert_eq!(fs::read_dir(&store.directory).unwrap().count(), count);
    // Simulate finishing the first bounded batch. The cursor must reach every
    // remaining candidate even when the earlier files are still referenced.
    scan.pending.clear();
    scan.collect(&store.directory, Policy::default(), now)
        .await
        .unwrap();
    assert_eq!(scan.pending.len(), 17);
    assert!(scan.entries.is_none());
    assert!(scan.pending.iter().all(|name| !first.contains(name)));
}

#[tokio::test]
async fn active_leases_exclude_collection_across_store_instances() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let second = ContentStore::open(root.path()).unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://localhost/unused")
        .unwrap();
    let one = store.lease().await.unwrap();
    let two = second.lease().await.unwrap();
    assert_eq!(
        store
            .reclaim(&pool, Policy::default(), SystemTime::now())
            .await
            .unwrap(),
        None
    );
    drop(one);
    assert_eq!(
        store
            .reclaim(&pool, Policy::default(), SystemTime::now())
            .await
            .unwrap(),
        None
    );
    drop(two);
    let lock = open_lock(&store.activity).unwrap();
    lock.try_lock().unwrap();
    // Failed database access cannot reach physical deletion.
    let blob = uuid::Uuid::new_v4().to_string();
    fs::write(store.directory.join(&blob), b"keep on error").unwrap();
    drop(lock);
    pool.close().await;
    assert!(
        store
            .reclaim(&pool, Policy::default(), SystemTime::now())
            .await
            .is_err()
    );
    assert!(store.directory.join(blob).exists());
}

fn command(base: i64) -> SaveDocument {
    SaveDocument {
        id: DOCUMENT.into(),
        kind: DocumentKind::Document,
        title: format!("Version {}", base + 1),
        base_revision: base,
        request_id: uuid::Uuid::new_v4().to_string(),
        content: format!("content {}", base + 1).into_bytes(),
        media: Vec::new(),
        deleted: false,
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database in ROVAR_TEST_DATABASE_URL"]
async fn retention_preserves_current_content_receipts_and_recent_media_probes() {
    let url = std::env::var("ROVAR_TEST_DATABASE_URL").unwrap();
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    // Keep the retention clock and sweep isolated from other integration tests.
    let schema = format!("retention_{}", uuid::Uuid::new_v4().simple());
    // The identifier consists solely of this literal prefix and UUID hex.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(move |connection, _| {
            let statement = search_path.clone();
            Box::pin(async move {
                sqlx::query("SELECT set_config('search_path',$1,false)")
                    .bind(statement)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO users VALUES('actor','actor','unused');
        INSERT INTO spaces VALUES('space','Space','personal','actor');
        INSERT INTO memberships VALUES('space','actor','owner');",
    )
    .execute(&pool)
    .await
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(ContentStore::open(root.path()).unwrap());
    let service = DocumentService::new(
        Arc::new(DocumentRepository::new(pool.clone())),
        store.clone(),
    );
    let first = command(0);
    let mut original = command(0);
    original.request_id = first.request_id.clone();
    service.save("actor", "space", original).await.unwrap();
    for base in 1..5 {
        service.save("actor", "space", command(base)).await.unwrap();
    }
    let now = crate::domain::error::now();
    let old = now - 3 * 86400;
    sqlx::query("UPDATE revisions SET modified=$1 WHERE revision<>2")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let blobs: Vec<(i64, String)> =
        sqlx::query_as("SELECT revision,blob FROM revisions ORDER BY revision")
            .fetch_all(&pool)
            .await
            .unwrap();
    // One old media row is referenced only by an expiring revision, another by
    // the live revision. A third was probed by a client preparing a new upload.
    for (index, letter) in ['a', 'b', 'c'].into_iter().enumerate() {
        let hash = letter.to_string().repeat(64);
        let blob = store
            .write(vec![index as u8], format!("space/media/{hash}"))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO media(space_id,hash,length,blob,last_used) VALUES('space',$1,1,$2,$3)",
        )
        .bind(&hash)
        .bind(blob)
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
        if index < 2 {
            let revision = if index == 0 { 1 } else { 5 };
            sqlx::query("UPDATE revisions SET media=$1 WHERE revision=$2")
                .bind(serde_json::json!([{"hash":hash,"length":1}]))
                .bind(revision)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    assert!(
        service
            .missing_media(
                "actor",
                "space",
                &[crate::domain::document::Media {
                    hash: "c".repeat(64),
                    length: 1
                }]
            )
            .await
            .unwrap()
            .is_empty()
    );
    let orphan = store
        .write(b"abandoned".to_vec(), "abandoned".into())
        .await
        .unwrap();
    for entry in fs::read_dir(&store.directory).unwrap() {
        File::options()
            .write(true)
            .open(entry.unwrap().path())
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(old as u64))
            .unwrap();
    }
    let recent = store
        .write(b"not committed yet".to_vec(), "recent".into())
        .await
        .unwrap();
    let policy = Policy {
        history_days: 1,
        history_versions: 2,
        orphan_days: 1,
    };
    // Candidates can change after a lock-free scan. A renewed file and a newly
    // referenced blob must both survive final validation under the store lease.
    let renewed = store
        .write(b"renewed".to_vec(), "renewed".into())
        .await
        .unwrap();
    let referenced_later = store
        .write(b"later reference".to_vec(), "later".into())
        .await
        .unwrap();
    for blob in [&renewed, &referenced_later] {
        File::options()
            .write(true)
            .open(store.directory.join(blob))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(old as u64))
            .unwrap();
    }
    {
        let mut scan = store.retention_scan.lock().await;
        scan.collect(&store.directory, policy, SystemTime::now())
            .await
            .unwrap();
        assert!(scan.pending.contains(&renewed));
        assert!(scan.pending.contains(&referenced_later));
    }
    File::options()
        .write(true)
        .open(store.directory.join(&renewed))
        .unwrap()
        .set_modified(SystemTime::now())
        .unwrap();
    sqlx::query(
        "INSERT INTO media(space_id,hash,length,blob,last_used) VALUES('space',$1,1,$2,$3)",
    )
    .bind("d".repeat(64))
    .bind(&referenced_later)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    // Even after a cancelled HTTP request releases its file lease, an
    // unfinished database publication must fence off the collector.
    let mut publication = pool.begin().await.unwrap();
    crate::infrastructure::postgres::retention::protect_publication(&mut publication)
        .await
        .unwrap();
    let worker_store = store.clone();
    let worker_pool = pool.clone();
    let mut worker = tokio::spawn(async move {
        worker_store
            .reclaim(&worker_pool, policy, SystemTime::now())
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut worker)
            .await
            .is_err()
    );
    publication.rollback().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Some(false)
    );
    let retained: Vec<i64> = sqlx::query_scalar(
        "SELECT revision FROM revisions WHERE blob IS NOT NULL ORDER BY revision",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(retained, [2, 4, 5]);
    for (revision, blob) in &blobs {
        assert_eq!(
            store.directory.join(blob).exists(),
            retained.contains(revision)
        );
    }
    assert!(!store.directory.join(orphan).exists());
    assert!(store.directory.join(recent).exists());
    assert!(store.directory.join(renewed).exists());
    assert!(store.directory.join(referenced_later).exists());
    let media: Vec<String> = sqlx::query_scalar("SELECT hash FROM media ORDER BY hash")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(media, ["b".repeat(64), "c".repeat(64), "d".repeat(64)]);
    assert_eq!(
        service
            .save("actor", "space", first)
            .await
            .unwrap()
            .revision,
        1
    );
    let current = service.transfer("actor", "space", DOCUMENT).await.unwrap();
    assert_eq!(current.content, b"content 5");
    assert!(
        !service
            .download("actor", "space", DOCUMENT, 1, [0; 32])
            .await
            .unwrap()
            .delta
    );
    assert_eq!(
        store
            .reclaim(&pool, policy, SystemTime::now())
            .await
            .unwrap(),
        Some(false)
    );
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
}
