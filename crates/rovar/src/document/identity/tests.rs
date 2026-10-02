use super::*;
use crate::scene::{
    artboard::Rect,
    shape::{Shape, ShapeKind},
};

fn shape(id: usize) -> Shape {
    Shape::new(
        id,
        None,
        ShapeKind::Rectangle,
        Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 50.,
        },
    )
}

#[test]
fn legacy_nodes_upgrade_identically_but_parallel_insertions_have_distinct_identities() {
    let mut legacy = Page::empty("Design".into());
    let mut old_shape = shape(1);
    old_shape.uid = Uuid::nil();
    legacy.shapes.push(old_shape);
    legacy.next_id = 2;
    let bytes = serde_json::to_vec(&legacy).unwrap();
    let mut a = Page::decode(&bytes).unwrap();
    let mut b = Page::decode(&bytes).unwrap();
    assert_eq!(a.node_ids(), b.node_ids());
    assert!(!a.shapes[0].uid.is_nil());
    a.shapes.push(shape(2));
    b.shapes.push(shape(2));
    a.next_id = 3;
    b.next_id = 3;
    assert_ne!(
        a.shapes[1].uid, b.shapes[1].uid,
        "Offline insertions must not share an identity just because their handles match"
    );
    let mut reindexed = a.clone();
    reindexed.shapes[0].id = 40;
    reindexed.next_id = 41;
    assert_eq!(
        Page::decode(&serde_json::to_vec(&reindexed).unwrap())
            .unwrap()
            .shapes[0]
            .uid,
        a.shapes[0].uid
    );
    a.shapes[1].uid = a.shapes[0].uid;
    assert!(a.validate().unwrap_err().to_string().contains("identity"));
}

#[gpui::test]
fn schema_two_upgrade_preserves_node_identity_across_devices_and_save(
    cx: &mut gpui::TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("legacy.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    let mut page = Page::empty("Design".into());
    page.shapes.push(shape(1));
    page.next_id = 2;
    let document = Document::single(page);
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&writer.snapshot().read("document", 65536).unwrap()).unwrap();
    manifest["schema"] = 2.into();
    writer
        .put_bytes("document", "json", &serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    let key = format!("page/{}/shape/1", document.pages[0].id);
    let mut node = serde_json::to_value(&document.pages[0].shapes[0]).unwrap();
    node.as_object_mut().unwrap().remove("uid");
    writer
        .put_bytes(&key, "json", &serde_json::to_vec(&node).unwrap())
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    let first = crate::document::load(&path).unwrap();
    let second = crate::document::load(&path).unwrap();
    assert!(first.needs_upgrade);
    assert_eq!(first.json, second.json);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "Reading must not rewrite the cache"
    );
    crate::document::save(&path, &first.json, &[], &first.json, &text_system).unwrap();
    let upgraded = crate::document::load(&path).unwrap();
    assert!(!upgraded.needs_upgrade);
    assert_eq!(upgraded.json, first.json);
    // The current schema must fail closed if identity metadata is missing.
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    writer
        .put_bytes(&key, "json", &serde_json::to_vec(&node).unwrap())
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    assert!(crate::document::load(&path).is_err());
}
