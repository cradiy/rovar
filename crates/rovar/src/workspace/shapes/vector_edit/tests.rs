use super::*;
use crate::workspace::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};

fn fixture(cx: &mut TestAppContext, kind: ShapeKind, rotation: f32) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            this.view.pan = point(300., 160.);
            this.view.zoom = 1.25;
            let mut shape = Shape::new(
                1,
                None,
                kind,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 180.,
                    height: 120.,
                },
            );
            shape.layer.rotation = rotation;
            shape.fill_mode = FillMode::Linear;
            this.shapes.push(shape);
            this.next_id = 2;
            this.select_shape(1, cx);
        })
        .unwrap();
    window
}
fn double_click(visual: &mut VisualTestContext, position: Point<Pixels>) {
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    visual.simulate_mouse_up(position, MouseButton::Left, Default::default());
    draw(visual);
}
fn enter(visual: &mut VisualTestContext) {
    draw(visual);
    let center = visual.debug_bounds("shape-1").unwrap().center();
    double_click(visual, center);
    assert!(visual.debug_bounds("vector-toolbar").is_some());
    assert!(visual.debug_bounds("shape-handle-0").is_none());
}
fn displayed(shape: &Shape) -> Vec<bezier::Node> {
    shape
        .editable_nodes()
        .iter()
        .map(|n| {
            n.map(|p| {
                crate::rotation::around(
                    p,
                    crate::rotation::center(shape.rect),
                    shape.layer.rotation,
                )
            })
        })
        .collect()
}
fn near(a: Point<f32>, b: Point<f32>) {
    assert!((a.x - b.x).hypot(a.y - b.y) < 0.03, "{a:?} != {b:?}");
}

#[gpui::test]
fn edge_hover_shows_fixed_midpoint_and_keeps_arbitrary_double_click_insertion(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx, ShapeKind::Rectangle, 30.);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    enter(&mut visual);
    let (edge, midpoint, anchor, before) = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            let screen = |p| {
                this.bounds.get().origin
                    + this
                        .view
                        .screen(crate::rotation::around(
                            p,
                            crate::rotation::center(shape.rect),
                            30.,
                        ))
                        .map(px)
            };
            (
                screen(point(45., 0.)),
                screen(point(90., 0.)),
                screen(point(0., 0.)),
                shape.clone(),
            )
        })
        .unwrap();
    visual.simulate_mouse_move(edge, None, Default::default());
    draw(&mut visual);
    let preview = visual.debug_bounds("vector-insert-preview").unwrap();
    // Layout snaps the marker to screen pixels; it must remain on the projected edge.
    let offset = (preview.center() - midpoint).map(f32::from);
    assert!(offset.x.hypot(offset.y) < 1.);
    // Moving along this segment keeps the marker at its midpoint.
    visual.simulate_mouse_move(edge + (midpoint - edge) * 0.5, None, Default::default());
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("vector-insert-preview").unwrap(),
        preview
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    visual.simulate_mouse_move(anchor, None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("vector-insert-preview").is_none());
    visual.simulate_mouse_move(edge, None, Default::default());
    draw(&mut visual);
    double_click(&mut visual, edge);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].editable_nodes().len(), 5);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    assert!(visual.debug_bounds("vector-insert-preview").is_none());
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before)
        })
        .unwrap();
    visual.simulate_mouse_move(edge, None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("vector-insert-preview").is_some());
    click(&mut visual, "vector-insert-preview");
    window
        .update(&mut visual.cx, |this, _, _| {
            let nodes = this.shapes[0].editable_nodes();
            assert_eq!(nodes.len(), 5);
            near(nodes[1].anchor, point(90., 0.));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    assert!(visual.debug_bounds("vector-insert-preview").is_none());
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("vector-insert-preview").is_none());
}

#[gpui::test]
fn rotated_rectangle_enters_without_mutation_then_node_drag_converts_and_undo_restores_primitive(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx, ShapeKind::Rectangle, 30.);
    let before = window
        .update(cx, |this, _, _| this.shapes[0].clone())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    enter(&mut visual);
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before);
            assert_eq!(this.history.borrow().undo_len(), 0);
            assert!(this.vector_edit.is_none());
        })
        .unwrap();
    enter(&mut visual);
    let pivot = visual.debug_bounds("shape-1").unwrap().center();
    let raw = visual.debug_bounds("bezier-node-0-0").unwrap().center();
    let start = crate::rotation::pixels(raw, pivot, 30.);
    let end = start + point(px(30.), px(-20.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    let changed = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            assert_eq!(shape.kind, ShapeKind::Bezier);
            assert!(shape.closed && shape.fill_enabled);
            assert_eq!(shape.gradient, before.gradient);
            let old = displayed(&before);
            let now = displayed(shape);
            near(now[0].anchor, old[0].anchor + point(24., -16.));
            for i in 1..4 {
                near(now[i].anchor, old[i].anchor);
            }
            assert_eq!(this.history.borrow().undo_len(), 1);
            shape.clone()
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before);
            assert_eq!(this.vector_edit, Some(1));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    click(&mut visual, "vector-done");
    assert!(visual.debug_bounds("bezier-node-0-0").is_none());
    assert!(visual.debug_bounds("shape-handle-0").is_some());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], changed)
        })
        .unwrap();
}

#[gpui::test]
fn edges_move_and_bend_with_cancel_insertion_and_one_step_history(cx: &mut TestAppContext) {
    let window = fixture(cx, ShapeKind::Rectangle, 0.);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    enter(&mut visual);
    let rect = visual.debug_bounds("shape-1").unwrap();
    let start = point(rect.center().x, rect.top() + px(1.));
    let end = start + point(px(0.), px(25.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = this.shapes[0].world_nodes();
            near(n[0].anchor, point(0., 20.));
            near(n[1].anchor, point(180., 20.));
            near(n[2].anchor, point(180., 120.));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    click(&mut visual, "vector-bend");
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].kind, ShapeKind::Rectangle);
            assert_eq!(this.vector_edit, Some(1));
            assert!(this.history.borrow().can_redo());
        })
        .unwrap();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = this.shapes[0].world_nodes();
            near(n[0].anchor, point(0., 0.));
            near(n[1].anchor, point(180., 0.));
            let mid = n[0].anchor * 0.125
                + n[0].outgoing * 0.375
                + n[1].incoming * 0.375
                + n[1].anchor * 0.125;
            near(mid, point(90., 20.));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    double_click(&mut visual, end - point(px(0.), px(1.)));
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 5)
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 4)
        })
        .unwrap();
}

#[gpui::test]
fn ellipse_and_independent_rounded_corners_restore_exactly_after_editing(cx: &mut TestAppContext) {
    for kind in [ShapeKind::Ellipse, ShapeKind::Rectangle] {
        let window = fixture(cx, kind, 0.);
        let before = window
            .update(cx, |this, _, cx| {
                this.shapes[0].independent_corners = true;
                this.shapes[0].corners = Some([12., 24., 36., 0.]);
                cx.notify();
                this.shapes[0].clone()
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        enter(&mut visual);
        let nodes = before.editable_nodes();
        assert_eq!(nodes.len(), if kind == ShapeKind::Ellipse { 4 } else { 7 });
        let p = visual.debug_bounds("bezier-node-0-0").unwrap().center();
        visual.simulate_click(p, Default::default());
        visual.simulate_keystrokes("shift-right");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                let changed = this.shapes[0].world_nodes();
                near(changed[0].anchor, nodes[0].anchor + point(10., 0.));
                for i in 1..nodes.len() {
                    near(changed[i].anchor, nodes[i].anchor);
                }
            })
            .unwrap();
        visual.simulate_keystrokes("ctrl-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0], before)
            })
            .unwrap();
    }
}
