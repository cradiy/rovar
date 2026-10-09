use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{EntityInputHandler, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use std::collections::BTreeSet;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.01, "{a} != {b}");
}
fn near_point(a: Point<f32>, b: Point<f32>) {
    near(a.x, b.x);
    near(a.y, b.y);
}
fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(320., 180.);
            this.view.zoom = 1.;
            this.shapes.push(Shape::new(
                1,
                None,
                ShapeKind::Rectangle,
                rect(0., 0., 200., 100.),
            ));
            this.next_id = 2;
            this.set_selection(BTreeSet::from([1]), cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
fn displayed(visual: &mut VisualTestContext, selector: &'static str, angle: f32) -> Point<Pixels> {
    let pivot = visual.debug_bounds("shape-1").unwrap().center();
    let p = visual.debug_bounds(selector).unwrap().center();
    geometry::pixels(p, pivot, angle)
}

#[gpui::test]
fn rotation_drag_shift_cancel_property_and_copy_share_history(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = displayed(&mut visual, "rotate-handle-2", 0.);
    let pivot = visual.debug_bounds("shape-1").unwrap().center();
    let end = geometry::pixels(start, pivot, 38.);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.object_rotation(1), 45.);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    visual.simulate_mouse_up(end, MouseButton::Left, shift);
    draw(&mut visual);
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.object_rotation(1), 45.)
        })
        .unwrap();
    click(&mut visual, "property-15");
    visual.simulate_keystrokes("secondary-a 4 5 0 enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            near(this.object_rotation(1), 90.);
            assert_eq!(this.inspector.fields[15].read(cx).value().as_ref(), "90");
            this.duplicate_selection(window, cx);
            near(this.shapes[1].layer.rotation, 90.);
            this.replay_history(false, window, cx);
            this.replay_history(false, window, cx);
            near(this.object_rotation(1), 45.);
        })
        .unwrap();
}

#[gpui::test]
fn rotated_resize_preserves_world_anchor_ratio_and_actual_hit_region(cx: &mut TestAppContext) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, _| this.shapes[0].layer.rotation = 45.)
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let original = rect(0., 0., 200., 100.);
    let anchor = geometry::around(point(0., 0.), center(original), 45.);
    let start = displayed(&mut visual, "shape-handle-4", 45.);
    let end = start + geometry::vector(point(100., 50.), 45.).map(px);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    visual.simulate_mouse_down(start, MouseButton::Left, shift);
    visual.simulate_mouse_move(end, MouseButton::Left, shift);
    visual.simulate_mouse_up(end, MouseButton::Left, shift);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let r = this.shapes[0].rect;
            near(r.width, 300.);
            near(r.height, 150.);
            near_point(geometry::around(point(r.x, r.y), center(r), 45.), anchor);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    // Pick a rotated-out corner beyond the edge's six-pixel resize hit slop.
    let bounds = visual.debug_bounds("shape-1").unwrap();
    let empty_corner = bounds.origin + point(bounds.size.width - px(2.), px(2.));
    visual.simulate_click(empty_corner, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.selection_ids().is_empty())
        })
        .unwrap();
    // The part extending beyond the old layout box remains selectable and draggable.
    let center = visual.debug_bounds("shape-1").unwrap().center();
    let inside = geometry::pixels(center + point(px(85.), px(0.)), center, 45.);
    visual.simulate_mouse_down(inside, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        inside + point(px(30.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        inside + point(px(30.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.shapes[0].rect.x, 30.);
            near(this.shapes[0].rect.y, 20.);
        })
        .unwrap();
}

#[gpui::test]
fn shape_edges_resize_away_from_midpoints_with_only_four_visible_corners(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for angle in [0., 45.] {
        for (index, selector) in [
            (1, "shape-handle-1"),
            (3, "shape-handle-3"),
            (5, "shape-handle-5"),
            (7, "shape-handle-7"),
        ] {
            let handle = Handle::ALL[index];
            window
                .update(&mut visual.cx, |this, _, cx| {
                    this.view.zoom = 2.;
                    this.shapes[0].rect = rect(0., 0., 200., 100.);
                    this.shapes[0].layer.rotation = angle;
                    this.select_shape(1, cx);
                    cx.notify();
                })
                .unwrap();
            draw(&mut visual);
            for (i, selector) in [
                "shape-corner-0",
                "shape-corner-1",
                "shape-corner-2",
                "shape-corner-3",
                "shape-corner-4",
                "shape-corner-5",
                "shape-corner-6",
                "shape-corner-7",
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(visual.debug_bounds(selector).is_some(), i % 2 == 0);
            }
            let edge = visual.debug_bounds(selector).unwrap();
            let pivot = visual.debug_bounds("shape-1").unwrap().center();
            // Use the first quarter of the edge, well away from the old midpoint handle.
            let start = geometry::pixels(
                edge.origin + edge.size.map(|v| v * 0.25).into(),
                pivot,
                angle,
            );
            let delta = point(handle.0 as f32 * 20., handle.1 as f32 * 20.);
            let end = start + geometry::vector(delta * 2., angle).map(px);
            visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
            window
                .update(&mut visual.cx, |this, _, _| {
                    let GestureKind::Shape {
                        handle: Some(active),
                        ..
                    } = this.gesture.unwrap().kind
                    else {
                        panic!("edge must start resizing, not moving");
                    };
                    assert_eq!((active.0, active.1), (handle.0, handle.1));
                })
                .unwrap();
            visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
            visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
            draw(&mut visual);
            window
                .update(&mut visual.cx, |this, window, cx| {
                    let resized = this.shapes[0].rect;
                    near(resized.width, 200. + if handle.0 != 0 { 20. } else { 0. });
                    near(resized.height, 100. + if handle.1 != 0 { 20. } else { 0. });
                    this.replay_history(false, window, cx);
                    assert_eq!(this.shapes[0].rect, rect(0., 0., 200., 100.));
                })
                .unwrap();
        }
    }
}

#[gpui::test]
fn rotated_text_input_uses_source_geometry_and_displayed_pointer(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let editor = window
        .update(cx, |this, window, cx| {
            this.shapes.clear();
            let mut text = this.make_text(2, None, rect(0., 0., 240., 110.), window, cx);
            text.layer.rotation = 45.;
            text.editor.update(cx, |e, cx| {
                e.replace_text_in_range(None, "abcd你好", window, cx)
            });
            let editor = text.editor.clone();
            this.texts.push(text);
            this.set_selection(BTreeSet::from([2]), cx);
            editor.update(cx, |e, _| e.editing = true);
            editor.read(cx).focus.clone().focus(window, cx);
            editor
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let pivot = visual.debug_bounds("text-box-2").unwrap().center();
    let caret = visual.update(|window, cx| {
        editor.update(cx, |e, cx| {
            let b = e
                .bounds_for_range(4..4, Default::default(), window, cx)
                .unwrap();
            assert_eq!(e.character_index_for_point(b.center(), window, cx), Some(4));
            geometry::pixel_bounds(b, pivot, 45.)
        })
    });
    visual.simulate_click(caret.center(), Default::default());
    draw(&mut visual);
    visual.update(|window, cx| {
        editor.update(cx, |e, cx| {
            assert_eq!(
                e.selected_text_range(false, window, cx).unwrap().range,
                4..4
            );
        })
    });
    visual.simulate_input("X");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(editor.read(cx).content, "abcdX你好");
            near(this.texts[0].layer.rotation, 45.);
        })
        .unwrap();
}

#[gpui::test]
fn rotated_line_endpoint_keeps_other_endpoint_and_undo_exact(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let before = window
        .update(cx, |this, _, cx| {
            this.shapes[0].kind = ShapeKind::Line;
            this.shapes[0].set_path(&[point(0., 0.), point(200., 100.)]);
            this.shapes[0].layer.rotation = 30.;
            this.sync_fields(cx);
            this.shapes[0].clone()
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let start = displayed(&mut visual, "line-end-1", 30.);
    let end = start + point(px(50.), px(-30.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near_point(
                this.shapes[0].display_path_point(0),
                before.display_path_point(0),
            );
            near_point(
                this.shapes[0].display_path_point(1),
                before.display_path_point(1) + point(50., -30.),
            );
            near(this.shapes[0].layer.rotation, 30.);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before)
        })
        .unwrap();
}

#[gpui::test]
fn rotated_side_resize_snaps_in_world_space_and_angle_scrub_cancel_keeps_redo(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    window
        .update(cx, |this, _, cx| {
            this.add_artboard(rect(100., 200., 400., 400.), cx);
            let board = this.selected.unwrap();
            this.shapes[0].board = Some(board);
            this.shapes[0].rect = rect(50., 100., 100., 50.);
            this.shapes[0].layer.rotation = 90.;
            this.shapes.push(Shape::new(
                3,
                None,
                ShapeKind::Rectangle,
                rect(180., 425., 40., 80.),
            ));
            this.next_id = 4;
            this.snapping.enabled = true;
            this.set_selection(BTreeSet::from([1]), cx);
            let original = this.shapes[0].rect;
            this.begin_snapping(GestureKind::Shape {
                id: 1,
                original,
                handle: Some(Handle(1, 0)),
            });
            let r = this.resize_with_snapping(1, original, Handle(1, 0), point(0., 47.), false);
            near(r.width, 150.);
            near(r.height, 50.);
            near_point(
                geometry::around(point(r.x, r.y + r.height / 2.), center(r), 90.),
                point(100., 75.),
            );
            this.snapping.bypass = true;
            near(
                this.resize_with_snapping(1, original, Handle(1, 0), point(0., 47.), false)
                    .width,
                147.,
            );
            this.snapping.clear();
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "property-15");
    visual.simulate_keystrokes("secondary-a 1 2 0 enter");
    draw(&mut visual);
    let start = visual.debug_bounds("property-drag-15").unwrap().center();
    let end = start + point(px(40.), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.object_rotation(1), 160.)
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("secondary-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.object_rotation(1), 90.)
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.object_rotation(1), 120.)
        })
        .unwrap();
}
