use super::*;
use crate::editor::tests::{draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 100.);
            this.view.zoom = 2.;
            this.snapping.enabled = false;
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                rect(100., 100., 40., 40.),
            ));
            this.shapes.push(Shape::new(
                2,
                None,
                ShapeKind::Ellipse,
                rect(180., 100., 60., 40.),
            ));
            this.next_id = 3;
            this.set_selection(BTreeSet::from([1, 2]), cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}

#[gpui::test]
fn edge_resize_scales_positions_and_widths_once_and_undo_redo_restore_selection(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual
        .debug_bounds("selection-resize-handle-3")
        .unwrap()
        .center();
    let end = start + point(px(140.), px(30.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, rect(100., 100., 60., 40.));
            assert_eq!(this.shapes[1].rect, rect(220., 100., 90., 40.));
            assert_eq!(this.selection_ids(), BTreeSet::from([1, 2]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect, rect(180., 100., 60., 40.));
        })
        .unwrap();
    // Merely clicking a handle must preserve the redo stack.
    let start = visual
        .debug_bounds("selection-resize-handle-3")
        .unwrap()
        .center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect, rect(220., 100., 90., 40.));
        })
        .unwrap();
}

#[gpui::test]
fn shift_resize_keeps_opposite_corner_and_escape_restores_every_object(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = visual
        .debug_bounds("selection-resize-handle-0")
        .unwrap()
        .center();
    let end = start - point(px(140.), px(10.));
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, shift);
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, rect(30., 80., 60., 60.));
            assert_eq!(this.shapes[1].rect, rect(150., 80., 90., 60.));
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, rect(100., 100., 40., 40.));
            assert_eq!(this.shapes[1].rect, rect(180., 100., 60., 40.));
            assert_eq!(this.history.borrow().undo_len(), 0);
            assert!(this.selection_resize.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn resized_group_preserves_text_style_and_restores_children_on_undo(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            let text = this.make_text(3, None, rect(100., 160., 140., 40.), window, cx);
            this.texts.push(text);
            this.next_id = 4;
            this.set_selection(BTreeSet::from([1, 2, 3]), cx);
            this.group_selection(cx);
            let style = this.texts[0].editor.read(cx).snapshot();
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(Handle(1, 1), &gpui::MouseDownEvent::default(), window, cx);
            this.resize_selection(original, Handle(1, 1), point(140., 100.), true, cx);
            this.finish_gesture(window, cx);
            assert_eq!(this.shapes[1].rect, rect(260., 100., 120., 80.));
            assert_eq!(this.texts[0].rect, rect(100., 220., 280., 80.));
            assert!(this.texts[0].editor.read(cx).snapshot() == style);
            this.replay_history(false, window, cx);
            assert_eq!(this.texts[0].rect, rect(100., 160., 140., 40.));
            assert_eq!(this.shapes[1].rect, rect(180., 100., 60., 40.));
        })
        .unwrap();
}

#[gpui::test]
fn selection_resize_snaps_outer_edge_and_clamps_the_smallest_object(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.snapping.enabled = true;
            this.shapes.push(Shape::new(
                3,
                None,
                ShapeKind::Rectangle,
                rect(300., 100., 40., 40.),
            ));
            this.next_id = 4;
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(Handle(1, 0), &gpui::MouseDownEvent::default(), window, cx);
            this.resize_selection(original, Handle(1, 0), point(59., 0.), false, cx);
            let right = this.world_rect(2).unwrap();
            assert!((right.x + right.width - 300.).abs() < 0.001);
            assert_eq!(this.shapes[2].rect, rect(300., 100., 40., 40.));
            this.snapping.bypass = true;
            this.resize_selection(original, Handle(1, 0), point(59., 0.), false, cx);
            let right = this.world_rect(2).unwrap();
            assert!((right.x + right.width - 299.).abs() < 0.001);
            this.resize_selection(original, Handle(1, 0), point(-1000., 0.), false, cx);
            assert_eq!(this.shapes[0].rect.width, 1.);
            assert_eq!(this.shapes[1].rect.width, 1.5);
            assert_eq!(this.shapes[0].rect.x, 100.);
            this.resize_selection(original, Handle(1, 0), point(0., 0.), false, cx);
            this.finish_gesture(window, cx);
            assert_eq!(this.shapes[1].rect, rect(180., 100., 60., 40.));
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn selected_frame_and_child_are_not_scaled_twice_and_ownership_stays_fixed(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.add_artboard(rect(80., 80., 200., 120.), cx);
            this.shapes[0].board = Some(3);
            this.shapes[0].rect = rect(20., 20., 40., 40.);
            this.shapes[1].rect = rect(400., 80., 80., 80.);
            this.set_selection(BTreeSet::from([1, 2, 3]), cx);
            assert_eq!(this.selection_ids(), BTreeSet::from([2, 3]));
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(
                Handle(-1, -1),
                &gpui::MouseDownEvent::default(),
                window,
                cx,
            );
            this.resize_selection(original, Handle(-1, -1), point(-400., -120.), true, cx);
            this.finish_gesture(window, cx);
            assert_eq!(this.boards[0].rect, rect(-320., -40., 400., 240.));
            assert_eq!(this.shapes[0].rect, rect(20., 20., 40., 40.));
            assert_eq!(this.world_rect(1).unwrap(), rect(-300., -20., 40., 40.));
            assert_eq!(this.shapes[0].board, Some(3));
            assert_eq!(this.shapes[1].board, None);
            this.replay_history(false, window, cx);
            assert_eq!(this.world_rect(1).unwrap(), rect(100., 100., 40., 40.));
        })
        .unwrap();
}

#[gpui::test]
fn layout_container_resize_fixes_hug_size_and_undo_restores_layout(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.enable_auto_layout(window, cx);
            let group = *this.selection_ids().first().unwrap();
            let old_frame = this.world_rect(group).unwrap();
            let hierarchy = this.hierarchy.clone();
            let shapes = this.shapes.clone();
            let undo_len = this.history.borrow().undo_len();
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(Handle(1, 0), &gpui::MouseDownEvent::default(), window, cx);
            this.resize_selection(original, Handle(1, 0), point(original.width, 0.), false, cx);
            this.finish_gesture(window, cx);
            this.reflow_layout(cx);
            assert_eq!(this.world_rect(group).unwrap().width, old_frame.width * 2.);
            assert_eq!(
                this.hierarchy.sizing[&group].width,
                crate::scene::auto_layout::Mode::Fixed
            );
            assert_eq!(this.history.borrow().undo_len(), undo_len + 1);
            this.replay_history(false, window, cx);
            this.reflow_layout(cx);
            assert!(this.hierarchy == hierarchy);
            assert_eq!(this.shapes, shapes);
            this.set_selection(BTreeSet::from([1, 2]), cx);
            assert!(this.selection_resize_bounds().is_none());
        })
        .unwrap();
}

#[gpui::test]
fn rotated_selection_uses_uniform_scaling_and_keeps_rotation(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.shapes[0].layer.rotation = 45.;
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(Handle(1, 0), &gpui::MouseDownEvent::default(), window, cx);
            this.resize_selection(original, Handle(1, 0), point(original.width, 0.), false, cx);
            assert_eq!(this.shapes[0].rect.width, 80.);
            assert_eq!(this.shapes[0].rect.height, 80.);
            assert_eq!(this.shapes[0].layer.rotation, 45.);
            let resized = this.selection_resize_bounds().unwrap();
            assert!((resized.x - original.x).abs() < 0.001);
            assert!((resized.width - original.width * 2.).abs() < 0.001);
            this.cancel_gesture(window, cx);
            assert_eq!(this.shapes[0].rect, rect(100., 100., 40., 40.));
        })
        .unwrap();
}

#[gpui::test]
fn distant_objects_and_zero_height_paths_do_not_jump_or_produce_invalid_geometry(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, window, cx| {
            this.shapes[0].rect.x = -150_000.;
            this.shapes[1].rect.x = 150_000.;
            this.shapes[1].kind = ShapeKind::Line;
            this.shapes[1].rect.height = 0.;
            let original = this.selection_resize_bounds().unwrap();
            this.begin_selection_resize(Handle(1, 0), &gpui::MouseDownEvent::default(), window, cx);
            this.resize_selection(original, Handle(1, 0), point(original.width, 0.), false, cx);
            assert_eq!(this.shapes[0].rect, rect(-150_000., 100., 80., 40.));
            assert_eq!(this.shapes[1].rect, rect(450_000., 100., 120., 0.));
            this.cancel_gesture(window, cx);
            assert_eq!(this.shapes[1].rect, rect(150_000., 100., 60., 0.));
        })
        .unwrap();
}
