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
fn historical_upload_receipt_is_confirmed_before_fetching_another_clients_edit(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("design.rovar");
    std::fs::write(&path, b"base").unwrap();
    let original = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    let receipt = Object {
        revision: 2,
        modified: 2,
        ..original.clone()
    };
    let latest = Object {
        title: "Another client's title".into(),
        revision: 3,
        modified: 3,
        ..original.clone()
    };
    let session = (200, serde_json::to_value(identity()).unwrap());
    let (url, saves, thread) = server_with_requests(
        vec![
            session.clone(),
            (
                503,
                serde_json::json!({"code":"internal_error","message":"acknowledgement lost"}),
            ),
            session.clone(),
            (200, serde_json::to_value(&receipt).unwrap()),
            session,
            page(vec![latest.clone()], 3),
            snapshot(&latest, b"another client's edit"),
        ],
        Arc::new(Mutex::new(Vec::new())),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = cx.update(|cx| confirmed_document(&remote, url, path.clone(), original, cx));
    std::fs::write(&path, b"my upload").unwrap();
    remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
    sync(&remote, cx);
    sync(&remote, cx);
    remote.read_with(cx, |r, _| {
        let link = r.link(&path).unwrap();
        assert_eq!(link.object.revision, 2);
        assert!(!link.dirty && !link.conflict);
        assert!(!r.pending_path(link).exists());
        assert_eq!(
            STANDARD
                .decode(r.read_baseline(link).unwrap().unwrap().content)
                .unwrap(),
            b"my upload"
        );
    });
    remote.update(cx, |r, cx| r.refresh(connection, BTreeSet::new(), cx));
    wait_sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        let link = r.link(&path).unwrap();
        assert_eq!(link.object.revision, 3);
        assert_eq!(link.object.title, latest.title);
        assert!(!link.dirty && !link.conflict);
    });
    assert_eq!(std::fs::read(path).unwrap(), b"another client's edit");
    let saves = saves.lock().unwrap();
    assert_eq!(saves.len(), 2);
    assert_eq!(
        saves[0], saves[1],
        "Retry must retain the exact request identity and content"
    );
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
fn failed_download_retains_pending_version_and_manual_retry_bypasses_backoff(
    cx: &mut TestAppContext,
) {
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
        page(vec![], 2),
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
        let directory = &r.catalog.directories[&connection];
        assert_eq!(directory.cursor, 2);
        assert_eq!(directory.pending[&object.id].revision, 2);
        assert!(!directory.ready(&object.id));
        assert!(r.download_failed(r.link(&path).unwrap()));
        assert!(r.download_pending(r.link(&path).unwrap()));
        assert_eq!(std::fs::read(&path).unwrap(), b"base");
        r.retry(cx);
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, _| {
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.catalog.directories[&connection].cursor, 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"remote edit");
        assert!(!r.download_failed(r.link(&path).unwrap()));
        assert!(!r.download_pending(r.link(&path).unwrap()));
    });
    thread.join().unwrap();
}

fn remote_object(number: u128) -> Object {
    Object {
        id: uuid::Uuid::from_u128(number).to_string(),
        kind: Kind::Document,
        title: format!("Document {number}"),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    }
}

#[gpui::test]
fn local_install_failure_does_not_block_an_unrelated_document(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("blocked");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("design.rovar");
    std::fs::write(&path, b"base").unwrap();
    let original = remote_object(1);
    let failed = Object {
        revision: 2,
        ..original.clone()
    };
    let healthy = remote_object(2);
    let (url, _, thread) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        page(vec![failed.clone(), healthy.clone()], 3),
        snapshot(&failed, b"downloaded"),
        snapshot(&healthy, b"healthy"),
    ]);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = cx.update(|cx| confirmed_document(&remote, url, path.clone(), original, cx));
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&parent).unwrap();
    std::fs::write(&parent, b"blocked parent").unwrap();
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        let directory = &r.catalog.directories[&connection];
        assert_eq!(directory.cursor, 3);
        assert!(directory.pending.contains_key(&failed.id));
        assert!(!directory.ready(&failed.id));
        let (path, link) = r
            .catalog
            .links
            .iter()
            .find(|(_, link)| link.object.id == healthy.id)
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"healthy");
        assert!(!r.download_failed(link));
        assert!(!root.path().join("incoming.json").exists());
    });
}

#[gpui::test]
fn failed_object_does_not_discard_successes_and_backoff_survives_restart(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let [first, broken, last] = [1, 2, 3].map(remote_object);
    let repaired = Object {
        revision: 2,
        ..broken.clone()
    };
    let session = (200, serde_json::to_value(identity()).unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (url, _, thread) = server_with_requests(
        vec![
            session.clone(),
            page(vec![first.clone(), broken.clone(), last.clone()], 3),
            snapshot(&first, b"first"),
            (
                503,
                serde_json::json!({"code":"unavailable","message":"Broken object"}),
            ),
            snapshot(&last, b"last"),
            session.clone(),
            page(vec![], 3),
            session,
            page(vec![repaired.clone()], 4),
            snapshot(&repaired, b"repaired"),
        ],
        requests.clone(),
    );
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = remote.update(cx, |r, cx| {
        r.connect(url.clone(), identity(), "token".into(), cx)
            .unwrap()
    });
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        let directory = &r.catalog.directories[&connection];
        assert_eq!(directory.cursor, 3);
        assert_eq!(directory.pending.len(), 1);
        assert_eq!(directory.attempts(&broken.id), 1);
        assert!(!directory.ready(&broken.id));
        for (object, bytes) in [(&first, b"first".as_slice()), (&last, b"last".as_slice())] {
            let (path, link) = r
                .catalog
                .links
                .iter()
                .find(|(_, link)| link.object.id == object.id)
                .unwrap();
            assert!(!link.dirty && !link.conflict);
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        r.catalog =
            serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
                .unwrap();
        r.connect(url, identity(), "token".into(), cx).unwrap();
        assert!(!r.catalog.directories[&connection].ready(&broken.id));
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert_eq!(
            r.catalog.directories[&connection].attempts(&broken.id),
            1,
            "Backoff must avoid fetching the same failed version again"
        );
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        let directory = &r.catalog.directories[&connection];
        assert_eq!(directory.cursor, 4);
        assert!(directory.pending.is_empty());
        assert!(directory.error().is_none());
        assert!(r.error.is_none(), "{:?}", r.error);
        let (path, _) = r
            .catalog
            .links
            .iter()
            .find(|(_, link)| link.object.id == broken.id)
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"repaired");
    });
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/transfer "))
            .count(),
        4
    );
}

#[gpui::test]
fn attempt_budget_and_retry_order_allow_later_objects_to_progress(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let objects: Vec<_> = (1..=17).map(remote_object).collect();
    let session = (200, serde_json::to_value(identity()).unwrap());
    let failure = (
        503,
        serde_json::json!({"code":"unavailable","message":"Unavailable"}),
    );
    let mut responses = vec![session.clone(), page(objects.clone(), 17)];
    responses.extend(std::iter::repeat_n(failure.clone(), 16));
    responses.extend([
        session,
        page(vec![], 17),
        snapshot(&objects[16], b"healthy"),
    ]);
    responses.extend(std::iter::repeat_n(failure, 15));
    let (url, _, thread) = server(responses);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = remote.update(cx, |r, cx| {
        r.connect(url, identity(), "token".into(), cx).unwrap()
    });
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, cx| {
        assert_eq!(r.catalog.directories[&connection].pending.len(), 17);
        // Even when all old failures become eligible, fresh work goes first.
        r.retry(cx);
        r.refresh(connection.clone(), BTreeSet::new(), cx);
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        assert!(
            r.catalog
                .links
                .values()
                .any(|link| link.object.id == objects[16].id)
        );
        assert!(
            r.catalog.directories[&connection]
                .pending
                .contains_key(&objects[0].id)
        );
    });
}

#[gpui::test]
fn authentication_failure_aborts_batch_without_becoming_an_object_retry(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = tempfile::tempdir().unwrap();
    let object = remote_object(1);
    let (url, _, thread) = server(vec![
        (200, serde_json::to_value(identity()).unwrap()),
        page(vec![object], 1),
        (
            401,
            serde_json::json!({"code":"unauthorized","message":"Session expired"}),
        ),
    ]);
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let connection = remote.update(cx, |r, cx| {
        r.connect(url, identity(), "token".into(), cx).unwrap()
    });
    remote.update(cx, |r, cx| {
        r.refresh(connection.clone(), BTreeSet::new(), cx)
    });
    wait_sync(&remote, cx);
    thread.join().unwrap();
    remote.read_with(cx, |r, _| {
        assert!(!r.connection(&connection).unwrap().authenticated);
        assert!(
            r.catalog
                .directories
                .get(&connection)
                .is_none_or(|directory| directory.cursor == 0)
        );
        assert!(r.catalog.links.is_empty());
    });
}
