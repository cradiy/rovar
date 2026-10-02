use super::*;
use gpui::TestAppContext;

fn object() -> Object {
    Object {
        id: "document".into(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    }
}

fn baseline(root: &Path, content: usize) -> String {
    let bytes = serde_json::to_vec(&baseline::Baseline {
        object: object(),
        content: STANDARD.encode(content.to_string()),
        transfer: None,
    })
    .unwrap();
    let key = hex::encode(Sha256::digest(&bytes));
    write_atomic(&root.join("baselines").join(&key), &bytes).unwrap();
    key
}

fn link(key: &str) -> Link {
    Link {
        connection: "account".into(),
        object: object(),
        baseline: Some(key.into()),
        dirty: false,
        digest: String::new(),
        conflict: false,
        error: None,
    }
}

fn save_catalog(root: &Path, catalog: &Catalog) {
    write_atomic(
        &root.join("servers.json"),
        &serde_json::to_vec(catalog).unwrap(),
    )
    .unwrap();
}

#[test]
fn deletion_rechecks_live_durable_and_recovery_references_after_scan() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let keys: Vec<_> = (0..6).map(|n| baseline(root, n)).collect();
    save_catalog(root, &Catalog::default());
    let batch = candidates(root, BTreeSet::new()).unwrap();
    assert_eq!(batch.len(), 6);

    let mut durable = Catalog::default();
    durable.links.insert("saved.rovar".into(), link(&keys[0]));
    let mut rejected = link(&keys[1]);
    rejected.dirty = true;
    rejected.conflict = true;
    durable.links.insert("conflict.rovar".into(), rejected);
    save_catalog(root, &durable);
    // The in-memory catalog can differ after a failed persist. Both versions
    // must survive, not just whichever was seen by the background scan.
    let mut current = Catalog::default();
    current.links.insert("live.rovar".into(), link(&keys[2]));
    write_atomic(
        &root.join("incoming.json"),
        &serde_json::to_vec(&serde_json::json!({
            "path": "download.rovar",
            "previous": link(&keys[3]),
            "next": link(&keys[4]),
        }))
        .unwrap(),
    )
    .unwrap();
    let pending = root.join("pending/account/document.json");
    let request = serde_json::to_vec(&PendingSave {
        input: Save {
            kind: Kind::Document,
            title: "Local edit".into(),
            base_revision: 1,
            request_id: "rejected".into(),
            content: STANDARD.encode(b"unsynced content"),
            media: Vec::new(),
            deleted: false,
        },
        delta: Some("frozen patch".into()),
    })
    .unwrap();
    write_atomic(&pending, &request).unwrap();
    let document = root.join("conflict.rovar");
    write_atomic(&document, b"later local edit").unwrap();

    prune(root, &current, &batch).unwrap();
    for key in &keys[..5] {
        assert!(baseline::read(root, &link(key)).unwrap().is_some());
    }
    assert!(!root.join("baselines").join(&keys[5]).exists());
    assert_eq!(fs::read(&pending).unwrap(), request);
    assert_eq!(fs::read(&document).unwrap(), b"later local edit");
    assert!(
        candidates(root, live_references(&current))
            .unwrap()
            .is_empty()
    );

    // References disappearing makes the same baselines reclaimable later.
    save_catalog(root, &Catalog::default());
    fs::remove_file(root.join("incoming.json")).unwrap();
    let batch = candidates(root, BTreeSet::new()).unwrap();
    prune(root, &Catalog::default(), &batch).unwrap();
    assert!(
        fs::read_dir(root.join("baselines"))
            .unwrap()
            .next()
            .is_none()
    );
    assert_eq!(fs::read(pending).unwrap(), request);
}

#[test]
fn unreadable_reference_state_aborts_before_any_deletion() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let key = baseline(root, 1);
    let catalog = root.join("servers.json");
    let journal = root.join("incoming.json");
    save_catalog(root, &Catalog::default());
    let batch = candidates(root, BTreeSet::new()).unwrap();
    fs::remove_file(&catalog).unwrap();
    assert!(prune(root, &Catalog::default(), &batch).is_err());
    write_atomic(&catalog, b"broken catalog").unwrap();
    assert!(candidates(root, BTreeSet::new()).is_err());
    assert!(prune(root, &Catalog::default(), &batch).is_err());
    save_catalog(root, &Catalog::default());
    write_atomic(&journal, b"broken recovery record").unwrap();
    assert!(candidates(root, BTreeSet::new()).is_err());
    assert!(prune(root, &Catalog::default(), &batch).is_err());
    fs::remove_file(&journal).unwrap();
    fs::create_dir(&journal).unwrap();
    assert!(prune(root, &Catalog::default(), &batch).is_err());
    assert!(baseline::read(root, &link(&key)).unwrap().is_some());
}

#[gpui::test]
fn idle_sync_reclaims_in_bounded_batches_and_defers_while_busy(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let keep = baseline(root, 0);
    let obsolete: Vec<_> = (1..=BATCH * 2 + 3).map(|n| baseline(root, n)).collect();
    let mut catalog = Catalog::default();
    catalog.links.insert("active.rovar".into(), link(&keep));
    save_catalog(root, &catalog);
    let directory = root.join("baselines");
    let temporary = directory.join(".in-progress.tmp");
    fs::write(&temporary, b"unpublished").unwrap();
    let foreign_directory = directory.join("a".repeat(64));
    fs::create_dir(&foreign_directory).unwrap();
    #[cfg(unix)]
    let symlink = {
        let symlink = directory.join("b".repeat(64));
        std::os::unix::fs::symlink(&temporary, &symlink).unwrap();
        symlink
    };
    let remote = cx.update(|cx| Remote::shared(root, cx));
    remote.update(cx, |r, cx| r.sync(cx));
    cx.run_until_parked();
    assert!(obsolete.iter().all(|key| directory.join(key).exists()));
    remote.update(cx, |r, cx| {
        assert_eq!(r.baseline_cleanup.pending.as_ref().unwrap().len(), BATCH);
        r.busy = true;
        r.sync(cx);
        assert!(obsolete.iter().all(|key| directory.join(key).exists()));
        r.busy = false;
        r.sync(cx);
    });
    assert_eq!(
        obsolete
            .iter()
            .filter(|key| directory.join(key).exists())
            .count(),
        BATCH + 3
    );
    for _ in 0..2 {
        cx.run_until_parked();
        remote.update(cx, |r, cx| r.sync(cx));
    }
    assert!(obsolete.iter().all(|key| !directory.join(key).exists()));
    assert!(baseline::read(root, &link(&keep)).unwrap().is_some());
    assert!(temporary.exists() && foreign_directory.is_dir());
    #[cfg(unix)]
    assert!(symlink.is_symlink());

    // Empty/small final batches stop scanning until the next maintenance hour.
    let later = baseline(root, BATCH * 3);
    remote.update(cx, |r, cx| r.sync(cx));
    cx.run_until_parked();
    assert!(directory.join(&later).exists());
    remote.update(cx, |r, cx| {
        r.baseline_cleanup.last =
            Some(web_time::Instant::now() - std::time::Duration::from_secs(INTERVAL + 1));
        r.sync(cx);
    });
    cx.run_until_parked();
    remote.update(cx, |r, cx| r.sync(cx));
    assert!(!directory.join(later).exists());
}
