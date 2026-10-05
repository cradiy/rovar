use super::*;
use crate::{
    document::{Page, export::Format},
    editor::tests::open,
};
use gpui::{TestAppContext, VisualTestContext};
use std::collections::BTreeSet;

#[gpui::test]
fn boolean_group_keeps_current_menu_option_and_editable_operands(cx: &mut TestAppContext) {
    use crate::editor::tests::{click, draw};
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.group_selection(cx);
            assert!(this.can_boolean());
            this.apply_boolean(Operation::Subtract, window, cx);
            this.view.zoom = 1.;
            this.view.pan = point(300., 250.);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-3").is_some());
    let operation = visual.debug_bounds("boolean-menu").unwrap();
    let release = visual.debug_bounds("boolean-release").unwrap();
    assert_eq!(operation.center().y, release.center().y);
    assert!(operation.right() < release.left());
    assert!(visual.debug_bounds("shape-1").is_none());
    assert!(visual.debug_bounds("shape-2").is_none());
    handle
        .update(&mut visual.cx, |this, window, cx| {
            this.enter_vector_edit(2, window, cx);
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-3").is_some());
    assert!(visual.debug_bounds("shape-2").is_some());
    handle
        .update(&mut visual.cx, |this, _, cx| {
            this.exit_vector_edit(cx);
            this.set_selection(BTreeSet::from([3]), cx);
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "boolean-menu");
    let current = visual.debug_bounds("boolean-current-option").unwrap();
    assert!(
        visual
            .debug_bounds("boolean-subtract-option")
            .unwrap()
            .contains(&current.center())
    );
    click(&mut visual, "boolean-exclude-option");
    click(&mut visual, "boolean-menu");
    let current = visual.debug_bounds("boolean-current-option").unwrap();
    assert!(
        visual
            .debug_bounds("boolean-exclude-option")
            .unwrap()
            .contains(&current.center())
    );
    visual.simulate_keystrokes("escape");
    click(&mut visual, "boolean-release");
    handle
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.hierarchy.groups.is_empty());
            assert_eq!(this.shapes.len(), 2);
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
        })
        .unwrap();
    handle
        .update(&mut visual.cx, |this, window, cx| {
            this.undo_redo(false, window, cx);
            assert_eq!(this.hierarchy.groups[&3].boolean, Some(Operation::Exclude));
            this.undo_redo(true, window, cx);
            assert!(this.hierarchy.groups.is_empty());
        })
        .unwrap();
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn operands(this: &mut Workspace, cx: &mut Context<Workspace>) {
    this.shapes = vec![
        Shape::new(1, None, ShapeKind::Rectangle, rect(0., 0., 100., 100.)),
        Shape::new(2, None, ShapeKind::Rectangle, rect(25., 25., 50., 50.)),
    ];
    this.shapes[0].color = gpui::rgba(0xff0000ff);
    this.shapes[1].color = gpui::rgba(0x0000ffff);
    this.next_id = 3;
    this.set_selection(BTreeSet::from([1, 2]), cx);
}

#[gpui::test]
fn disjoint_boolean_results_show_current_names_bounds_and_empty_state(cx: &mut TestAppContext) {
    use crate::editor::tests::draw;
    let handle = open(cx);
    handle
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.shapes[1].kind = ShapeKind::Ellipse;
            this.shapes[1].rect = rect(150., 25., 50., 50.);
            this.apply_boolean(Operation::Subtract, window, cx);
            assert_eq!(this.field_value(3, cx).as_deref(), Some("100"));
            this.apply_boolean(Operation::Union, window, cx);
            assert_eq!(this.layer_name(3, cx), Operation::Union.label());
            assert_eq!(this.field_value(3, cx).as_deref(), Some("200"));
            assert_eq!(this.boolean_geometry(3).unwrap().contours.len(), 2);
            this.apply_boolean(Operation::Exclude, window, cx);
            assert_eq!(this.layer_name(3, cx), Operation::Exclude.label());
            assert_eq!(this.boolean_geometry(3).unwrap().contours.len(), 2);
            this.apply_boolean(Operation::Intersect, window, cx);
            assert!(this.boolean_result_empty());
            assert_eq!(this.field_value(3, cx).as_deref(), Some("0"));
            assert_eq!(this.field_value(4, cx).as_deref(), Some("0"));
            let before = this.shapes.clone();
            assert!(!this.edit_multi_field(Property::Width, "200", cx));
            assert_eq!(this.shapes, before);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("boolean-empty-result").is_some());
    handle
        .update(&mut visual.cx, |this, window, cx| {
            this.undo_redo(false, window, cx);
            assert_eq!(this.layer_name(3, cx), Operation::Exclude.label());
            assert!(!this.boolean_result_empty());
            this.hierarchy.groups.get_mut(&3).unwrap().name = "Custom icon".into();
            this.apply_boolean(Operation::Union, window, cx);
            assert_eq!(this.layer_name(3, cx), "Custom icon");
        })
        .unwrap();
}

#[gpui::test]
fn boolean_properties_transform_the_result_and_edit_only_its_paint_source(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.apply_boolean(Operation::Subtract, window, cx);
            assert_eq!(this.field_value(1, cx).as_deref(), Some("0"));
            assert_eq!(this.field_value(5, cx).as_deref(), Some("FF0000"));
            assert!(this.edit_multi_field(Property::X, "20", cx));
            assert_eq!(this.shapes[0].rect.x, 20.);
            assert_eq!(this.shapes[1].rect.x, 45.);
            this.history.borrow_mut().break_group();
            assert!(this.edit_multi_field(Property::Width, "200", cx));
            assert_eq!(this.shapes[0].rect, rect(20., 0., 200., 100.));
            assert_eq!(this.shapes[1].rect, rect(70., 25., 100., 50.));
            this.undo_redo(false, window, cx);
            assert_eq!(this.shapes[0].rect, rect(20., 0., 100., 100.));
            assert_eq!(this.shapes[1].rect, rect(45., 25., 50., 50.));
            this.begin_multi_property(
                Property::Width,
                &gpui::MouseDownEvent::default(),
                window,
                cx,
            );
            this.move_multi_property(Property::Width, 20., false, cx);
            this.move_multi_property(Property::Width, 40., false, cx);
            assert_eq!(this.field_value(3, cx).as_deref(), Some("140"));
            this.cancel_gesture(window, cx);
            assert_eq!(this.field_value(3, cx).as_deref(), Some("100"));
            let top = this.shapes[1].clone();
            assert!(this.edit_multi_field(Property::Color, "00FF00", cx));
            assert_eq!(this.shapes[0].color, gpui::rgb(0x00ff00));
            assert_eq!(this.shapes[1], top);
            this.undo_redo(false, window, cx);
            assert_eq!(this.field_value(5, cx).as_deref(), Some("FF0000"));
        })
        .unwrap();
}

#[gpui::test]
fn boolean_create_edit_release_and_history_preserve_source_nodes(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            operands(this, cx);
            let original = this.shapes.clone();
            this.apply_boolean(Operation::Subtract, window, cx);
            let root = *this.selection_ids().first().unwrap();
            assert_eq!(this.shapes, original);
            assert_eq!(this.boolean_geometry(root).unwrap().contours.len(), 2);
            assert_eq!(this.content_elements(window, cx).len(), 1);
            this.undo_redo(false, window, cx);
            assert!(this.hierarchy.groups.is_empty());
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
            this.undo_redo(true, window, cx);
            assert_eq!(
                this.hierarchy.groups[&root].boolean,
                Some(Operation::Subtract)
            );
            this.set_selection(BTreeSet::from([2]), cx);
            this.edit_shape(|s| s.rect.x = 150.);
            assert_eq!(this.boolean_geometry(root).unwrap().contours.len(), 1);
            this.undo_redo(false, window, cx);
            assert_eq!(this.boolean_geometry(root).unwrap().contours.len(), 2);
            this.set_selection(BTreeSet::from([root]), cx);
            this.ungroup_selection(cx);
            assert!(this.hierarchy.groups.is_empty());
            assert_eq!(this.shapes, original);
            this.undo_redo(false, window, cx);
            let page = this.snapshot_page(cx).0;
            let loaded = Page::decode(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert_eq!(
                loaded.hierarchy.groups[&root].boolean,
                Some(Operation::Subtract)
            );
            assert_eq!(loaded.node_ids(), page.node_ids());
            this.duplicate_selection(window, cx);
            let duplicate = *this.selection_ids().first().unwrap();
            assert_ne!(root, duplicate);
            assert_eq!(
                this.hierarchy.groups[&duplicate].boolean,
                Some(Operation::Subtract)
            );
            this.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn boolean_components_propagate_operation_and_child_geometry(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.apply_boolean(Operation::Subtract, window, cx);
            let main = *this.selection_ids().first().unwrap();
            this.create_component(window, cx);
            this.sync_components(window, cx);
            let component = this.hierarchy.components[&main].component.clone();
            this.insert_document_component(&component, false, Some(point(200., 0.)), window, cx);
            let instance = *this.selection_ids().first().unwrap();
            assert_eq!(this.boolean_geometry(instance).unwrap().contours.len(), 2);
            this.set_selection(BTreeSet::from([main]), cx);
            this.apply_boolean(Operation::Intersect, window, cx);
            this.sync_components(window, cx);
            assert_eq!(
                this.hierarchy.groups[&instance].boolean,
                Some(Operation::Intersect)
            );
            assert_eq!(
                this.boolean_geometry(instance).unwrap().shape.rect.width,
                50.
            );
            this.set_selection(BTreeSet::from([2]), cx);
            this.edit_shape(|s| s.rect.width = 30.);
            this.sync_components(window, cx);
            assert_eq!(
                this.boolean_geometry(instance).unwrap().shape.rect.width,
                30.
            );
            this.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[gpui::test]
fn boolean_groups_use_composed_bounds_inside_auto_layout(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.shapes[1].rect = rect(50., 0., 100., 100.);
            this.apply_boolean(Operation::Subtract, window, cx);
            let root = *this.selection_ids().first().unwrap();
            assert!(!this.can_auto_layout());
            assert_eq!(this.group_bounds(root).unwrap().width, 50.);
            this.duplicate_selection(window, cx);
            let copy = *this.selection_ids().first().unwrap();
            this.set_selection(BTreeSet::from([root, copy]), cx);
            assert!(this.can_auto_layout());
            this.enable_auto_layout(window, cx);
            this.reflow_layout(cx);
            for id in [root, copy] {
                assert_eq!(this.boolean_geometry(id).unwrap().shape.rect.width, 50.);
            }
            this.snapshot_page(cx).0.validate().unwrap();
        })
        .unwrap();
}

#[cfg(not(target_family = "wasm"))]
#[gpui::test]
fn boolean_svg_png_and_operand_exports_preserve_holes_and_appearance(cx: &mut TestAppContext) {
    open(cx)
        .update(cx, |this, window, cx| {
            operands(this, cx);
            this.apply_boolean(Operation::Subtract, window, cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            assert_eq!(jobs[0].order, vec![3]);
            let options = crate::render::svg::render_options(false).unwrap();
            let svg = String::from_utf8(jobs[0].render(Format::Svg, 1, &options).unwrap()).unwrap();
            assert!(svg.contains("fill-rule=\"evenodd\""));
            assert!(!svg.contains("<image"));
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (100, 100));
            assert_eq!(image.get_pixel(10, 10).0, [255, 0, 0, 255]);
            assert_eq!(image.get_pixel(50, 50).0[3], 0);
            this.set_selection(BTreeSet::from([2]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let image = image::load_from_memory(&jobs[0].render(Format::Png, 1, &options).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (50, 50));
            assert_eq!(image.get_pixel(10, 10).0, [0, 0, 255, 255]);
        })
        .unwrap();
}
