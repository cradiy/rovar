use crate::{
    bootstrap::{self, config::*},
    domain::{
        document::{DocumentKind, SaveDocument},
        error::Error,
    },
};

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database in ROVAR_TEST_DATABASE_URL"]
async fn registration_invitation_membership_and_space_isolation() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Config {
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
        registration: Registration {
            personal: false,
            teams: true,
        },
    };
    let app = bootstrap::build(&config).await.unwrap();
    let owner_name = uuid::Uuid::new_v4().to_string();
    let password = "abc123";
    for short in ["12345", "你好世界啊", "😀😀😀😀😀"] {
        assert!(matches!(
            app.auth
                .register(&owner_name, short, Some("Design team"), Default::default())
                .await,
            Err(Error::Invalid(_))
        ));
    }
    app.auth
        .create_user(&uuid::Uuid::new_v4().to_string(), "你好世界朋友")
        .await
        .unwrap();
    assert!(matches!(
        app.auth
            .register(&owner_name, password, None, Default::default())
            .await,
        Err(Error::Forbidden)
    ));
    let owner = app
        .auth
        .register(
            &owner_name,
            password,
            Some("Design team"),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(owner.identity.spaces.len(), 2);
    assert!(matches!(
        app.auth
            .register(&owner_name, password, Some("Duplicate"), Default::default())
            .await,
        Err(Error::AlreadyExists)
    ));
    let team = &owner
        .identity
        .spaces
        .iter()
        .find(|s| s.kind == "team")
        .unwrap()
        .id;
    let personal = &owner
        .identity
        .spaces
        .iter()
        .find(|s| s.kind == "personal")
        .unwrap()
        .id;
    config.registration = Registration {
        personal: true,
        teams: false,
    };
    let closed = bootstrap::build(&config).await.unwrap();
    let member = closed
        .auth
        .register(
            &uuid::Uuid::new_v4().to_string(),
            password,
            None,
            Default::default(),
        )
        .await
        .unwrap();
    let outsider = closed
        .auth
        .register(
            &uuid::Uuid::new_v4().to_string(),
            password,
            None,
            Default::default(),
        )
        .await
        .unwrap();
    let member_id = &member.identity.user_id;
    let owner_id = &owner.identity.user_id;
    assert!(matches!(
        closed
            .auth
            .register(
                &uuid::Uuid::new_v4().to_string(),
                password,
                Some("Closed"),
                Default::default()
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        closed.spaces.create(owner_id, "Closed").await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.spaces.invite(member_id, team).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.spaces.invite(owner_id, personal).await,
        Err(Error::NotFound)
    ));
    let old_code = app.spaces.invite(owner_id, team).await.unwrap();
    let code = app.spaces.invite(owner_id, team).await.unwrap();
    assert!(closed.spaces.join(member_id, &old_code.code).await.is_err());
    closed.spaces.join(member_id, &code.code).await.unwrap();
    assert!(
        closed
            .spaces
            .join(&outsider.identity.user_id, &code.code)
            .await
            .is_err()
    );
    assert_eq!(
        closed.spaces.members(member_id, team).await.unwrap().len(),
        2
    );
    assert!(matches!(
        app.spaces.invite(member_id, team).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.spaces.remove(member_id, team, owner_id).await,
        Err(Error::Forbidden)
    ));
    assert!(app.spaces.remove(owner_id, team, owner_id).await.is_err());

    // Identical document IDs in different spaces never alias, including components.
    let id = uuid::Uuid::new_v4().to_string();
    let command = |content: &[u8], kind| SaveDocument {
        id: id.clone(),
        kind,
        title: "Shared".into(),
        base_revision: 0,
        request_id: uuid::Uuid::new_v4().to_string(),
        content: content.into(),
        deleted: false,
    };
    app.documents
        .save(
            owner_id,
            personal,
            command(b"private", DocumentKind::Document),
        )
        .await
        .unwrap();
    app.documents
        .save(owner_id, team, command(b"shared", DocumentKind::Component))
        .await
        .unwrap();
    assert_eq!(
        app.documents
            .read(member_id, team, &id)
            .await
            .unwrap()
            .content,
        b"shared"
    );
    assert_eq!(
        app.documents
            .read(owner_id, personal, &id)
            .await
            .unwrap()
            .content,
        b"private"
    );
    assert!(matches!(
        app.documents.read(member_id, personal, &id).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents.list(member_id, personal).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents
            .save(
                member_id,
                personal,
                command(b"attack", DocumentKind::Document)
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents.list(&outsider.identity.user_id, team).await,
        Err(Error::Forbidden)
    ));
    let color_id = uuid::Uuid::new_v4().to_string();
    for (space, content) in [
        (personal, b"private-color".as_slice()),
        (team, b"team-color".as_slice()),
    ] {
        let mut color = command(content, DocumentKind::ColorStyle);
        color.id = color_id.clone();
        let saved = app.documents.save(owner_id, space, color).await.unwrap();
        assert_eq!(saved.kind, DocumentKind::ColorStyle);
        assert_eq!(
            app.documents
                .read(owner_id, space, &color_id)
                .await
                .unwrap()
                .content,
            content
        );
    }
    assert!(matches!(
        app.documents.read(member_id, personal, &color_id).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        app.documents
            .read(member_id, team, &color_id)
            .await
            .unwrap()
            .content,
        b"team-color"
    );
    let mut edit = command(b"edited-color", DocumentKind::ColorStyle);
    edit.id = color_id.clone();
    edit.base_revision = 1;
    assert_eq!(
        app.documents
            .save(member_id, team, edit)
            .await
            .unwrap()
            .revision,
        2
    );
    let mut stale = command(b"stale-color", DocumentKind::ColorStyle);
    stale.id = color_id.clone();
    stale.base_revision = 1;
    assert!(matches!(
        app.documents.save(owner_id, team, stale).await,
        Err(Error::Conflict)
    ));
    let mut delete = command(b"", DocumentKind::ColorStyle);
    delete.id = color_id.clone();
    delete.base_revision = 2;
    delete.deleted = true;
    assert!(
        app.documents
            .save(owner_id, team, delete)
            .await
            .unwrap()
            .deleted
    );
    assert!(matches!(
        app.documents.read(member_id, team, &color_id).await,
        Err(Error::NotFound)
    ));
    assert_eq!(
        app.documents
            .read(owner_id, personal, &color_id)
            .await
            .unwrap()
            .content,
        b"private-color"
    );
    app.spaces.remove(owner_id, team, member_id).await.unwrap();
    assert_eq!(
        app.auth
            .authenticate(&member.token)
            .await
            .unwrap()
            .spaces
            .len(),
        1
    );
    assert!(matches!(
        app.documents.read(member_id, team, &id).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        app.documents
            .save(
                member_id,
                team,
                command(b"revoked", DocumentKind::Component)
            )
            .await,
        Err(Error::Forbidden)
    ));
    let code = app.spaces.invite(owner_id, team).await.unwrap();
    let (first, second) = tokio::join!(
        app.spaces.join(member_id, &code.code),
        app.spaces.join(&outsider.identity.user_id, &code.code)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let joined = if first.is_ok() {
        member_id
    } else {
        &outsider.identity.user_id
    };
    app.spaces.remove(joined, team, joined).await.unwrap();
    assert!(matches!(
        app.documents.list(joined, team).await,
        Err(Error::Forbidden)
    ));
}
