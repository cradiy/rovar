use super::super::tests::{click, create, draw, open};
use super::{ShapeKind, *};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn paint_sections_stay_visible_and_popovers_keep_target_and_stops(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1000.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "stroke-visibility");
    assert!(visual.debug_bounds("property-5").is_some());
    assert!(visual.debug_bounds("property-16").is_some());
    click(&mut visual, "shape-fill");
    assert!(visual.debug_bounds("property-5").is_some());
    assert!(visual.debug_bounds("property-16").is_some());
    click(&mut visual, "shape-fill");
    click(&mut visual, "shape-stroke");
    assert!(visual.debug_bounds("property-5").is_some());
    assert!(visual.debug_bounds("property-16").is_some());
    click(&mut visual, "shape-stroke");
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("FF0000");
    click(&mut visual, "property-16");
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("0000FF");
    click(&mut visual, "property-17");
    visual.simulate_keystrokes("secondary-a 4 0 enter");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].color, rgb(0xff0000));
            assert_eq!(this.shapes[0].stroke.color, rgb(0x0000ff).opacity(0.4));
            assert_eq!(this.inspector.fields[5].read(cx).value().as_ref(), "FF0000");
            assert_eq!(
                this.inspector.fields[16].read(cx).value().as_ref(),
                "0000FF"
            );
        })
        .unwrap();
    click(&mut visual, "property-drag-5");
    let panel = visual.debug_bounds("color-panel").unwrap();
    let sidebar = visual.debug_bounds("properties-panel").unwrap();
    assert!(panel.right() < sidebar.left());
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    let fill_stop = window
        .update(&mut visual.cx, |this, _, _| this.inspector.active_stop)
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("color-panel").is_none());
    assert!(visual.debug_bounds("property-16").is_some());
    click(&mut visual, "property-drag-16");
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-stop-1");
    visual.simulate_keystrokes("escape");
    click(&mut visual, "property-drag-5");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.inspector.active_stop, fill_stop);
            assert_eq!(this.shapes[0].gradient.stops().len(), 3);
            assert_eq!(this.shapes[0].stroke.gradient.stops().len(), 2);
            assert!(this.inspector.paint_popovers[0].read(cx).is_open());
        })
        .unwrap();
    let outside = point(px(400.), px(150.));
    visual.simulate_click(outside, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("color-panel").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.selected_shape.is_some())
        })
        .unwrap();
}

#[gpui::test]
fn shape_properties_and_gradient_are_independent_of_the_board_and_other_shapes(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1200.)));
    click(&mut visual, "add-rectangle");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.shapes.is_empty()))
        .unwrap();
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("secondary-a 3 2 0");
    click(&mut visual, "property-9");
    visual.simulate_keystrokes("secondary-a 4 0");
    click(&mut visual, "property-6");
    visual.simulate_keystrokes("secondary-a 5 0");
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("secondary-a 4 5");
    click(&mut visual, "property-8");
    visual.simulate_keystrokes("secondary-a 1 0");
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("secondary-a f f 0 0 0 0");
    click(&mut visual, "property-6");
    visual.simulate_keystrokes("secondary-a 2 5");
    draw(&mut visual);
    let rectangle = window
        .update(&mut visual.cx, |this, _, cx| {
            let shape = &this.shapes[0];
            assert_eq!(shape.board, Some(1));
            assert_eq!(shape.kind, ShapeKind::Rectangle);
            assert_eq!(shape.rect.width, 320.);
            assert_eq!(shape.radius, 40.);
            assert_eq!(shape.color.a, 0.5);
            assert_eq!(shape.gradient.stops().len(), 4);
            assert_eq!(shape.gradient.angle, 45.);
            assert_eq!(
                shape
                    .gradient
                    .stop(this.inspector.active_stop)
                    .unwrap()
                    .position,
                0.1
            );
            assert_eq!(shape.paint_color(this.inspector.active_stop, false).r, 1.);
            assert_eq!(shape.paint_color(this.inspector.active_stop, false).g, 0.);
            assert_eq!(shape.paint_color(this.inspector.active_stop, false).a, 0.25);
            assert_eq!(this.boards[0].rect.width, 640.);
            assert_eq!(this.boards[0].fill_mode, FillMode::Solid);
            assert_eq!(
                this.inspector.fields[0].read(cx).value().as_ref(),
                "Rectangle 2"
            );
            shape.clone()
        })
        .unwrap();
    // A newly added shape stop must not leak into its owner's gradient selection.
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.select(Some(1), cx);
            assert!(
                this.boards[0]
                    .gradient
                    .stop(this.inspector.active_stop)
                    .is_some()
            );
            assert_eq!(this.inspector.fields[8].read(cx).value().as_ref(), "0");
        })
        .unwrap();
    create(&mut visual, "add-ellipse");
    assert!(visual.debug_bounds("property-9").is_none());
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("secondary-a 1 2 0");
    click(&mut visual, "property-4");
    visual.simulate_keystrokes("secondary-a 2 4 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], rectangle);
            assert_eq!(this.shapes[1].kind, ShapeKind::Ellipse);
            assert_eq!(this.shapes[1].rect.width, 120.);
            assert_eq!(this.shapes[1].rect.height, 240.);
            assert!(this.shape_paths.borrow().contains_key(&3));
        })
        .unwrap();
    // Invalid values never enter history or corrupt ellipse geometry.
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("secondary-a 0");
    click(&mut visual, "property-4");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[1].rect.width, 120.);
            assert_eq!(this.inspector.fields[3].read(cx).value().as_ref(), "120");
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect.height, 160.)
        })
        .unwrap();
}

#[gpui::test]
fn shape_resize_uses_world_coordinates_and_board_movement_preserves_local_position(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    let original = window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(320., 240.), 0.5);
            cx.notify();
            this.shapes[0].rect
        })
        .unwrap();
    draw(&mut visual);
    let start = visual.debug_bounds("shape-handle-4").unwrap().center();
    let end = start + point(px(40.), px(20.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect.width, original.width + 80.);
            assert_eq!(this.shapes[0].rect.height, original.height + 40.);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, original)
        })
        .unwrap();
    let start = visual.debug_bounds("shape-2").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start + point(px(30.), px(15.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, original)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    let shape_before = visual.debug_bounds("shape-2").unwrap();
    let board = visual.debug_bounds("artboard-1").unwrap();
    let grab = board.origin + point(px(10.), px(10.));
    visual.simulate_mouse_down(grab, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        grab + point(px(20.), px(10.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        grab + point(px(20.), px(10.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("shape-2").unwrap().origin,
        shape_before.origin + point(px(20.), px(10.))
    );
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].rect.x, original.x);
            assert_eq!(this.shapes[0].rect.y, original.y);
            this.select_shape(2, cx);
        })
        .unwrap();
    click(&mut visual, "property-1");
    visual.simulate_keystrokes("secondary-a - 2 0");
    draw(&mut visual);
    let shape = visual.debug_bounds("shape-2").unwrap();
    assert!(shape.left() < visual.debug_bounds("artboard-1").unwrap().left());
    visual.simulate_click(shape.origin + point(px(2.), px(20.)), Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2))
        })
        .unwrap();
}

#[gpui::test]
fn shape_text_stacking_and_board_delete_restore_share_one_history(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    create(&mut visual, "add-text");
    visual.simulate_input("layer text");
    visual.simulate_keystrokes("escape");
    create(&mut visual, "add-ellipse");
    let center = visual.debug_bounds("shape-4").unwrap().center();
    visual.simulate_click(center, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(4));
            assert!(this.selected_text.is_none());
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    visual.simulate_click(center, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_text, Some(3));
            assert!(this.selected_shape.is_none());
            assert!(!this.shape_paths.borrow().contains_key(&4));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    let saved = window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.shapes.len(), 2);
            this.select(Some(1), cx);
            this.focus.focus(window, cx);
            this.shapes.clone()
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.boards.is_empty() && this.shapes.is_empty() && this.texts.is_empty());
            assert!(this.shape_paths.borrow().is_empty());
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes, saved);
            assert_eq!(this.texts[0].editor.read(cx).content, "layer text");
            assert_eq!(this.selected, Some(1));
            assert!(this.selected_shape.is_none());
        })
        .unwrap();
    visual.simulate_click(center, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(4))
        })
        .unwrap();
    // A selection-only change leaves deletion redo available.
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.is_empty() && this.texts.is_empty())
        })
        .unwrap();
}

#[gpui::test]
fn fill_stroke_and_corner_modes_preserve_independent_values_and_undo(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1600.)));
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    click(&mut visual, "property-9");
    visual.simulate_keystrokes("secondary-a 2 4");
    click(&mut visual, "corners-independent");
    click(&mut visual, "property-11");
    visual.simulate_keystrokes("secondary-a 4 8");
    click(&mut visual, "corners-unified");
    click(&mut visual, "property-9");
    visual.simulate_keystrokes("secondary-a 1 2");
    click(&mut visual, "corners-independent");
    window
        .update(&mut visual.cx, |this, _, cx| {
            let shape = &this.shapes[0];
            assert_eq!(shape.radius, 12.);
            assert_eq!(shape.corners, Some([24., 48., 24., 24.]));
            assert_eq!(this.inspector.fields[11].read(cx).value().as_ref(), "48");
        })
        .unwrap();
    click(&mut visual, "property-5");
    visual.simulate_keystrokes("secondary-a f f 0 0 0 0");
    click(&mut visual, "stroke-visibility");
    click(&mut visual, "property-14");
    visual.simulate_keystrokes("secondary-a 2 0");
    click(&mut visual, "stroke-outside");
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    click(&mut visual, "gradient-add");
    click(&mut visual, "property-16");
    visual.simulate_keystrokes("secondary-a 0 0 f f 0 0");
    click(&mut visual, "property-17");
    visual.simulate_keystrokes("secondary-a 4 0");
    draw(&mut visual);
    let geometry = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            assert_eq!(shape.color, rgb(0xff0000));
            assert_eq!(shape.fill_mode, FillMode::Solid);
            assert_eq!(shape.stroke.gradient.stops().len(), 4);
            assert_eq!(shape.stroke.width, 20.);
            assert_eq!(
                shape.stroke.align,
                crate::scene::shape::StrokeAlign::Outside
            );
            let color = shape.paint_color(this.inspector.active_stop, true);
            assert_eq!(color.g, 1.);
            assert_eq!(color.r, 0.);
            assert_eq!(color.a, 0.4);
            this.shape_paths.borrow()[&2].clone()
        })
        .unwrap();
    // Outside stroke expands the hit area, but the resize handles retain model bounds.
    click(&mut visual, "color-close");
    let bounds = visual.debug_bounds("shape-2").unwrap();
    let handle = visual.debug_bounds("shape-handle-0").unwrap().center();
    assert_eq!(handle, bounds.origin + point(px(20.), px(20.)));
    let start = bounds.origin + point(px(2.), bounds.size.height / 2.);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start + point(px(10.), px(10.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        start + point(px(10.), px(10.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("shape-2").unwrap().origin,
        bounds.origin + point(px(10.), px(10.))
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.inspector.stroke_editing);
            assert!(Rc::ptr_eq(&geometry, &this.shape_paths.borrow()[&2]));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    let (before_picker, depth) = window
        .update(&mut visual.cx, |this, _, _| {
            (
                this.shapes[0].stroke.gradient.clone(),
                this.history.borrow().undo_len(),
            )
        })
        .unwrap();
    for color in [rgb(0xff00ff), rgb(0x0000ff)] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.inspector
                    .picker
                    .update(cx, |_, cx| cx.emit(ColorPickerEvent::Preview(color)));
            })
            .unwrap();
        draw(&mut visual);
    }
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.inspector
                .picker
                .update(cx, |_, cx| cx.emit(ColorPickerEvent::Commit(rgb(0x0000ff))));
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].color, rgb(0xff0000));
            assert_eq!(
                this.shapes[0]
                    .paint_color(this.inspector.active_stop, true)
                    .b,
                1.
            );
            assert_eq!(this.history.borrow().undo_len(), depth + 1);
            assert!(Rc::ptr_eq(&geometry, &this.shape_paths.borrow()[&2]));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].stroke.gradient, before_picker)
        })
        .unwrap();
    // The inactive fill section has its own visibility control while editing the stroke.
    click(&mut visual, "fill-visibility");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes[0].fill_enabled);
            assert!(this.shapes[0].stroke.enabled);
            assert!(!this.inspector.stroke_editing);
            assert!(Rc::ptr_eq(&geometry, &this.shape_paths.borrow()[&2]));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes[0].fill_enabled)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    click(&mut visual, "stroke-visibility");
    click(&mut visual, "shape-paint-enabled");
    click(&mut visual, "property-14");
    visual.simulate_keystrokes("secondary-a - 1");
    click(&mut visual, "stroke-center");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[0].stroke.width, 20.);
            assert_eq!(this.inspector.fields[14].read(cx).value().as_ref(), "20");
            assert_eq!(this.shapes[0].stroke.gradient.stops().len(), 4);
            assert!(!this.shapes[0].fill_enabled);
        })
        .unwrap();
    // Only selection changes when switching the paint editor: no extra undo entry.
    click(&mut visual, "shape-fill");
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.shapes[0].stroke.align,
                crate::scene::shape::StrokeAlign::Outside
            );
        })
        .unwrap();
}

#[gpui::test]
fn undo_hiding_corner_or_stroke_inputs_keeps_keyboard_history_working(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1200.)));
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    click(&mut visual, "corners-independent");
    click(&mut visual, "property-10");
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, _| {
            assert!(!this.shapes[0].independent_corners);
            assert!(this.focus.is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("property-10").is_some());
    click(&mut visual, "stroke-visibility");
    click(&mut visual, "property-14");
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, _| {
            assert!(!this.shapes[0].stroke.enabled);
            assert!(this.focus.is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("property-14").is_some());
}
