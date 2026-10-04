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
    let mut definition_page = page.clone();
    definition_page.id = Uuid::new_v4().to_string();
    let component = Uuid::new_v4().to_string();
    page.hierarchy.components.insert(
        1,
        crate::scene::components::Binding {
            component: component.clone(),
            master: false,
            nodes: BTreeMap::from([(1, 1)]),
            baseline: serde_json::to_value(&definition_page).unwrap(),
        },
    );
    let mut document = Document::single(page);
    document.components.insert(
        component.clone(),
        crate::scene::components::Definition {
            name: "Component".into(),
            source: None,
            root: 1,
            page: definition_page,
        },
    );
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
    let hierarchy_key = format!("page/{}/hierarchy", document.pages[0].id);
    let mut hierarchy = serde_json::to_value(&document.pages[0].hierarchy).unwrap();
    hierarchy["components"]["1"]["baseline"]["shapes"][0]
        .as_object_mut()
        .unwrap()
        .remove("uid");
    writer
        .put_bytes(
            &hierarchy_key,
            "json",
            &serde_json::to_vec(&hierarchy).unwrap(),
        )
        .unwrap();
    let mut definition = serde_json::to_value(&document.components[&component]).unwrap();
    definition["page"]["shapes"][0]
        .as_object_mut()
        .unwrap()
        .remove("uid");
    writer
        .put_bytes(
            &format!("component/{component}"),
            "json",
            &serde_json::to_vec(&definition).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    let first = crate::document::load(&path).unwrap();
    let second = crate::document::load(&path).unwrap();
    assert!(first.needs_upgrade);
    assert_eq!(first.json, second.json);
    let migrated = Document::decode(&first.json).unwrap();
    let baseline: Page =
        serde_json::from_value(migrated.pages[0].hierarchy.components[&1].baseline.clone())
            .unwrap();
    assert_eq!(
        baseline.node_ids(),
        migrated.components[&component].page.node_ids()
    );
    assert!(!baseline.shapes[0].uid.is_nil());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "Reading must not rewrite the cache"
    );
    crate::document::save(&path, &first.json, &[], &first.json, &text_system).unwrap();
    let upgraded = crate::document::load(&path).unwrap();
    assert!(!upgraded.needs_upgrade);
    assert_eq!(upgraded.json, first.json);
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    writer
        .put_bytes(
            &hierarchy_key,
            "json",
            &serde_json::to_vec(&hierarchy).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    assert!(crate::document::load(&path).is_err());
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    writer
        .put_bytes(
            &hierarchy_key,
            "json",
            &serde_json::to_vec(&migrated.pages[0].hierarchy).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    // The current schema must fail closed if identity metadata is missing.
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    writer
        .put_bytes(&key, "json", &serde_json::to_vec(&node).unwrap())
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    assert!(crate::document::load(&path).is_err());
}

#[test]
fn component_baseline_upgrade_preserves_existing_ids_and_rejects_invalid_ids() {
    let mut baseline = Page::empty("Component".into());
    baseline.shapes = vec![shape(1), shape(2)];
    baseline.next_id = 3;
    let existing = baseline.shapes[1].uid;
    baseline.shapes[0].uid = Uuid::nil();
    let mut page = baseline.clone();
    page.id = Uuid::new_v4().to_string();
    page.hierarchy.components.insert(
        1,
        crate::scene::components::Binding {
            component: Uuid::new_v4().to_string(),
            master: false,
            nodes: BTreeMap::from([(1, 1), (2, 2)]),
            baseline: serde_json::to_value(&baseline).unwrap(),
        },
    );
    let migrated = Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
    let migrated_baseline: Page =
        serde_json::from_value(migrated.hierarchy.components[&1].baseline.clone()).unwrap();
    baseline.upgrade_node_ids();
    assert_eq!(migrated_baseline.node_ids(), baseline.node_ids());
    assert_eq!(migrated_baseline.shapes[1].uid, existing);
    assert_ne!(migrated_baseline.shapes[0].uid, migrated.shapes[0].uid);
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!("invalid"),
        serde_json::json!(existing),
    ] {
        page.hierarchy.components.get_mut(&1).unwrap().baseline["shapes"][0]["uid"] = invalid;
        assert!(Page::decode(&serde_json::to_vec(&page).unwrap()).is_err());
    }
}

#[gpui::test]
fn published_shadow_lists_load_and_save_as_effects_without_losing_snapshots(
    cx: &mut gpui::TestAppContext,
) {
    use crate::scene::effects::{Effect, Shadow};
    let mut page = Page::empty("Design".into());
    page.shapes.push(shape(1));
    page.next_id = 2;
    page.hierarchy.effects.insert(1, vec![Effect::default()]);
    let mut snapshot = serde_json::to_value(&page).unwrap();
    let hierarchy = snapshot["hierarchy"].as_object_mut().unwrap();
    let mut shadows = hierarchy.remove("effects").unwrap();
    let shadow = shadows["1"][0].as_object_mut().unwrap();
    shadow.remove("kind");
    shadow.remove("type");
    shadow.insert("x".into(), 17.into());
    hierarchy.insert("shadows".into(), shadows);
    let expected = vec![Effect::Shadow(Shadow {
        x: 17.,
        ..Default::default()
    })];
    let migrated = Page::decode(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(migrated.hierarchy.effects[&1], expected);
    let component = Uuid::new_v4().to_string();
    page.hierarchy.components.insert(
        1,
        crate::scene::components::Binding {
            component: component.clone(),
            master: false,
            nodes: BTreeMap::from([(1, 1)]),
            baseline: snapshot.clone(),
        },
    );
    let mut document = Document::single(page);
    document.components.insert(
        component,
        crate::scene::components::Definition {
            name: "Component".into(),
            source: None,
            root: 1,
            page: migrated,
        },
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("published.rovar");
    let text_system = cx.update(|cx| cx.text_system().clone());
    crate::document::save_as(
        &path,
        &serde_json::to_vec(&document).unwrap(),
        &[],
        &text_system,
    )
    .unwrap();
    let mut writer = rovar_format::Writer::open(&path).unwrap();
    let mut legacy = snapshot["hierarchy"].clone();
    legacy["components"] = serde_json::to_value(&document.pages[0].hierarchy.components).unwrap();
    writer
        .put_bytes(
            &format!("page/{}/hierarchy", document.pages[0].id),
            "json",
            &serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    let loaded = crate::document::load(&path).unwrap();
    let migrated = Document::decode(&loaded.json).unwrap();
    assert_eq!(migrated.pages[0].hierarchy.effects[&1], expected);
    let baseline: Page =
        serde_json::from_value(migrated.pages[0].hierarchy.components[&1].baseline.clone())
            .unwrap();
    assert_eq!(baseline.hierarchy.effects[&1], expected);
    assert!(
        migrated.pages[0].hierarchy.components[&1].baseline["hierarchy"]
            .get("shadows")
            .is_none()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    crate::document::save(
        &path,
        &serde_json::to_vec(&migrated).unwrap(),
        &[],
        &loaded.json,
        &text_system,
    )
    .unwrap();
    let saved = crate::document::load(&path)
        .unwrap()
        .into_document()
        .unwrap();
    assert_eq!(saved.pages[0].hierarchy.effects[&1], expected);
}
