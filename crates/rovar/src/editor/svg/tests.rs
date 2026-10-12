use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn svg_import_creates_editable_group_preserves_transforms_and_undo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("icon.svg");
    std::fs::write(&path, r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><g id="translated" transform="translate(10 20)" opacity="0.5"><rect id="box" width="20" height="30" fill="#ff0000"/><path id="curve" d="M 25 10 Q 35 0 45 10" fill="none" stroke="#00f" stroke-width="2" stroke-linecap="round"/></g></svg>"##).unwrap();
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, window, cx| {
            w.choose_tool(toolbar::Tool::Svg, window, cx)
        })
        .unwrap();
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |w, _, _| {
            assert!(w.svg.pending.is_some());
            assert!(w.shapes.is_empty());
        })
        .unwrap();
    click(&mut visual, "svg-import-material");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert!(w.svg.pending.is_none());
            assert_eq!(w.shapes.len(), 1);
            assert_eq!(w.shapes[0].kind, ShapeKind::Svg);
            w.undo_redo(false, window, cx);
            assert!(w.shapes.is_empty());
            w.choose_tool(toolbar::Tool::Svg, window, cx);
        })
        .unwrap();
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
    draw(&mut visual);
    click(&mut visual, "svg-import-convert");
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert!(w.svg.error.is_none(), "{:?}", w.svg.error);
            assert!(w.svg.pending.is_none());
            assert_eq!(w.shapes.len(), 2);
            assert!(w.shapes.iter().all(|s| s.kind == ShapeKind::Bezier
                && !s.nodes.0.is_empty()
                && s.asset.is_none()));
            let root = *w.selection_ids().first().unwrap();
            assert_eq!(w.hierarchy.groups[&root].name, "icon.svg");
            assert!(
                w.hierarchy
                    .groups
                    .values()
                    .any(|g| g.name == "translated" && g.layer.opacity == 0.5)
            );
            let shape = w.shapes.iter().find(|s| s.name == "box").unwrap().clone();
            assert_eq!((shape.rect.width, shape.rect.height), (20., 30.));
            assert_eq!(shape.color, gpui::rgb(0xff0000));
            let saved = serde_json::to_value(w.snapshot_page(cx).0).unwrap();
            let page = crate::document::Page::decode(&serde_json::to_vec(&saved).unwrap()).unwrap();
            assert!(page.assets.is_empty());
            let job = crate::render::export::Job {
                preset: Default::default(),
                original: None,
                name: "Editable SVG".into(),
                json: Arc::new(serde_json::to_vec(&page).unwrap()),
                assets: Arc::new(vec![]),
                order: w.canvas_layer_order(),
                bounds: Rect {
                    x: shape.rect.x - 10.,
                    y: shape.rect.y - 20.,
                    width: 100.,
                    height: 100.,
                },
                clips: Default::default(),
                text: Default::default(),
            };
            let actual = image::load_from_memory(
                &job.render(crate::document::export::Format::Png, 1, &Default::default())
                    .unwrap(),
            )
            .unwrap()
            .into_rgba8();
            let tree =
                resvg::usvg::Tree::from_data(&std::fs::read(&path).unwrap(), &Default::default())
                    .unwrap();
            let mut expected = resvg::tiny_skia::Pixmap::new(100, 100).unwrap();
            resvg::render(&tree, Default::default(), &mut expected.as_mut());
            let expected = image::load_from_memory(&expected.encode_png().unwrap())
                .unwrap()
                .into_rgba8();
            let error: u64 = actual
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .map(|(a, b)| a.abs_diff(*b) as u64)
                .sum();
            assert!(error < 2000, "SVG conversion raster difference: {error}");
            w.undo_redo(false, window, cx);
            assert!(w.shapes.is_empty());
            assert!(w.hierarchy.groups.is_empty());
            w.undo_redo(true, window, cx);
            assert_eq!(serde_json::to_value(w.snapshot_page(cx).0).unwrap(), saved);
            w.set_selection(std::collections::BTreeSet::from([shape.id]), cx);
            w.enter_vector_edit(shape.id, window, cx);
            assert!(w.vector_edit.is_some());
        })
        .unwrap();
}

#[gpui::test]
fn svg_automatically_keeps_vector_source_after_original_is_removed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gradient.svg");
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><defs><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><rect width="100" height="100" fill="url(#g)"/></svg>"##;
    std::fs::write(&path, source).unwrap();
    let asset = FileAsset::load(path.clone()).unwrap();
    let handle = open(cx);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let rect = Rect {
        x: 0.,
        y: 0.,
        width: 100.,
        height: 100.,
    };
    handle
        .update(&mut visual.cx, |w, window, cx| {
            w.import_svg(
                Pending {
                    asset: asset.clone(),
                    page: w.pages.active.clone(),
                    rect,
                },
                window,
                cx,
            );
            assert!(w.svg.error.is_none());
            assert!(w.svg.pending.is_none());
            assert_eq!(w.shapes.len(), 1);
        })
        .unwrap();
    std::fs::remove_file(path).unwrap();
    handle
        .update(&mut visual.cx, |w, window, cx| {
            assert_eq!(w.shapes[0].kind, ShapeKind::Svg);
            let (page, assets) = w.snapshot_page(cx);
            page.validate().unwrap();
            assert_eq!(assets[0].path.bytes(), source.as_bytes());
            let file = dir.path().join("saved.rovar");
            crate::document::save_as(
                &file,
                &serde_json::to_vec(&crate::document::Document::single(page.clone())).unwrap(),
                &assets,
                window.text_system(),
            )
            .unwrap();
            let restored = crate::document::load(&file)
                .unwrap()
                .into_document()
                .unwrap();
            let deferred = restored.pages[0].shapes[0].asset.as_ref().unwrap();
            assert_eq!(deferred.source.bytes(), source.as_bytes());
            deferred.ensure_decoded().unwrap();
            assert!(matches!(deferred.content(), Some(AssetContent::Svg(_))));
            let job = crate::render::export::Job {
                preset: Default::default(),
                original: None,
                name: "SVG".into(),
                json: Arc::new(serde_json::to_vec(&page).unwrap()),
                assets: Arc::new(assets),
                order: w.canvas_layer_order(),
                bounds: rect,
                clips: Default::default(),
                text: Default::default(),
            };
            let exported = String::from_utf8(
                job.render(crate::document::export::Format::Svg, 1, &Default::default())
                    .unwrap(),
            )
            .unwrap();
            assert!(exported.contains("data:image/svg+xml;base64,"));
            assert!(!exported.contains("data:image/png"));
            let pixels = image::load_from_memory(
                &job.render(crate::document::export::Format::Png, 2, &Default::default())
                    .unwrap(),
            )
            .unwrap()
            .into_rgba8();
            assert!(pixels.get_pixel(10, 100)[0] > 230);
            assert!(pixels.get_pixel(190, 100)[2] > 230);
            w.undo_redo(false, window, cx);
            assert!(w.shapes.is_empty());
            w.undo_redo(true, window, cx);
            assert_eq!(w.shapes[0].kind, ShapeKind::Svg);
        })
        .unwrap();
}
