use super::*;

fn page(objects: Vec<Object>, cursor: i64) -> (u16, serde_json::Value) {
    (
        200,
        serde_json::to_value(rovar_api::Changes {
            objects,
            cursor,
            has_more: false,
        })
        .unwrap(),
    )
}

fn snapshot(object: &Object, bytes: &[u8]) -> (u16, serde_json::Value) {
    (
        200,
        serde_json::to_value(Snapshot {
            media: Vec::new(),
            object: object.clone(),
            content: STANDARD.encode(bytes),
        })
        .unwrap(),
    )
}

#[gpui::test]
fn deferred_changes_survive_restart_and_missing_caches_are_repaired(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"base").unwrap();
    let mut object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let session = (200, serde_json::to_value(identity()).unwrap());
    let original = object.clone();
    object.revision = 2;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, _, thread) = server_with_requests(
        vec![
            session.clone(),
            page(vec![object.clone()], 8),
            session.clone(),
            page(vec![], 8),
            snapshot(&object, b"remote edit"),
            session.clone(),
            page(vec![], 8),
            snapshot(&object, b"remote edit"),
            session,
            page(
                vec![Object {
                    revision: 3,
                    deleted: true,
                    ..object.clone()
                }],
                9,
            ),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection =
        cx.update(|cx| confirmed_document(&remote, url.clone(), path.clone(), original, cx));
    std::fs::write(&path, b"local edit").unwrap();
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(std::fs::read(&path).unwrap(), b"local edit");
        assert_eq!(r.catalog.directories[&connection].cursor, 8);
        assert_eq!(
            r.catalog.directories[&connection].pending[&object.id].revision,
            2
        );
        // Reload the persisted catalog, as a new process would, then reconnect.
        r.catalog =
            serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
                .unwrap();
        r.connect(url, identity(), "token".into(), cx).unwrap();
        std::fs::write(&path, b"base").unwrap();
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(std::fs::read(&path).unwrap(), b"remote edit");
        assert!(r.catalog.directories[&connection].pending.is_empty());
        assert!(!r.link(&path).unwrap().dirty);
        std::fs::remove_file(&path).unwrap();
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(std::fs::read(&path).unwrap(), b"remote edit");
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, _| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(!path.exists());
        assert!(r.link(&path).unwrap().object.deleted);
        assert_eq!(r.catalog.directories[&connection].cursor, 9);
    });
    thread.join().unwrap();
    let changes: Vec<_> = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.contains("/changes?"))
        .cloned()
        .collect();
    assert_eq!(
        changes,
        [0, 8, 8, 8]
            .map(|cursor| format!("GET /api/v1/spaces/personal/changes?after={cursor} HTTP/1.1"))
    );
}

#[gpui::test]
fn failed_download_does_not_advance_the_cursor(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"base").unwrap();
    let mut object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let original = object.clone();
    object.revision = 2;
    let session = (200, serde_json::to_value(identity()).unwrap());
    let (url, _, thread) = server(vec![
        session.clone(),
        page(vec![object.clone()], 2),
        (
            503,
            serde_json::json!({"code":"unavailable","message":"Retry later"}),
        ),
        session,
        page(vec![object.clone()], 2),
        snapshot(&object, b"remote edit"),
    ]);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = cx.update(|cx| confirmed_document(&remote, url, path.clone(), original, cx));
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert!(r.error.is_some());
        assert!(
            r.catalog
                .directories
                .get(&connection)
                .is_none_or(|d| d.cursor == 0)
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"base");
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, _| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.catalog.directories[&connection].cursor, 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"remote edit");
    });
    thread.join().unwrap();
}
