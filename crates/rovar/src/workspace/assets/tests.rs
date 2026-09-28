use super::*;
use crate::{artboard::Artboard, layer::LayerGroup};
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

#[gpui::test]
fn saved_group_embeds_media_and_inserts_independent_objects_with_one_undo(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    cx.run_until_parked();
    let original = root.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([90, 45, 190, 255]))
        .save(&original)
        .unwrap();
    let asset = crate::media::MediaAsset::load(original.clone()).unwrap();
    let source = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Workspace::new(window, cx)
    });
    source
        .update(cx, |editor, window, cx| {
            editor.attach_library(library.clone(), cx);
            editor.boards.push(board(
                1,
                Rect {
                    x: 100.,
                    y: 200.,
                    width: 600.,
                    height: 500.,
                },
            ));
            let mut image = Shape::new(
                2,
                Some(1),
                ShapeKind::Image,
                Rect {
                    x: 30.,
                    y: 40.,
                    width: 80.,
                    height: 60.,
                },
            );
            image.media = Some(asset.clone());
            image.layer.rotation = 17.;
            editor.shapes.push(image);
            let text = editor.make_text(
                3,
                Some(1),
                Rect {
                    x: 120.,
                    y: 50.,
                    width: 130.,
                    height: 40.,
                },
                window,
                cx,
            );
            text.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "Reusable 组件", window, cx)
            });
            editor.texts.push(text);
            editor.hierarchy.groups.insert(
                4,
                LayerGroup {
                    name: "Card".into(),
                    board: Some(1),
                    layer: Default::default(),
                },
            );
            editor.hierarchy.parents.extend([(2, 4), (3, 4)]);
            editor.hierarchy.order = vec![1, 4, 3, 2];
            editor.next_id = 5;
            editor.set_selection(BTreeSet::from([4]), cx);
            editor.begin_save_asset(window, cx);
            assert!(matches!(editor.assets.dialog, Some(Dialog::Save(_))));
            editor
                .assets
                .name
                .update(cx, |input, cx| input.set_value("Photo card", cx));
            editor.confirm_asset_dialog(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let entry = library.read_with(cx, |lib, _| {
        assert!(lib.error.is_none(), "{:?}", lib.error);
        lib.entries[0].clone()
    });
    source
        .update(cx, |editor, _, _| {
            editor.shapes.clear();
            editor.texts.clear();
        })
        .unwrap();
    std::fs::remove_file(original).unwrap();
    drop(asset);
    let destination = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Workspace::new(window, cx)
    });
    destination
        .update(cx, |editor, window, cx| {
            editor.attach_library(library.clone(), cx);
            editor.boards.push(board(
                1,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 1000.,
                    height: 1000.,
                },
            ));
            editor.next_id = 10;
            editor.insert_asset(entry.clone(), Some(point(400., 350.)), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    destination
        .update(cx, |editor, window, cx| {
            assert!(editor.assets.error.is_none(), "{:?}", editor.assets.error);
            assert_eq!(editor.shapes.len(), 1);
            assert_eq!(editor.texts.len(), 1);
            assert_eq!(editor.boards.len(), 1);
            let selected = *editor.selection_ids().first().unwrap();
            assert!(selected >= 10);
            assert_eq!(editor.layer_name(selected, cx), "Photo card");
            let bounds = editor.world_bounds(selected).unwrap();
            assert!((bounds.x + bounds.width / 2. - 400.).abs() < 0.01);
            assert!((bounds.y + bounds.height / 2. - 350.).abs() < 0.01);
            assert_eq!(editor.shapes[0].layer.rotation, 17.);
            assert_eq!(editor.shapes[0].board, Some(1));
            assert!(
                editor.shapes[0]
                    .media
                    .as_ref()
                    .unwrap()
                    .source
                    .open()
                    .is_ok()
            );
            assert_eq!(editor.texts[0].editor.read(cx).content, "Reusable 组件");
            let order = editor.hierarchy.order.clone();
            assert!(
                order
                    .iter()
                    .position(|id| *id == editor.texts[0].id)
                    .unwrap()
                    < order
                        .iter()
                        .position(|id| *id == editor.shapes[0].id)
                        .unwrap()
            );
            editor.replay_history(false, window, cx);
            assert!(
                editor.shapes.is_empty()
                    && editor.texts.is_empty()
                    && editor.hierarchy.groups.is_empty()
            );
            editor.replay_history(true, window, cx);
            assert_eq!(editor.shapes.len(), 1);
            editor.insert_asset(entry.clone(), Some(point(600., 500.)), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    library.update(cx, |lib, cx| lib.delete(entry.clone(), cx));
    cx.run_until_parked();
    destination
        .update(cx, |editor, _, cx| {
            assert_eq!(editor.shapes.len(), 2);
            assert_ne!(editor.shapes[0].id, editor.shapes[1].id);
            editor.shapes[0].color = rgb(0xff0000);
            assert_ne!(editor.shapes[0].color, editor.shapes[1].color);
            let (json, sources) = editor
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            crate::document::save_as(
                &root.path().join("copy.rovar"),
                &json,
                &sources,
                cx.text_system(),
            )
            .unwrap();
        })
        .unwrap();
    assert_eq!(
        crate::document::load(&root.path().join("copy.rovar"))
            .unwrap()
            .into_document()
            .unwrap()
            .shapes
            .len(),
        2
    );
}

#[gpui::test]
fn assets_panel_search_save_cancel_and_drop_respect_the_canvas(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    cx.run_until_parked();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Workspace::new(window, cx)
    });
    window
        .update(cx, |editor, _, cx| {
            editor.attach_library(library.clone(), cx);
            editor.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 100.,
                    height: 80.,
                },
            ));
            editor.shapes.push(Shape::new(
                2,
                None,
                ShapeKind::Ellipse,
                Rect {
                    x: 130.,
                    y: 20.,
                    width: 50.,
                    height: 50.,
                },
            ));
            editor.next_id = 3;
            editor.set_selection(BTreeSet::from([1, 2]), cx);
            editor.sidebar.resources = true;
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    crate::workspace::tests::draw(&mut visual);
    crate::workspace::tests::click(&mut visual, "save-selection-to-assets");
    assert!(visual.debug_bounds("asset-dialog").is_some());
    visual.simulate_keystrokes("escape");
    crate::workspace::tests::draw(&mut visual);
    assert!(visual.debug_bounds("asset-dialog").is_none());
    assert!(library.read_with(&visual.cx, |lib, _| lib.entries.is_empty()));
    window
        .update(&mut visual.cx, |editor, window, cx| {
            editor.begin_save_asset(window, cx);
            editor
                .assets
                .name
                .update(cx, |input, cx| input.set_value("Two shapes", cx));
            editor.confirm_asset_dialog(window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    let entry = library.read_with(&visual.cx, |lib, _| lib.entries[0].clone());
    crate::workspace::tests::draw(&mut visual);
    let document = crate::document::load(&entry.path)
        .unwrap()
        .into_document()
        .unwrap();
    assert_eq!(document.hierarchy.groups.len(), 1);
    assert_eq!(document.hierarchy.parents.len(), 2);
    assert_eq!(entry.size, [180., 80.]);
    window
        .update(&mut visual.cx, |editor, window, cx| {
            editor.edit_asset(entry.clone(), true, window, cx);
            editor.cancel_asset_dialog(window, cx);
        })
        .unwrap();
    assert!(entry.path.exists());
    let selector: &'static str = Box::leak(format!("asset-card-{}", entry.id).into_boxed_str());
    window
        .update(&mut visual.cx, |editor, _, cx| {
            editor
                .assets
                .search
                .update(cx, |input, cx| input.set_value("no match", cx));
        })
        .unwrap();
    crate::workspace::tests::draw(&mut visual);
    assert!(visual.debug_bounds(selector).is_none());
    window
        .update(&mut visual.cx, |editor, _, cx| {
            editor
                .assets
                .search
                .update(cx, |input, cx| input.set_value("TWO", cx));
        })
        .unwrap();
    crate::workspace::tests::draw(&mut visual);
    let card = visual.debug_bounds(selector).unwrap();
    assert!(card.size.width > px(60.));
    let drag = AssetDrag {
        entry: entry.clone(),
        library: library.clone(),
    };
    visual.simulate_mouse_move(card.center(), None, Default::default());
    window
        .update(&mut visual.cx, |editor, window, cx| {
            editor.drop_asset(&drag, window, cx)
        })
        .unwrap();
    visual.cx.run_until_parked();
    window
        .update(&mut visual.cx, |editor, _, _| {
            assert_eq!(editor.shapes.len(), 2)
        })
        .unwrap();
    // Exercise the actual GPUI drag payload and the canvas drop handler.
    visual.simulate_mouse_down(card.center(), MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        card.center() + point(px(12.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(px(600.), px(340.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        point(px(600.), px(340.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.cx.run_until_parked();
    window
        .update(&mut visual.cx, |editor, window, cx| {
            assert_eq!(editor.shapes.len(), 4);
            let bounds = editor
                .world_bounds(*editor.selection_ids().first().unwrap())
                .unwrap();
            assert!((bounds.x + bounds.width / 2. - 600.).abs() < 0.01);
            assert!((bounds.y + bounds.height / 2. - 340.).abs() < 0.01);
            // Leaving a tab cancels a pending insert instead of stealing focus later.
            editor.insert_asset(entry.clone(), None, window, cx);
            editor.suspend(window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    window
        .update(&mut visual.cx, |editor, _, _| {
            assert_eq!(editor.shapes.len(), 4)
        })
        .unwrap();
}

fn board(id: usize, rect: Rect) -> Artboard {
    Artboard {
        id,
        rect,
        name: "Frame".into(),
        color: rgb(0xffffff),
        fill_mode: crate::artboard::FillMode::Solid,
        gradient: Default::default(),
        image_fill: Default::default(),
        layer: Default::default(),
    }
}

#[gpui::test]
fn failed_asset_card_can_be_removed_without_a_dialog(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("components");
    std::fs::create_dir_all(&directory).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(directory.join(format!("{id}.rovar")), b"unsupported").unwrap();
    let library = cx.update(|cx| Library::open(root.path(), cx));
    cx.run_until_parked();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        let mut editor = Workspace::new(window, cx);
        editor.attach_library(library.clone(), cx);
        editor.sidebar.resources = true;
        editor.sidebar.collapsed = false;
        editor
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    crate::workspace::tests::draw(&mut visual);
    assert!(visual.debug_bounds("asset-dialog").is_none());
    let selector: &'static str = Box::leak(format!("remove-failed-asset-{id}").into_boxed_str());
    let button = visual.debug_bounds(selector).unwrap();
    let panel = visual.debug_bounds("layers-panel").unwrap();
    assert!(panel.contains(&button.center()));
    let remove = button.center();
    visual.simulate_click(remove, Default::default());
    crate::workspace::tests::draw(&mut visual);
    assert!(
        library.read_with(&visual.cx, |lib, _| lib.entries.is_empty()),
        "{:?}",
        library.read_with(&visual.cx, |lib, _| (lib.busy, lib.error.clone()))
    );
    assert!(visual.debug_bounds(selector).is_none());
    assert!(visual.debug_bounds("asset-dialog").is_none());
    assert!(
        library.read_with(&visual.cx, |lib, _| lib.entries.is_empty()
            && lib.error.is_none())
    );
}
