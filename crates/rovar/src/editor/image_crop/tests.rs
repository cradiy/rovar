use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn crop_preview_keeps_document_unchanged_then_commits_once_and_undoes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crop.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([30, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let asset = crate::media::MediaAsset::load_image(&path).unwrap();
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 2.;
            let mut shape = Shape::new(
                1,
                None,
                ShapeKind::Image,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 100.,
                    height: 100.,
                },
            );
            shape.media = Some(asset);
            shape.layer.rotation = 90.;
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "image-crop-start");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.zoom_image_crop(2., None, cx)
        })
        .unwrap();
    draw(&mut visual);
    let start = window
        .update(&mut visual.cx, |this, _, _| {
            this.bounds.get().origin + this.view.screen(point(150., 150.)).map(px)
        })
        .unwrap();
    let end = start + point(px(0.), px(40.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].media_placement, Placement::default());
            assert_eq!(
                this.snapshot_page(cx).0.shapes[0].media_placement,
                Placement::default()
            );
            assert_eq!(this.history.borrow().undo_len(), 0);
            let crop = this.image_crop.as_ref().unwrap();
            assert!((crop.image.placement.offset[0] - 0.2).abs() < 0.001);
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.image_crop.is_none());
            assert_eq!(this.shapes[0].rect.width, 100.);
            assert_eq!(this.shapes[0].media_placement.zoom, 2.);
            assert_eq!(this.history.borrow().undo_len(), 1);
            let page = this.snapshot_page(cx).0;
            let encoded = serde_json::to_vec(&crate::document::Document::single(page)).unwrap();
            let loaded = crate::document::Document::decode(&encoded).unwrap();
            assert_eq!(
                loaded.pages[0].shapes[0].media_placement,
                this.shapes[0].media_placement
            );
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].media_placement, Placement::default())
        })
        .unwrap();
    click(&mut visual, "image-crop-start");
    click(&mut visual, "image-crop-in");
    visual.simulate_keystrokes("escape");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.image_crop.is_none());
            assert_eq!(this.shapes[0].media_placement, Placement::default());
            assert!(this.history.borrow().can_redo());
        })
        .unwrap();
}

#[gpui::test]
fn double_click_image_fill_enters_crop_and_locked_layers_cannot_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fill.png");
    image::RgbaImage::from_pixel(100, 50, image::Rgba([50, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let asset = crate::media::MediaAsset::load_image(&path).unwrap();
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 1.;
            let mut shape = Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 120.,
                    height: 120.,
                },
            );
            shape.fill_mode = FillMode::Image;
            shape.image_fill.asset = Some(asset);
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = visual.debug_bounds("shape-1").unwrap().center();
    visual.simulate_event(gpui::MouseDownEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 2,
        modifiers: Default::default(),
        first_mouse: false,
    });
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("image-crop-surface").is_some());
    click(&mut visual, "image-crop-in");
    click(&mut visual, "image-crop-done");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.shapes[0].image_fill.placement.zoom > 1.);
            assert_eq!(this.history.borrow().undo_len(), 1);
            assert!(this.vector_edit.is_none());
            this.shapes[0].layer.locked = true;
            assert!(!this.start_image_crop(1, window, cx));
        })
        .unwrap();
}
