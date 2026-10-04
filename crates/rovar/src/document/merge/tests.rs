use super::*;
use crate::scene::shape::{Shape, ShapeKind};

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

fn document() -> Document {
    let mut page = Page::empty("Design".into());
    page.shapes = vec![shape(1), shape(2)];
    page.next_id = 3;
    Document::single(page)
}

fn merged(base: &Document, local: &Document, remote: &Document) -> Result<Document> {
    let mut baseline = serde_json::to_value(base)?;
    // Confirmed semantic baselines deliberately omit allocation watermarks.
    for page in baseline["pages"].as_array_mut().unwrap() {
        page.as_object_mut().unwrap().remove("next_id");
    }
    Document::decode(&merge(
        &serde_json::to_vec(&baseline)?,
        &serde_json::to_vec(local)?,
        &serde_json::to_vec(remote)?,
    )?)
}

#[test]
fn disjoint_properties_merge_but_divergent_edits_and_delete_edit_do_not() {
    let base = document();
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages[0].shapes[0].rect.x = 20.;
    remote.pages[0].shapes[0].rect.y = 30.;
    remote.pages[0].name = "Renamed remotely".into();
    let output = merged(&base, &local, &remote).unwrap();
    assert_eq!(output.pages[0].shapes[0].rect.x, 20.);
    assert_eq!(output.pages[0].shapes[0].rect.y, 30.);
    assert_eq!(output.pages[0].name, "Renamed remotely");
    remote.pages[0].shapes[0].rect.x = 40.;
    assert!(merged(&base, &local, &remote).is_err());
    remote.pages[0].shapes.remove(0);
    assert!(merged(&base, &local, &remote).is_err());
}

#[test]
fn shadows_merge_with_geometry_and_follow_remapped_object_ids() {
    use crate::scene::effects::Shadow;
    let base = document();
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages[0]
        .hierarchy
        .shadows
        .insert(1, vec![Shadow::default()]);
    remote.pages[0].shapes[0].rect.x = 80.;
    let result = merged(&base, &local, &remote).unwrap();
    assert_eq!(
        result.pages[0].hierarchy.shadows[&1],
        vec![Shadow::default()]
    );
    assert_eq!(result.pages[0].shapes[0].rect.x, 80.);
    let a = shape(3);
    let b = shape(3);
    local.pages[0].shapes.push(a.clone());
    remote.pages[0].shapes.push(b.clone());
    local.pages[0].next_id = 4;
    remote.pages[0].next_id = 4;
    local.pages[0].hierarchy.shadows.insert(
        3,
        vec![Shadow {
            x: 24.,
            ..Default::default()
        }],
    );
    remote.pages[0].hierarchy.shadows.insert(
        3,
        vec![Shadow {
            x: -40.,
            ..Default::default()
        }],
    );
    let result = merged(&base, &local, &remote).unwrap();
    let page = &result.pages[0];
    let id = |uid| page.shapes.iter().find(|s| s.uid == uid).unwrap().id;
    assert_eq!(page.hierarchy.shadows[&id(a.uid)][0].x, 24.);
    assert_eq!(page.hierarchy.shadows[&id(b.uid)][0].x, -40.);
    let mut invalid = result.clone();
    invalid.pages[0].hierarchy.shadows.get_mut(&1).unwrap()[0].blur = -1.;
    assert!(invalid.pages[0].validate().is_err());
}

#[test]
fn offline_insertions_with_the_same_handle_keep_their_identity_and_references() {
    let base = document();
    let mut local = base.clone();
    let mut remote = base.clone();
    let a = shape(3);
    let b = shape(3);
    local.pages[0].shapes.push(a.clone());
    remote.pages[0].shapes.push(b.clone());
    local.pages[0].next_id = 4;
    remote.pages[0].next_id = 4;
    local.pages[0].hierarchy.names.insert(3, "Local".into());
    remote.pages[0].hierarchy.names.insert(3, "Remote".into());
    let output = merged(&base, &local, &remote).unwrap();
    let page = &output.pages[0];
    let a = page.shapes.iter().find(|s| s.uid == a.uid).unwrap();
    let b = page.shapes.iter().find(|s| s.uid == b.uid).unwrap();
    assert_eq!(a.id, 3);
    assert_ne!(a.id, b.id);
    assert_eq!(page.hierarchy.names[&a.id], "Local");
    assert_eq!(page.hierarchy.names[&b.id], "Remote");
    assert_eq!(page.hierarchy.order.len(), 4);
    // A different process can reindex every handle without creating an edit.
    let mut reindexed = base.clone();
    reindexed.pages[0].shapes[0].id = 100;
    reindexed.pages[0].next_id = 101;
    let mut edit = base.clone();
    edit.pages[0].shapes[0].rect.width = 80.;
    assert_eq!(
        merged(&base, &reindexed, &edit).unwrap().pages[0]
            .shapes
            .iter()
            .find(|s| s.uid == base.pages[0].shapes[0].uid)
            .unwrap()
            .rect
            .width,
        80.
    );
}

#[test]
fn concurrent_page_insertions_survive_and_invalid_combined_parents_are_rejected() {
    let base = document();
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages.push(Page::empty("Local page".into()));
    remote.pages.push(Page::empty("Remote page".into()));
    assert_eq!(merged(&base, &local, &remote).unwrap().pages.len(), 3);

    let mut base = document();
    base.pages[0].hierarchy.groups.insert(
        3,
        crate::scene::layer::LayerGroup {
            uid: uuid::Uuid::new_v4(),
            name: "Group".into(),
            board: None,
            layer: Default::default(),
        },
    );
    base.pages[0].next_id = 4;
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages[0].hierarchy.groups.remove(&3);
    remote.pages[0].hierarchy.parents.insert(1, 3);
    // Both inputs are valid; together they would point at a deleted group.
    local.validate().unwrap();
    remote.validate().unwrap();
    assert!(merged(&base, &local, &remote).is_err());
}

#[test]
fn incompatible_reordering_remains_a_conflict() {
    let mut base = document();
    base.pages[0].shapes.push(shape(3));
    base.pages[0].next_id = 4;
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages[0].hierarchy.order = vec![2, 1, 3];
    remote.pages[0].hierarchy.order = vec![1, 3, 2];
    assert!(merged(&base, &local, &remote).is_err());
}

#[test]
fn text_content_and_style_runs_are_one_edit_unit() {
    let mut base = document();
    base.pages[0].texts.push(Text {
        uid: uuid::Uuid::new_v4(),
        id: 3,
        board: None,
        rect: Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 50.,
        },
        layer: Default::default(),
        content: "hello".into(),
        styles: Default::default(),
    });
    base.pages[0].next_id = 4;
    base.pages[0].texts[0].styles.replace(0..0, 5);
    let mut local = base.clone();
    let mut remote = base.clone();
    local.pages[0].texts[0].content = "hi".into();
    local.pages[0].texts[0].styles.replace(0..5, 2);
    remote.pages[0].texts[0].styles.default.size = 32.;
    assert!(merged(&base, &local, &remote).is_err());
    remote.pages[0].texts[0].styles = base.pages[0].texts[0].styles.clone();
    remote.pages[0].texts[0].rect.x = 50.;
    let output = merged(&base, &local, &remote).unwrap();
    assert_eq!(output.pages[0].texts[0].content, "hi");
    assert_eq!(output.pages[0].texts[0].rect.x, 50.);
}

#[test]
fn identical_edits_do_not_rewrite_the_confirmed_representation() {
    let base = document();
    let mut local = base.clone();
    local.pages[0].shapes[0].rect.x = 10.;
    let output = merged(&base, &local, &local).unwrap();
    assert_eq!(
        serde_json::to_value(&output).unwrap(),
        serde_json::to_value(&local).unwrap()
    );
}
