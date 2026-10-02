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
    app.auth.logout(&a.token).await.unwrap();
    assert!(matches!(
        app.auth.authenticate(&a.token).await,
        Err(Error::Unauthorized)
    ));
}
