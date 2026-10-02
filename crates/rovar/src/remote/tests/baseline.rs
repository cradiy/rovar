use super::*;

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
        r.reconcile_baseline(&path).unwrap();
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
        assert!(!r.link(&path).unwrap().dirty);
        r.changed(&path, Some("Renamed".into()), false, cx);
        assert!(r.link(&path).unwrap().dirty);
        r.changed(&path, Some("Design".into()), false, cx);
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
        assert!(!r.link(&path).unwrap().dirty);
        let mut catalog: Catalog =
            serde_json::from_slice(&std::fs::read(root.path().join("servers.json")).unwrap())
                .unwrap();
        let link = catalog.links.remove(&path).unwrap();
        assert!(r.read_baseline(&link).unwrap().is_some());
        assert!(!link.dirty);
        // Unknown upload outcomes must still be acknowledged after an undo.
        write_atomic(&r.pending_path(&link), b"pending").unwrap();
        r.reconcile_baseline(&path).unwrap();
        assert!(r.link(&path).unwrap().dirty);
    });
}
