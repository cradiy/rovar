use super::*;
use crate::{
    document::Document,
    scene::artboard::Rect,
    scene::shape::{Shape, ShapeKind},
};

#[gpui::test]
fn failed_color_writes_keep_the_latest_edit_for_refresh_retry(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    cx.run_until_parked();
    let file = root.path().join("components/colors.json");
    std::fs::create_dir_all(&file).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let style = |color| crate::scene::color_styles::ColorStyle {
        name: "Brand".into(),
        color: gpui::rgb(color),
        gradient: None,
    };
    library.update(cx, |library, cx| {
        library
            .set_color(id.clone(), Some(style(0xff0000)), cx)
            .unwrap();
        library
            .set_color(id.clone(), Some(style(0x00ff00)), cx)
            .unwrap();
        library.refresh(cx);
    });
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert!(!library.busy);
        assert!(library.error.is_some());
        assert!(library.has_pending_colors());
        assert_eq!(library.colors[&id], style(0x00ff00));
    });
    std::fs::remove_dir(&file).unwrap();
    library.update(cx, |library, cx| library.refresh(cx));
    cx.run_until_parked();
    assert_eq!(
        colors::read(file.parent().unwrap()).unwrap()[&id],
        style(0x00ff00)
    );
    library.read_with(cx, |library, _| {
        assert!(!library.busy && !library.has_pending_colors());
        assert!(library.error.is_none());
        assert_eq!(library.colors[&id], style(0x00ff00));
    });
}

fn fixture() -> Document {
    Document::single(crate::document::Page {
        name: "Page 1".into(),
        id: uuid::Uuid::new_v4().to_string(),
        boards: vec![],
        shapes: vec![Shape::new(
            1,
            None,
            ShapeKind::Rectangle,
            Rect {
                x: 0.,
                y: 0.,
                width: 80.,
                height: 40.,
            },
        )],
        texts: vec![],
        hierarchy: Default::default(),
        next_id: 2,
        assets: vec![],
    })
}

#[gpui::test]
fn catalog_recovers_editable_components_and_failed_overwrite_keeps_original(
    cx: &mut gpui::TestAppContext,
) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("components");
    let document = fixture();
    let json = serde_json::to_vec(&document).unwrap();
    store(
        &directory,
        "按钮 / primary",
        &json,
        &[],
        [80., 40.],
        &text_system,
    )
    .unwrap();
    let entries = catalog(&directory, false).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].fingerprint.is_none());
    assert_eq!(entries[0].name, "按钮 / primary");
    assert!(entries[0].preview.as_ref().unwrap().exists());
    assert_eq!(
        document::load(&entries[0].path)
            .unwrap()
            .into_document()
            .unwrap()
            .pages[0]
            .shapes,
        document.pages[0].shapes
    );
    assert!(
        store(
            &directory,
            "replacement",
            &json,
            &[],
            [80., 40.],
            &text_system
        )
        .is_err()
    );
    assert_eq!(
        catalog(&directory, false).unwrap()[0].name,
        "按钮 / primary"
    );
    assert_eq!(document::load(&entries[0].path).unwrap().json, json);
    assert!(store(&directory, " ", &json, &[], [80., 40.], &text_system).is_err());
    assert!(component_path(&directory, "../documents/other").is_err());
}

#[gpui::test]
fn sync_catalog_rejects_stale_files_and_detects_deletion_and_restoration(
    cx: &mut gpui::TestAppContext,
) {
    use crate::remote::{Remote, tests::identity};
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let (connection, directory) = remote.update(cx, |remote, cx| {
        let id = remote
            .connect(
                "https://example.test".into(),
                identity(),
                "token".into(),
                cx,
            )
            .unwrap();
        let directory = remote.library_root(&id).join("components");
        (id, directory)
    });
    let document = fixture();
    let text_system = cx.update(|cx| cx.text_system().clone());
    store(
        &directory,
        "Card",
        &serde_json::to_vec(&document).unwrap(),
        &[],
        [80., 40.],
        &text_system,
    )
    .unwrap();
    let path = component_path(&directory, &document.id).unwrap();
    let object = rovar_api::Object {
        id: uuid::Uuid::new_v4().to_string(),
        kind: rovar_api::Kind::Component,
        title: "Card".into(),
        revision: 1,
        created: 1,
        modified: 1,
        deleted: false,
    };
    cx.update(|cx| {
        crate::remote::tests::confirmed_document(
            &remote,
            "https://example.test".into(),
            path.clone(),
            object,
            cx,
        );
    });
    let session = remote.read_with(cx, |r, _| r.library_session(&directory).unwrap());
    assert_eq!(session.0, connection);
    let entries = catalog(&directory, true).unwrap();
    remote.update(cx, |r, cx| {
        assert!(r.library_changed(&session, &entries, cx));
        assert!(
            !r.link(&path).unwrap().dirty,
            "An unchanged download stays clean"
        );
    });
    {
        let mut writer = Writer::open(&path).unwrap();
        writer
            .put_bytes(
                "component",
                "json",
                &serde_json::to_vec(&Metadata {
                    name: "Renamed card".into(),
                    size: [80., 40.],
                })
                .unwrap(),
            )
            .unwrap();
        writer.commit().unwrap();
    }
    remote.update(cx, |r, cx| {
        assert!(!r.library_changed(&session, &entries, cx));
        assert_eq!(r.link(&path).unwrap().object.title, "Card");
        assert!(!r.link(&path).unwrap().dirty);
    });
    let entries = catalog(&directory, true).unwrap();
    remote.update(cx, |r, cx| {
        assert!(r.library_changed(&session, &entries, cx));
        assert_eq!(r.link(&path).unwrap().object.title, "Renamed card");
        assert!(r.link(&path).unwrap().dirty);
    });
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    remote.update(cx, |r, cx| {
        assert!(!r.library_changed(&session, &entries, cx));
        assert!(!r.link(&path).unwrap().object.deleted);
        assert!(r.library_changed(&session, &[], cx));
        assert!(r.link(&path).unwrap().object.deleted);
    });
    std::fs::write(&path, bytes).unwrap();
    remote.update(cx, |r, cx| {
        assert!(!r.library_changed(&session, &[], cx));
        assert!(r.link(&path).unwrap().object.deleted);
    });
    let entries = catalog(&directory, true).unwrap();
    remote.update(cx, |r, cx| {
        assert!(r.library_changed(&session, &entries, cx));
        assert!(!r.link(&path).unwrap().object.deleted);
        assert!(r.link(&path).unwrap().dirty);
        r.sign_out(&connection, cx);
        assert!(!r.library_changed(&session, &entries, cx));
    });
}

#[gpui::test]
fn library_refresh_retries_a_changed_session_and_keeps_corrupt_entries(
    cx: &mut gpui::TestAppContext,
) {
    use crate::remote::{Remote, tests::identity};
    let root = tempfile::tempdir().unwrap();
    let remote = cx.update(|cx| Remote::shared(root.path(), cx));
    let (connection, library_root) = remote.update(cx, |remote, cx| {
        let id = remote
            .connect(
                "https://example.test".into(),
                identity(),
                "token".into(),
                cx,
            )
            .unwrap();
        let root = remote.library_root(&id);
        (id, root)
    });
    let directory = library_root.join("components");
    let document = fixture();
    let path = component_path(&directory, &document.id).unwrap();
    let text_system = cx.update(|cx| cx.text_system().clone());
    store(
        &directory,
        "Card",
        &serde_json::to_vec(&document).unwrap(),
        &[],
        [80., 40.],
        &text_system,
    )
    .unwrap();
    let library = cx.update(|cx| {
        let library = Library::open(&library_root, cx);
        remote.update(cx, |remote, cx| remote.sign_out(&connection, cx));
        library
    });
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert!(library.ready && !library.busy);
        assert!(library.error.is_none());
        assert!(library.entries[0].fingerprint.is_some());
    });
    remote.read_with(cx, |remote, _| assert!(remote.link(&path).is_some()));
    std::fs::write(&path, b"damaged component").unwrap();
    library.update(cx, |library, cx| library.refresh(cx));
    cx.run_until_parked();
    library.read_with(cx, |library, _| {
        assert!(!library.busy);
        assert!(library.entries[0].error.is_some());
    });
    remote.read_with(cx, |remote, _| {
        assert!(!remote.link(&path).unwrap().object.deleted)
    });
}

#[gpui::test]
fn shared_catalog_rename_delete_and_restart_preserve_existing_copies(
    cx: &mut gpui::TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    let same = cx.update(|cx| Library::open(root.path(), cx));
    assert_eq!(library, same);
    cx.run_until_parked();
    let document = fixture();
    let json = serde_json::to_vec(&document).unwrap();
    library.update(cx, |lib, cx| {
        lib.save("Card".into(), json, vec![], [80., 40.], cx)
    });
    cx.run_until_parked();
    let entry = library.read_with(cx, |lib, _| {
        assert!(lib.error.is_none());
        lib.entries[0].clone()
    });
    let copy = document::load(&entry.path).unwrap();
    library.update(cx, |lib, cx| {
        lib.rename(entry.clone(), "Renamed 卡片".into(), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        same.read_with(cx, |lib, _| lib.entries[0].name.clone()),
        "Renamed 卡片"
    );
    assert_eq!(
        catalog(&root.path().join("components"), false).unwrap()[0].name,
        "Renamed 卡片"
    );
    library.update(cx, |lib, cx| lib.delete(entry.clone(), cx));
    cx.run_until_parked();
    assert!(same.read_with(cx, |lib, _| lib.entries.is_empty()));
    assert!(!entry.path.exists());
    assert_eq!(
        copy.into_document().unwrap().pages[0].shapes,
        document.pages[0].shapes
    );
}

#[gpui::test]
fn invalid_component_is_isolated_and_removal_survives_refresh(cx: &mut gpui::TestAppContext) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("components");
    let valid = fixture();
    let invalid = fixture();
    for doc in [&valid, &invalid] {
        store(
            &directory,
            "Card",
            &serde_json::to_vec(doc).unwrap(),
            &[],
            [80., 40.],
            &text_system,
        )
        .unwrap();
    }
    let invalid_path = component_path(&directory, &invalid.id).unwrap();
    let mut bytes = std::fs::read(&invalid_path).unwrap();
    bytes[12..20].fill(0);
    std::fs::write(&invalid_path, &bytes).unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    cx.run_until_parked();
    let failed = library.read_with(cx, |lib, _| {
        assert!(lib.ready);
        assert!(lib.error.is_none());
        assert_eq!(lib.entries.len(), 2);
        assert!(
            lib.entries
                .iter()
                .find(|e| e.id == valid.id)
                .unwrap()
                .error
                .is_none()
        );
        let failed = lib.entries.iter().find(|e| e.id == invalid.id).unwrap();
        assert!(failed.error.as_ref().unwrap().contains("format identifier"));
        assert!(failed.name.is_empty());
        failed.clone()
    });
    let another = fixture();
    library.update(cx, |lib, cx| {
        lib.save(
            "New".into(),
            serde_json::to_vec(&another).unwrap(),
            vec![],
            [80., 40.],
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(library.read_with(cx, |lib, _| lib.entries.len()), 3);
    library.update(cx, |lib, cx| lib.remove_failed(failed, cx));
    cx.run_until_parked();
    let entries = catalog(&directory, false).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.error.is_none()));
    assert!(!invalid_path.exists());
    let archived = std::fs::read_dir(directory.join("removed"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read(archived).unwrap(), bytes);
    assert!(document::load(component_path(&directory, &valid.id).unwrap().as_path()).is_ok());
}
