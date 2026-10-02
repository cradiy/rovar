use crate::{
    bootstrap::{self, config::Config},
    domain::{
        document::{DocumentKind, SaveDocument},
        error::Error,
    },
};

fn command(id: &str, base: i64, request: &str, content: &[u8]) -> SaveDocument {
    SaveDocument {
        id: id.into(),
        kind: DocumentKind::Document,
        title: "Design".into(),
        base_revision: base,
        request_id: request.into(),
        content: content.into(),
        media: Vec::new(),
        deleted: false,
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database in ROVAR_TEST_DATABASE_URL"]
async fn isolated_accounts_atomic_versions_retries_and_encrypted_restart() {
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        server: bootstrap::config::Server {
            bind: "127.0.0.1:0".into(),
            public_origin: "http://localhost".into(),
        },
        database: bootstrap::config::Database {
            url: std::env::var("ROVAR_TEST_DATABASE_URL").unwrap(),
        },
        storage: bootstrap::config::Storage {
            directory: root.path().into(),
        },
        registration: bootstrap::config::Registration::default(),
    };
    let app = bootstrap::build(&config).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user_a = format!("a-{suffix}");
    let user_b = format!("b-{suffix}");
    app.auth
        .create_user(&user_a, "integration-password-a")
        .await
        .unwrap();
    app.auth
        .create_user(&user_b, "integration-password-b")
        .await
        .unwrap();
    assert!(matches!(
        app.auth
            .login(user_a.clone(), "wrong".into(), Default::default())
            .await,
        Err(Error::Unauthorized)
    ));
    let a = app
        .auth
        .login(user_a, "integration-password-a".into(), Default::default())
        .await
        .unwrap();
    let b = app
        .auth
        .login(user_b, "integration-password-b".into(), Default::default())
        .await
        .unwrap();
    let a_space = &a.identity.spaces[0].id;
    let b_space = &b.identity.spaces[0].id;
    let id = uuid::Uuid::new_v4().to_string();
    let request = uuid::Uuid::new_v4().to_string();
    let original = b"private-document-content";
    let saved = app
        .documents
        .save(
            &a.identity.user_id,
            a_space,
            command(&id, 0, &request, original),
        )
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    let replay = app
        .documents
        .save(
            &a.identity.user_id,
            a_space,
            command(&id, 0, &request, original),
        )
        .await
        .unwrap();
    assert_eq!(replay.revision, 1);
    let first_page = app
        .documents
        .changes(&a.identity.user_id, a_space, 0)
        .await
        .unwrap();
    assert_eq!(
        first_page.cursor, 1,
        "Idempotent retries must not consume another cursor"
    );
    assert_eq!(first_page.documents.len(), 1);
    assert!(
        app.documents
            .changes(&a.identity.user_id, a_space, 1)
            .await
            .unwrap()
            .documents
            .is_empty()
    );
    assert!(matches!(
        app.documents.changes(&b.identity.user_id, a_space, 0).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents
            .save(
                &a.identity.user_id,
                a_space,
                command(&id, 0, &request, b"changed retry")
            )
            .await,
        Err(Error::Conflict)
    ));
    assert!(matches!(
        app.documents.read(&b.identity.user_id, b_space, &id).await,
        Err(Error::NotFound)
    ));
    assert!(
        app.documents
            .list(&b.identity.user_id, b_space)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.documents
            .read(&a.identity.user_id, a_space, &id)
            .await
            .unwrap()
            .content,
        original
    );

    let r1 = uuid::Uuid::new_v4().to_string();
    let r2 = uuid::Uuid::new_v4().to_string();
    let (first, second) = tokio::join!(
        app.documents.save(
            &a.identity.user_id,
            a_space,
            command(&id, 1, &r1, b"first edit")
        ),
        app.documents.save(
            &a.identity.user_id,
            a_space,
            command(&id, 1, &r2, b"second edit")
        )
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(matches!(first, Err(Error::Conflict)) || matches!(second, Err(Error::Conflict)));
    let current = app
        .documents
        .read(&a.identity.user_id, a_space, &id)
        .await
        .unwrap();
    assert_eq!(current.document.revision, 2);
    let second_page = app
        .documents
        .changes(&a.identity.user_id, a_space, first_page.cursor)
        .await
        .unwrap();
    assert_eq!(
        second_page.cursor, 2,
        "A rejected competing write must not publish a change"
    );
    assert_eq!(second_page.documents[0].revision, 2);
    let reopened = bootstrap::build(&config).await.unwrap();
    assert_eq!(
        reopened
            .documents
            .read(&a.identity.user_id, a_space, &id)
            .await
            .unwrap()
            .content,
        current.content
    );
    for entry in std::fs::read_dir(root.path().join("blobs")).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes.windows(original.len()).any(|part| part == original));
    }
    let mut deleted = command(&id, 2, &uuid::Uuid::new_v4().to_string(), &[]);
    deleted.deleted = true;
    app.documents
        .save(&a.identity.user_id, a_space, deleted)
        .await
        .unwrap();
    assert!(matches!(
        app.documents.read(&a.identity.user_id, a_space, &id).await,
        Err(Error::NotFound)
    ));
    assert!(
        app.documents
            .list(&a.identity.user_id, a_space)
            .await
            .unwrap()
            .iter()
            .any(|item| item.id == id && item.deleted)
    );
    let deleted_page = app
        .documents
        .changes(&a.identity.user_id, a_space, second_page.cursor)
        .await
        .unwrap();
    assert_eq!(deleted_page.cursor, 3);
    assert_eq!(deleted_page.documents.len(), 1);
    assert!(deleted_page.documents[0].deleted);
    assert!(
        app.documents
            .metadata(&a.identity.user_id, a_space, &id)
            .await
            .unwrap()
            .deleted
    );
    // The first scan is bounded. Objects edited between pages move forward in
    // the feed and are still seen; historical states need not be downloaded.
    let mut ids = Vec::new();
    for _ in 0..257 {
        let object_id = uuid::Uuid::new_v4().to_string();
        app.documents
            .save(
                &a.identity.user_id,
                a_space,
                command(
                    &object_id,
                    0,
                    &uuid::Uuid::new_v4().to_string(),
                    b"page item",
                ),
            )
            .await
            .unwrap();
        ids.push(object_id);
    }
    let page = app
        .documents
        .changes(&a.identity.user_id, a_space, 3)
        .await
        .unwrap();
    assert_eq!(page.documents.len(), 256);
    assert!(page.has_more);
    app.documents
        .save(
            &a.identity.user_id,
            a_space,
            command(
                &ids[0],
                1,
                &uuid::Uuid::new_v4().to_string(),
                b"edited between pages",
            ),
        )
        .await
        .unwrap();
    let tail = app
        .documents
        .changes(&a.identity.user_id, a_space, page.cursor)
        .await
        .unwrap();
    assert_eq!(tail.documents.len(), 2);
    assert!(!tail.has_more);
    assert_eq!(tail.documents[0].id, ids[256]);
    assert_eq!(tail.documents[1].id, ids[0]);
    assert_eq!(tail.documents[1].revision, 2);
    assert!(
        app.documents
            .changes(&a.identity.user_id, a_space, tail.cursor)
            .await
            .unwrap()
            .documents
            .is_empty()
    );
    let media_bytes = format!("shared media {suffix}").into_bytes();
    let hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&media_bytes))
    };
    let media = crate::domain::document::Media {
        hash: hash.clone(),
        length: media_bytes.len() as u64,
    };
    let metadata_file = root.path().join("metadata.rovar");
    let mut writer = rovar_format::Writer::create(&metadata_file).unwrap();
    writer
        .put_bytes("document", "json", br#"{"name":"Shared media"}"#)
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let metadata = std::fs::read(&metadata_file).unwrap();
    let media_id = uuid::Uuid::new_v4().to_string();
    let media_request = uuid::Uuid::new_v4().to_string();
    let media_command = || {
        let mut input = command(&media_id, 0, &media_request, &metadata);
        input.media = vec![media.clone()];
        input
    };
    assert!(
        matches!(
            app.documents
                .save(&a.identity.user_id, a_space, media_command())
                .await,
            Err(Error::Invalid(_))
        ),
        "A revision cannot commit before its media exists"
    );
    assert!(matches!(
        app.documents
            .upload_media_chunk(
                &a.identity.user_id,
                a_space,
                media.clone(),
                0,
                futures_util::stream::iter([Ok(b"wrong bytes".to_vec())])
            )
            .await,
        Err(Error::Invalid(_))
    ));
    app.documents
        .upload_media_chunk(
            &a.identity.user_id,
            a_space,
            media.clone(),
            0,
            futures_util::stream::iter([Ok(media_bytes.clone())]),
        )
        .await
        .unwrap();
    app.documents
        .finish_media_upload(&a.identity.user_id, a_space, media.clone())
        .await
        .unwrap();
    let blob_count = std::fs::read_dir(root.path().join("blobs"))
        .unwrap()
        .count();
    app.documents
        .upload_media_chunk(
            &a.identity.user_id,
            a_space,
            media.clone(),
            0,
            futures_util::stream::iter([Ok(media_bytes.clone())]),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_dir(root.path().join("blobs"))
            .unwrap()
            .count(),
        blob_count,
        "Duplicate uploads must reuse the encrypted resource"
    );
    assert!(
        app.documents
            .missing_media(&a.identity.user_id, a_space, std::slice::from_ref(&media))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.documents
            .missing_media(&b.identity.user_id, b_space, std::slice::from_ref(&media))
            .await
            .unwrap(),
        vec![hash.clone()]
    );
    assert!(matches!(
        app.documents
            .download_media(&b.identity.user_id, a_space, &hash)
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents
            .download_media(&b.identity.user_id, b_space, &hash)
            .await,
        Err(Error::NotFound)
    ));
    app.documents
        .save(&a.identity.user_id, a_space, media_command())
        .await
        .unwrap();
    app.documents
        .save(&a.identity.user_id, a_space, media_command())
        .await
        .unwrap();
    let reopened = bootstrap::build(&config).await.unwrap();
    let transfer = reopened
        .documents
        .transfer(&a.identity.user_id, a_space, &media_id)
        .await
        .unwrap();
    assert_eq!(transfer.content, metadata);
    assert_eq!(transfer.media[0].hash, hash);
    let complete = reopened
        .documents
        .read(&a.identity.user_id, a_space, &media_id)
        .await
        .unwrap();
    assert!(complete.media.is_empty());
    let complete_file = root.path().join("complete.rovar");
    std::fs::write(&complete_file, complete.content).unwrap();
    assert_eq!(
        rovar_format::Reader::open(&complete_file)
            .unwrap()
            .read(&format!("media/{hash}"), media.length)
            .unwrap(),
        media_bytes
    );
    app.auth.logout(&a.token).await.unwrap();
    assert!(matches!(
        app.auth.authenticate(&a.token).await,
        Err(Error::Unauthorized)
    ));
}
