use super::*;
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
        remote.update(cx, |r, cx| {
            reload(r);
            let catalog = root.path().join("servers.json");
            // Force the real persist path to fail after the recovery record and
            // (for the server choice) the replacement document are durable.
            rovar_storage::fs::remove_file(&catalog).unwrap();
            rovar_storage::fs::create_dir(&catalog).unwrap();
            let server = use_server.then_some(b"server".as_slice());
            assert!(r.resolve_conflict(&path, &reviewed, server, cx).is_err());
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
                assert!(r.recover_incoming().is_err());
                assert!(journal.exists() && pending.exists());
                write_atomic(&baseline, &confirmed).unwrap();
            }
            r.recover_incoming().unwrap();
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
            r.recover_incoming().unwrap();
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
    remote.update(cx, |r, cx| {
        rovar_storage::fs::remove_file(&path).unwrap();
        rovar_storage::fs::create_dir(&path).unwrap();
        assert!(
            r.resolve_conflict(&path, &reviewed, Some(b"server"), cx)
                .is_err()
        );
        assert!(root.path().join("incoming.json").exists());
        rovar_storage::fs::remove_dir(&path).unwrap();
        write_atomic(&path, b"later local edit").unwrap();
        reload(r);
        r.recover_incoming().unwrap();
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
    remote.update(cx, |r, cx| {
        assert!(
            r.resolve_conflict(&path, &reviewed, Some(b"server"), cx)
                .is_err()
        );
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"local");
        assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
        assert!(r.link(&path).unwrap().conflict);
        assert!(!root.path().join("incoming.json").exists());
    });
}
