use super::*;
use crate::{
    artboard::Rect,
    document::Document,
    shape::{Shape, ShapeKind},
};

fn fixture() -> Document {
    Document {
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
    }
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
    let entries = catalog(&directory).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "按钮 / primary");
    assert!(entries[0].preview.as_ref().unwrap().exists());
    assert_eq!(
        document::load(&entries[0].path)
            .unwrap()
            .into_document()
            .unwrap()
            .shapes,
        document.shapes
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
    assert_eq!(catalog(&directory).unwrap()[0].name, "按钮 / primary");
    assert_eq!(document::load(&entries[0].path).unwrap().json, json);
    assert!(store(&directory, " ", &json, &[], [80., 40.], &text_system).is_err());
    assert!(component_path(&directory, "../documents/other").is_err());
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
        catalog(&root.path().join("components")).unwrap()[0].name,
        "Renamed 卡片"
    );
    library.update(cx, |lib, cx| lib.delete(entry.clone(), cx));
    cx.run_until_parked();
    assert!(same.read_with(cx, |lib, _| lib.entries.is_empty()));
    assert!(!entry.path.exists());
    assert_eq!(copy.into_document().unwrap().shapes, document.shapes);
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
    let entries = catalog(&directory).unwrap();
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
