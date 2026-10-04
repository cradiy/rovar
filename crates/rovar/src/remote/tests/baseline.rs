use super::*;

fn reconcile(remote: &Entity<Remote>, path: &Path, cx: &mut TestAppContext) {
    remote.update(cx, |r, cx| {
        r.reconcile_baselines(vec![path.to_owned()], cx, |_, _| {});
    });
    wait_sync(remote, cx);
}

#[gpui::test]
fn edits_during_a_scan_keep_rejected_requests_until_a_fresh_scan(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("document.rovar");
    write_atomic(&path, b"confirmed").unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    cx.update(|cx| {
        confirmed_document(
            &remote,
            "https://example.test".into(),
            path.clone(),
            Object {
                id: uuid::Uuid::new_v4().to_string(),
                kind: Kind::Document,
                title: "Design".into(),
                revision: 1,
                created: 1,
                modified: 1,
                deleted: false,
            },
            cx,
        );
    });
    let pending = remote.update(cx, |r, cx| {
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.conflict = true;
        let pending = r.pending_path(&r.catalog.links[&path]);
        write_atomic(&pending, b"rejected request").unwrap();
        r.reconcile_baselines(vec![path.clone()], cx, |_, _| {});
        // Even when catalog fields return to their original values, this scan
        // must not retire the request on behalf of a later edit/undo.
        r.changed(&path, Some("Renamed".into()), false, cx);
        r.changed(&path, Some("Design".into()), false, cx);
        pending
    });
    wait_sync(&remote, cx);
    remote.update(cx, |r, _| {
        assert!(r.link(&path).unwrap().dirty);
        assert!(r.link(&path).unwrap().conflict);
        assert!(rovar_storage::exists(&pending));
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(!r.link(&path).unwrap().dirty);
        assert!(!r.link(&path).unwrap().conflict);
        assert!(!rovar_storage::exists(&pending));
    });
}

#[gpui::test]
fn upgrading_confirmed_legacy_node_ids_is_clean_but_replacing_a_node_is_not(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("legacy.rovar");
    let mut page = crate::document::Page::empty("Design".into());
    let mut shape = crate::scene::shape::Shape::new(
        1,
        None,
        crate::scene::shape::ShapeKind::Rectangle,
        crate::scene::artboard::Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 50.,
        },
    );
    shape.uid = uuid::Uuid::nil();
    page.shapes.push(shape);
    page.next_id = 2;
    page.hierarchy
        .effects
        .insert(1, vec![crate::scene::effects::Effect::default()]);
    let document = crate::document::Document::single(page);
    let text_system = cx.update(|cx| cx.text_system().clone());
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Kind::Document,
        title: "Design".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    cx.update(|cx| {
        confirmed_document(
            &remote,
            "https://example.test".into(),
            path.clone(),
            object.clone(),
            cx,
        )
    });
    remote.update(cx, |r, cx| {
        let mut old: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&document).unwrap()).unwrap();
        old["pages"][0].as_object_mut().unwrap().remove("next_id");
        let hierarchy = old["pages"][0]["hierarchy"].as_object_mut().unwrap();
        let mut shadows = hierarchy.remove("effects").unwrap();
        let shadow = shadows["1"][0].as_object_mut().unwrap();
        shadow.remove("type");
        shadow.remove("kind");
        hierarchy.insert("shadows".into(), shadows);
        let baseline = r
            .store_baseline(&object, &serde_json::to_vec(&old).unwrap())
            .unwrap();
        r.catalog.links.get_mut(&path).unwrap().baseline = Some(baseline);
        r.changed(&path, None, false, cx);
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(
            !r.link(&path).unwrap().dirty,
            "Upgrading identities and published effects must not create a local edit"
        );
    });
    let loaded = crate::document::load(&path).unwrap();
    let mut replaced = crate::document::Document::decode(&loaded.json).unwrap();
    replaced.pages[0].shapes[0].uid = uuid::Uuid::new_v4();
    crate::document::save(
        &path,
        &serde_json::to_vec(&replaced).unwrap(),
        &[],
        &loaded.json,
        &text_system,
    )
    .unwrap();
    remote.update(cx, |r, cx| {
        r.changed(&path, None, false, cx);
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(
            r.link(&path).unwrap().dirty,
            "Replacing a node remains an edit even when its appearance and handle match"
        );
    });
}

#[gpui::test]
fn confirmed_content_ignores_container_rewrites_and_undo_clears_pending_changes(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("document.rovar");
    let document =
        crate::document::Document::single(crate::document::Page::empty("Original".into()));
    let json = serde_json::to_vec(&document).unwrap();
    cx.update(|cx| crate::document::save_as(&path, &json, &[], cx.text_system()).unwrap());
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    remote.update(cx, |r, cx| {
        let id = r
            .connect(
                "https://example.test".into(),
                identity(),
                "token".into(),
                cx,
            )
            .unwrap();
        r.track(path.clone(), id, "Design".into(), Kind::Document, cx);
        let link = r.catalog.links.get_mut(&path).unwrap();
        link.object.revision = 1;
        link.digest = digest(&std::fs::read(&path).unwrap(), "Design", false);
        link.dirty = false;
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(r.link(&path).unwrap().baseline.is_some());
        r.persist();
    });
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    writer
        .put_bytes("preview", "png", b"derived preview")
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    remote.update(cx, |r, cx| {
        r.changed(&path, None, false, cx);
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, cx| {
        assert!(!r.link(&path).unwrap().dirty);
        r.changed(&path, Some("Renamed".into()), false, cx);
        assert!(r.link(&path).unwrap().dirty);
        r.changed(&path, Some("Design".into()), false, cx);
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(!r.link(&path).unwrap().dirty);
    });
    let mut changed = document.clone();
    changed.pages[0].name = "Changed".into();
    let changed = serde_json::to_vec(&changed).unwrap();
    cx.update(|cx| crate::document::save(&path, &changed, &[], &json, cx.text_system()).unwrap());
    remote.update(cx, |r, cx| {
        r.changed(&path, None, false, cx);
        assert!(r.link(&path).unwrap().dirty);
    });
    cx.update(|cx| crate::document::save(&path, &json, &[], &changed, cx.text_system()).unwrap());
    let mut undone = document.clone();
    undone.pages[0].next_id += 1;
    cx.update(|cx| {
        crate::document::save(
            &path,
            &serde_json::to_vec(&undone).unwrap(),
            &[],
            &json,
            cx.text_system(),
        )
        .unwrap()
    });
    remote.update(cx, |r, cx| {
        r.changed(&path, None, false, cx);
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(!r.link(&path).unwrap().dirty);
        let mut catalog: Catalog =
            serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
                .unwrap();
        let link = catalog.links.remove(&path).unwrap();
        assert!(r.read_baseline(&link).unwrap().is_some());
        assert!(!link.dirty);
        // Unknown upload outcomes must still be acknowledged after an undo.
        write_atomic(&r.pending_path(&link), b"pending").unwrap();
    });
    reconcile(&remote, &path, cx);
    remote.update(cx, |r, _| {
        assert!(r.link(&path).unwrap().dirty);
    });
}
