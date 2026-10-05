use super::*;
use crate::{
    document::{Page, export::Format},
    editor::tests::{click, draw, open},
};
use gpui::EntityInputHandler;
use gpui::{TestAppContext, VisualTestContext};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn setup(this: &mut Workspace, cx: &mut Context<Workspace>) {
    this.shapes = vec![
        Shape::new(1, None, ShapeKind::Ellipse, rect(0., 0., 100., 100.)),
        Shape::new(2, None, ShapeKind::Rectangle, rect(-50., -50., 200., 200.)),
    ];
    this.shapes[0].color = gpui::rgba(0xff000000);
    this.shapes[1].color = gpui::rgb(0x0000ff);
    this.next_id = 3;
    this.set_selection(BTreeSet::from([1, 2]), cx);
}

#[gpui::test]
fn clipped_content_handles_remain_interactive_outside_the_mask(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.view.zoom = 1.;
            this.view.pan = point(350., 250.);
            this.create_mask(window, cx);
            assert!(!this.mask_hit(2, point(0., 0.)));
            assert!(this.mask_hit(2, point(50., 50.)));
            this.set_selection(BTreeSet::from([2]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let corner = visual.debug_bounds("shape-corner-4").unwrap().center();
    visual.simulate_mouse_down(corner, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        corner + point(px(20.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        corner + point(px(20.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.shapes[1].rect.width > 200.);
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[1].rect, rect(-50., -50., 200., 200.));
        })
        .unwrap();
}

#[gpui::test]
fn image_and_text_content_export_through_a_vector_mask(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    image::RgbaImage::from_pixel(10, 10, image::Rgba([30, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.shapes.truncate(1);
            this.next_id = 2;
            this.import_media(window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.shapes[1].kind, ShapeKind::Image);
            this.shapes[1].rect = rect(-50., -50., 200., 200.);
            this.add_text(None, rect(10., 10., 80., 50.), window, cx);
            let text = this.texts.last().unwrap();
            text.editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "Mask", window, cx)
            });
            this.set_selection(BTreeSet::from([1, 2, text.id]), cx);
            this.create_mask(window, cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let options = crate::render::svg::render_options(true).unwrap();
            let svg = String::from_utf8(jobs[0].render(Format::Svg, 1, &options).unwrap()).unwrap();
            assert!(svg.contains("<image"));
            assert!(svg.contains("url(#text-clip-3)"));
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.get_pixel(0, 0).0[3], 0);
            assert_eq!(image.get_pixel(50, 80).0, [30, 100, 190, 255]);
            assert!(
                (25..41).any(|y| (15..70).any(|x| image.get_pixel(x, y).0 != [30, 100, 190, 255]))
            );
        })
        .unwrap();
}

#[gpui::test]
fn mask_controls_preserve_sources_history_and_duplicate_references(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |this, _, cx| {
            setup(this, cx);
            this.sidebar.collapsed = false;
            this.view.zoom = 1.;
            this.view.pan = point(300., 250.);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    for width in [256., 480.] {
        handle
            .update(&mut visual.cx, |this, _, cx| {
                this.panels.set(super::super::panels::Side::Right, width);
                cx.notify();
            })
            .unwrap();
        draw(&mut visual);
        let row = visual.debug_bounds("composition-controls").unwrap();
        let boolean = visual.debug_bounds("boolean-menu").unwrap();
        let mask = visual.debug_bounds("mask-action").unwrap();
        assert_eq!(boolean.center().y, mask.center().y);
        assert!(boolean.right() < mask.left());
        assert!(mask.right() <= row.right());
        assert!(row.size.height <= px(54.));
    }
    click(&mut visual, "mask-action");
    assert!(visual.debug_bounds("layer-mask-source-1").is_some());
    assert!(visual.debug_bounds("layer-mask-source-2").is_none());
    assert!(visual.debug_bounds("layer-mask-source-3").is_none());
    handle
        .update(&mut visual.cx, |this, _, cx| {
            this.set_sibling_order(&[2, 1]);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-mask-source-1").is_some());
    assert!(visual.debug_bounds("layer-mask-source-2").is_none());
    let copied_source = handle
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.selected_mask(), Some(3));
            assert!(!this.can_boolean());
            assert!(!this.can_auto_layout());
            let original = this.shapes.clone();
            this.undo_redo(false, window, cx);
            assert!(this.hierarchy.groups.is_empty());
            this.undo_redo(true, window, cx);
            assert_eq!(this.hierarchy.groups[&3].mask, Some(1));
            assert_eq!(this.shapes, original);
            let page = this.snapshot_page(cx).0;
            let loaded = Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(loaded.hierarchy.groups[&3].mask, Some(1));
            this.duplicate_selection(window, cx);
            let root = this.selected_mask().unwrap();
            let source = this.hierarchy.groups[&root].mask.unwrap();
            assert_ne!(source, 1);
            assert_eq!(this.hierarchy.parents[&source], root);
            this.snapshot_page(cx).0.validate().unwrap();
            source
        })
        .unwrap();
    draw(&mut visual);
    let copied_badge: &'static str = format!("layer-mask-source-{copied_source}").leak();
    assert!(visual.debug_bounds(copied_badge).is_some());
    click(&mut visual, "mask-action");
    assert!(visual.debug_bounds(copied_badge).is_none());
    handle
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.selected_mask().is_none());
            assert_eq!(this.selection_ids().len(), 2);
            this.undo_redo(false, window, cx);
            assert!(this.selected_mask().is_some());
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds(copied_badge).is_some());
}

#[gpui::test]
fn vector_mask_exports_clip_content_and_preserve_individual_exports(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.create_mask(window, cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let options = crate::render::svg::render_options(false).unwrap();
            let svg = String::from_utf8(jobs[0].render(Format::Svg, 1, &options).unwrap()).unwrap();
            assert!(svg.contains("clip-path=\"url(#vector-mask-3)\""));
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (100, 100));
            assert_eq!(image.get_pixel(50, 50).0, [0, 0, 255, 255]);
            assert_eq!(image.get_pixel(0, 0).0[3], 0);
            this.set_selection(BTreeSet::from([2]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (200, 200));
            assert_eq!(image.get_pixel(0, 0).0, [0, 0, 255, 255]);
            this.set_selection(BTreeSet::from([1]), cx);
            this.delete_selected(cx);
            assert!(this.hierarchy.groups[&3].mask.is_none());
            this.snapshot_page(cx).0.validate().unwrap();
            this.undo_redo(false, window, cx);
            assert_eq!(this.hierarchy.groups[&3].mask, Some(1));
        })
        .unwrap();
}

#[gpui::test]
fn boolean_mask_holes_nested_masks_and_component_instances(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            setup(this, cx);
            this.shapes[0].kind = ShapeKind::Rectangle;
            this.shapes[1].rect = rect(25., 25., 50., 50.);
            this.apply_boolean(boolean::Operation::Subtract, window, cx);
            this.shapes.push(Shape::new(
                4,
                None,
                ShapeKind::Rectangle,
                rect(-20., -20., 140., 140.),
            ));
            this.shapes[2].color = gpui::rgb(0x00ff00);
            this.next_id = 5;
            this.set_selection(BTreeSet::from([3, 4]), cx);
            this.create_mask(window, cx);
            assert_eq!(this.selected_mask(), Some(5));
            let options = crate::render::svg::render_options(false).unwrap();
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.get_pixel(10, 10).0, [0, 255, 0, 255]);
            assert_eq!(image.get_pixel(50, 50).0[3], 0);
            this.create_component(window, cx);
            this.sync_components(window, cx);
            let component = this.hierarchy.components[&5].component.clone();
            this.insert_document_component(&component, false, Some(point(200., 0.)), window, cx);
            let instance = this.selected_mask().unwrap();
            assert_ne!(this.hierarchy.groups[&instance].mask, Some(3));
            this.snapshot_page(cx).0.validate().unwrap();
            this.set_selection(BTreeSet::from([1]), cx);
            this.edit_shape(|s| s.rect.width = 80.);
            this.sync_components(window, cx);
            assert_eq!(this.group_bounds(instance).unwrap().width, 80.);
            this.shapes.push(Shape::new(
                this.next_id,
                None,
                ShapeKind::Ellipse,
                rect(0., 0., 100., 100.),
            ));
            let outer_source = this.next_id;
            this.next_id += 1;
            this.hierarchy.order.insert(0, outer_source);
            this.set_selection(BTreeSet::from([outer_source, 5]), cx);
            this.create_mask(window, cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.get_pixel(0, 0).0[3], 0);
            assert_eq!(image.get_pixel(50, 50).0[3], 0);
            assert_eq!(image.get_pixel(50, 10).0, [0, 255, 0, 255]);
        })
        .unwrap();
}
