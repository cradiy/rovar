use crate::{
    bootstrap::{self, config::*},
    domain::error::Error,
};

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database in ROVAR_TEST_DATABASE_URL"]
async fn password_changes_and_session_revocation_are_isolated_and_atomic() {
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        server: Server {
            bind: "127.0.0.1:0".into(),
            public_origin: "http://localhost".into(),
        },
        database: Database {
            url: std::env::var("ROVAR_TEST_DATABASE_URL").unwrap(),
        },
        storage: Storage {
            directory: root.path().into(),
        },
        registration: Registration::default(),
    };
    let app = bootstrap::build(&config).await.unwrap();
    let name = uuid::Uuid::new_v4().to_string();
    let outsider = uuid::Uuid::new_v4().to_string();
    app.auth.create_user(&name, "before123").await.unwrap();
    app.auth.create_user(&outsider, "outside123").await.unwrap();
    let a = app
        .auth
        .login(
            name.clone(),
            "before123".into(),
            crate::domain::identity::SessionDevice {
                system: "Linux".into(),
                name: "Design workstation".into(),
                client: "Rovar Desktop".into(),
            },
        )
        .await
        .unwrap();
    let b = app
        .auth
        .login(name.clone(), "before123".into(), Default::default())
        .await
        .unwrap();
    let other = app
        .auth
        .login(outsider, "outside123".into(), Default::default())
        .await
        .unwrap();
    let sessions = app.auth.sessions(&a.token).await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions.iter().filter(|s| s.current).count(), 1);
    let current = sessions.iter().find(|s| s.current).unwrap();
    assert_eq!(current.device.system, "Linux");
    assert_eq!(current.device.name, "Design workstation");
    assert_eq!(current.device.client, "Rovar Desktop");
    assert!(
        sessions
            .iter()
            .find(|s| !s.current)
            .unwrap()
            .device
            .name
            .is_empty()
    );
    let b_id = &sessions.iter().find(|s| !s.current).unwrap().id;
    assert!(matches!(
        app.auth.revoke_sessions(&other.token, Some(b_id)).await,
        Err(Error::NotFound)
    ));
    assert!(matches!(
        app.auth
            .change_password(&a.token, "wrong".into(), "after123".into())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        app.auth
            .change_password(&a.token, "before123".into(), "一二三四五".into())
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(app.auth.authenticate(&b.token).await.is_ok());
    // Two simultaneous changes cannot both accept the same old password.
    let (first, second) = tokio::join!(
        app.auth
            .change_password(&a.token, "before123".into(), "一二三四五六".into()),
        app.auth
            .change_password(&a.token, "before123".into(), "一二三四五六".into()),
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(app.auth.authenticate(&a.token).await.is_ok());
    assert!(matches!(
        app.auth.authenticate(&b.token).await,
        Err(Error::Unauthorized)
    ));
    assert!(app.auth.authenticate(&other.token).await.is_ok());
    assert!(matches!(
        app.auth
            .login(name.clone(), "before123".into(), Default::default())
            .await,
        Err(Error::Unauthorized)
    ));
    let new = app
        .auth
        .login(name, "一二三四五六".into(), Default::default())
        .await
        .unwrap();
    let sessions = app.auth.sessions(&a.token).await.unwrap();
    let new_id = &sessions.iter().find(|s| !s.current).unwrap().id;
    app.auth
        .revoke_sessions(&a.token, Some(new_id))
        .await
        .unwrap();
    assert!(matches!(
        app.auth.authenticate(&new.token).await,
        Err(Error::Unauthorized)
    ));
    app.auth.revoke_sessions(&a.token, None).await.unwrap();
    assert!(app.auth.authenticate(&a.token).await.is_ok());
    assert!(app.auth.authenticate(&other.token).await.is_ok());
    // A login that verified the previous hash before the change cannot mint a session afterward.
    use crate::application::ports::Accounts;
    let pool = sqlx::PgPool::connect(&config.database.url).await.unwrap();
    let repo = crate::infrastructure::postgres::auth::AuthRepository::new(pool);
    assert!(matches!(
        repo.create_session(
            &a.identity.user_id,
            b"stale-token",
            i64::MAX,
            "stale-password-hash",
            &Default::default(),
        )
        .await,
        Err(Error::Unauthorized)
    ));
}
