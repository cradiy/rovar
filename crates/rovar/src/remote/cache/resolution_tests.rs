use super::*;
use crate::remote::tests::resolve;
use gpui::TestAppContext;

fn setup(root: &Path, cx: &mut TestAppContext) -> (Entity<Remote>, PathBuf, Object, PathBuf) {
    let path = root.join("document.rovar");
    write_atomic(&path, b"local").unwrap();
    let remote = cx.update(|cx| Remote::shared(root, cx));
    let (reviewed, pending) = remote.update(cx, |r, cx| {
        let connection = r
            .connect(
                "https://example.test".into(),
                super::super::tests::identity(),
                "token".into(),
                cx,
            )
            .unwrap();
        r.track(
            path.clone(),
            connection,
            "Local title".into(),
            Kind::Document,
            cx,
        );
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.conflict = true;
        link.object.revision = 1;
        let mut reviewed = link.object.clone();
        reviewed.title = "Server title".into();
        let pending = r.pending_path(&r.link(&path).unwrap().clone());
        write_atomic(
            &pending,
            &serde_json::to_vec(&PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Local title".into(),
                    base_revision: 0,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    content: STANDARD.encode(b"local"),
                    media: vec![],
                    deleted: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
        assert!(r.persist());
        (reviewed, pending)
    });
    (remote, path, reviewed, pending)
}

fn reload(r: &mut Remote) {
    r.catalog =
        serde_json::from_slice(&rovar_storage::fs::read(r.root.join("servers.json")).unwrap())
            .unwrap();
}

#[gpui::test]
fn resolving_rejects_new_edits_and_sign_out_and_dropped_waiters_release_the_worker(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (remote, path, reviewed, pending) = setup(root.path(), cx);
    let request = rovar_storage::fs::read(&pending).unwrap();
    let snapshot = root.path().join("reviewed.rovar");
    write_atomic(&snapshot, b"server").unwrap();
    let task = remote.update(cx, |r, cx| {
        let task = r
            .resolve_conflict(&path, &reviewed, Some(snapshot.clone()), cx)
            .unwrap();
        r.changed(&path, Some("Rename".into()), false, cx);
        r.changed(&path, Some("Local title".into()), false, cx);
        task
    });
    assert!(cx.foreground_executor().clone().block_test(task).is_err());
    remote.read_with(cx, |r, _| {
        assert!(!r.busy && r.link(&path).unwrap().conflict);
    });
    assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"local");
    assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
    assert!(!root.path().join("incoming.json").exists());

    let task = remote.update(cx, |r, cx| {
        let task = r
            .resolve_conflict(&path, &reviewed, Some(snapshot), cx)
            .unwrap();
        let connection = r.link(&path).unwrap().connection.clone();
        r.sign_out(&connection, cx);
        task
    });
    assert!(cx.foreground_executor().clone().block_test(task).is_err());
    assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"local");
    assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
    remote.update(cx, |r, cx| {
        assert!(!r.busy && r.link(&path).unwrap().conflict);
        r.connect(
            "https://example.test".into(),
            crate::remote::tests::identity(),
            "token".into(),
            cx,
        )
        .unwrap();
        let task = r.resolve_conflict(&path, &reviewed, None, cx).unwrap();
        drop(task);
    });
    crate::remote::tests::wait_sync(&remote, cx);
    remote.read_with(cx, |r, _| {
        assert!(!r.busy);
        let link = r.link(&path).unwrap();
        assert!(link.dirty && !link.conflict);
    });
    assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"local");
    assert!(!pending.exists());
}

#[gpui::test]
fn resolution_recovers_both_choices_after_catalog_failure_and_keeps_later_requests(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (remote, path, reviewed, pending) = setup(root.path(), cx);
    let original_catalog = rovar_storage::fs::read(root.path().join("servers.json")).unwrap();
    let original_pending = rovar_storage::fs::read(&pending).unwrap();
    for use_server in [true, false] {
        write_atomic(&root.path().join("servers.json"), &original_catalog).unwrap();
        write_atomic(&path, b"local").unwrap();
        write_atomic(&pending, &original_pending).unwrap();
        let catalog = root.path().join("servers.json");
        remote.update(cx, |r, cx| {
            reload(r);
            // Authentication is intentionally not persisted in the catalog.
            r.connect(
                "https://example.test".into(),
                crate::remote::tests::identity(),
                "token".into(),
                cx,
            )
            .unwrap();
            let catalog = root.path().join("servers.json");
            // Force the real persist path to fail after the recovery record and
            // (for the server choice) the replacement document are durable.
            rovar_storage::fs::remove_file(&catalog).unwrap();
            rovar_storage::fs::create_dir(&catalog).unwrap();
        });
        let server = use_server.then_some(b"server".as_slice());
        let error = resolve(&remote, &path, &reviewed, server, cx).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Could not persist downloaded revision"),
            "{error}"
        );
        remote.update(cx, |r, _| {
            let journal = root.path().join("incoming.json");
            let record = rovar_storage::fs::read(&journal).unwrap();
            assert!(
                pending.exists(),
                "The request cannot be retired before catalog commit"
            );
            assert!(r.link(&path).unwrap().conflict);
            rovar_storage::fs::remove_dir(&catalog).unwrap();
            write_atomic(&catalog, &original_catalog).unwrap();
            reload(r);
            if use_server {
                let incoming: Incoming = serde_json::from_slice(&record).unwrap();
                let baseline = root
                    .path()
                    .join("baselines")
                    .join(incoming.next.baseline.unwrap());
                let confirmed = rovar_storage::fs::read(&baseline).unwrap();
                write_atomic(&baseline, b"damaged baseline").unwrap();
                assert!(recovery::recover_for_test(r).is_err());
                assert!(journal.exists() && pending.exists());
                write_atomic(&baseline, &confirmed).unwrap();
            }
            recovery::recover_for_test(r).unwrap();
            let link = r.link(&path).unwrap();
            assert!(!link.conflict);
            assert_eq!(link.object.revision, reviewed.revision);
            assert_eq!(link.dirty, !use_server);
            assert_eq!(link.baseline.is_some(), use_server);
            assert_eq!(
                rovar_storage::fs::read(&path).unwrap(),
                if use_server {
                    b"server".as_slice()
                } else {
                    b"local".as_slice()
                }
            );
            assert!(!pending.exists());
            assert!(!journal.exists());
            // An old journal surviving cleanup must not reset edits or delete
            // a new upload request for the same object.
            let mut next: PendingSave = serde_json::from_slice(&original_pending).unwrap();
            next.input.request_id = uuid::Uuid::new_v4().to_string();
            next.input.base_revision = reviewed.revision;
            next.input.content = STANDARD.encode(b"later edit");
            let next_bytes = serde_json::to_vec(&next).unwrap();
            write_atomic(&pending, &next_bytes).unwrap();
            write_atomic(&path, b"later edit").unwrap();
            let link = r.catalog.links.get_mut(&path).unwrap();
            link.object.title = "Later title".into();
            link.dirty = true;
            assert!(r.persist());
            write_atomic(&journal, &record).unwrap();
            reload(r);
            recovery::recover_for_test(r).unwrap();
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later edit");
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), next_bytes);
            assert_eq!(r.link(&path).unwrap().object.title, "Later title");
            assert!(r.link(&path).unwrap().dirty);
        });
    }
}

#[gpui::test]
fn failed_replacement_retains_conflict_and_request_without_overwriting_later_edits(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (remote, path, reviewed, pending) = setup(root.path(), cx);
    let request = rovar_storage::fs::read(&pending).unwrap();
    remote.update(cx, |_, _| {
        rovar_storage::fs::remove_file(&path).unwrap();
        rovar_storage::fs::create_dir(&path).unwrap();
    });
    assert!(resolve(&remote, &path, &reviewed, Some(b"server"), cx).is_err());
    remote.update(cx, |r, _| {
        assert!(root.path().join("incoming.json").exists());
        rovar_storage::fs::remove_dir(&path).unwrap();
        write_atomic(&path, b"later local edit").unwrap();
        reload(r);
        recovery::recover_for_test(r).unwrap();
        assert!(r.link(&path).unwrap().conflict);
        assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later local edit");
        assert!(!root.path().join("incoming.json").exists());
    });
}

#[gpui::test]
fn failed_baseline_preparation_leaves_local_file_and_request_untouched(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (remote, path, reviewed, pending) = setup(root.path(), cx);
    let request = rovar_storage::fs::read(&pending).unwrap();
    write_atomic(&root.path().join("baselines"), b"blocked").unwrap();
    assert!(resolve(&remote, &path, &reviewed, Some(b"server"), cx).is_err());
    remote.update(cx, |r, _| {
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"local");
        assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
        assert!(r.link(&path).unwrap().conflict);
        assert!(!root.path().join("incoming.json").exists());
    });
}
