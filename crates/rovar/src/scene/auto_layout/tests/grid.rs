use super::*;

fn grid_page() -> Page {
    let mut page = fixture();
    page.hierarchy.layouts.get_mut(&1).unwrap().axis = Axis::Grid;
    page.shapes.push(Shape::new(
        4,
        Some(1),
        ShapeKind::Rectangle,
        rect(0., 0., 50., 30.),
    ));
    page.next_id = 5;
    for id in 2..=4 {
        page.hierarchy.sizing.insert(
            id,
            Sizing {
                width: Mode::Fill,
                ..Default::default()
            },
        );
    }
    page
}

#[gpui::test]
fn grid_guides_follow_tracks_spans_and_hidden_items(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = grid_page();
    let grids = resolve(&mut page, &system).unwrap();
    assert_eq!(grids[&1].bounds, page.boards[0].rect);
    assert_eq!(grids[&1].columns, [[10., 146.], [154., 290.]]);
    assert_eq!(grids[&1].rows, [[10., 50.], [62., 92.]]);
    page.hierarchy.sizing.get_mut(&2).unwrap().row_span = 2;
    let grids = resolve(&mut page, &system).unwrap();
    assert_eq!(grids[&1].rows, [[10., 50.], [62., 92.]]);
    page.shapes[2].layer.hidden = true;
    page.hierarchy.sizing.get_mut(&2).unwrap().row_span = 1;
    let layout = page.hierarchy.layouts.get_mut(&1).unwrap();
    layout.columns = 3;
    layout.column_width = Some(50.);
    let grids = resolve(&mut page, &system).unwrap();
    assert_eq!(grids[&1].columns, [[10., 60.], [68., 118.], [126., 176.]]);
    assert_eq!(grids[&1].rows, [[10., 50.]]);
    page.boards[0].layer.hidden = true;
    assert!(resolve(&mut page, &system).unwrap().is_empty());
}

#[gpui::test]
fn grid_equal_columns_reflow_with_frame_and_spans(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = grid_page();
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 10., 136., 20.));
    assert_eq!(page.shapes[1].rect, rect(154., 10., 136., 40.));
    assert_eq!(page.shapes[2].rect, rect(10., 62., 136., 30.));
    page.boards[0].rect.width = 500.;
    page.hierarchy.sizing.get_mut(&2).unwrap().column_span = 2;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 10., 480., 20.));
    assert_eq!(page.shapes[1].rect, rect(10., 42., 236., 40.));
    assert_eq!(page.shapes[2].rect, rect(254., 42., 236., 30.));
    let sizing = page.hierarchy.sizing.get_mut(&2).unwrap();
    sizing.column_span = 1;
    sizing.row_span = 2;
    sizing.height = Mode::Fill;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[0].rect, rect(10., 10., 236., 82.));
    assert_eq!(page.shapes[1].rect, rect(254., 10., 236., 40.));
    assert_eq!(page.shapes[2].rect, rect(254., 62., 236., 30.));
    page.validate().unwrap();
}

#[gpui::test]
fn grid_fixed_columns_hug_hidden_and_absolute_items(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = grid_page();
    let layout = page.hierarchy.layouts.get_mut(&1).unwrap();
    layout.column_width = Some(80.);
    layout.columns = 3;
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            width: Mode::Hug,
            height: Mode::Hug,
            ..Default::default()
        },
    );
    page.shapes[1].layer.hidden = true;
    page.hierarchy.sizing.get_mut(&4).unwrap().absolute = true;
    page.shapes[2].rect = rect(260., 70., 50., 30.);
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.boards[0].rect.width, 276.);
    assert_eq!(page.boards[0].rect.height, 40.);
    assert_eq!(page.shapes[0].rect, rect(10., 10., 80., 20.));
    assert_eq!(page.shapes[2].rect, rect(260., 70., 50., 30.));
    page.shapes[1].layer.hidden = false;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.shapes[1].rect, rect(98., 10., 80., 40.));
    assert_eq!(page.boards[0].rect.height, 60.);
}

#[test]
fn grid_validation_rejects_invalid_tracks_and_spans() {
    let mut page = grid_page();
    page.hierarchy.layouts.get_mut(&1).unwrap().columns = 0;
    assert!(page.validate().is_err());
    page.hierarchy.layouts.get_mut(&1).unwrap().columns = 2;
    page.hierarchy.layouts.get_mut(&1).unwrap().column_width = Some(f32::NAN);
    assert!(page.validate().is_err());
    page.hierarchy.layouts.get_mut(&1).unwrap().column_width = Some(80.);
    page.hierarchy.sizing.get_mut(&2).unwrap().row_span = 0;
    assert!(page.validate().is_err());
    page.hierarchy.sizing.get_mut(&2).unwrap().row_span = 2;
    page.validate().unwrap();
}

#[gpui::test]
fn grid_document_roundtrip_and_sync_delta_preserve_layout(cx: &mut gpui::TestAppContext) {
    use crate::document::{self, Document};
    use rovar_format::delta::Snapshot;
    let system = cx.update(|cx| cx.text_system().clone());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("grid.rovar");
    let mut document = Document::single(grid_page());
    let original = serde_json::to_vec(&document).unwrap();
    document::save_as(&path, &original, &[], &system).unwrap();
    let before = document::load(&path).unwrap();
    let limit = rovar_api::MAX_METADATA_BYTES;
    let base = Snapshot::from_bytes(&std::fs::read(&path).unwrap(), limit).unwrap();
    let layout = document.pages[0].hierarchy.layouts.get_mut(&1).unwrap();
    layout.columns = 3;
    layout.column_width = Some(90.);
    document.pages[0]
        .hierarchy
        .sizing
        .get_mut(&2)
        .unwrap()
        .column_span = 2;
    let json = serde_json::to_vec(&document).unwrap();
    document::save(&path, &json, &[], &before.json, &system).unwrap();
    let next = Snapshot::from_bytes(&std::fs::read(&path).unwrap(), limit).unwrap();
    let patch = base.difference(&next).unwrap();
    let reconstructed = base.apply(&patch, limit).unwrap().to_bytes(limit).unwrap();
    let synced = temp.path().join("synced.rovar");
    std::fs::write(&synced, reconstructed).unwrap();
    let loaded = Document::decode(&document::load(&synced).unwrap().json).unwrap();
    assert!(loaded.pages[0].hierarchy == document.pages[0].hierarchy);
    let mut remote = Document::decode(&original).unwrap();
    remote.pages[0]
        .hierarchy
        .sizing
        .get_mut(&3)
        .unwrap()
        .row_span = 2;
    let merged =
        document::merge::merge(&original, &json, &serde_json::to_vec(&remote).unwrap()).unwrap();
    let merged = Document::decode(&merged).unwrap();
    assert_eq!(merged.pages[0].hierarchy.layouts[&1].columns, 3);
    assert_eq!(merged.pages[0].hierarchy.sizing[&2].column_span, 2);
    assert_eq!(merged.pages[0].hierarchy.sizing[&3].row_span, 2);
}

#[gpui::test]
fn grid_text_wraps_inside_equal_columns(cx: &mut gpui::TestAppContext) {
    let system = cx.update(|cx| gpui::WindowTextSystem::new(cx.text_system().clone()));
    let mut page = grid_page();
    page.shapes.retain(|s| s.id != 2);
    let content = "Responsive text should wrap within its grid column. ".repeat(3);
    let mut styles = StyledText::default();
    styles.replace(0..0, content.len());
    page.texts.push(crate::document::Text {
        uid: uuid::Uuid::new_v4(),
        id: 2,
        board: Some(1),
        rect: rect(0., 0., 100., 20.),
        layer: Default::default(),
        content,
        styles,
    });
    page.hierarchy.sizing.get_mut(&2).unwrap().height = Mode::Hug;
    page.hierarchy.sizing.insert(
        1,
        Sizing {
            height: Mode::Hug,
            ..Default::default()
        },
    );
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.texts[0].rect.width, 136.);
    let narrow = page.texts[0].rect.height;
    page.boards[0].rect.width = 600.;
    resolve(&mut page, &system).unwrap();
    assert_eq!(page.texts[0].rect.width, 286.);
    assert!(page.texts[0].rect.height < narrow);
    assert!(page.boards[0].rect.height >= page.texts[0].rect.height + 20.);
}
