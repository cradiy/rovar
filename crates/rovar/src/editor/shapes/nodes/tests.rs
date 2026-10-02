use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use std::collections::BTreeSet;

fn near(a: Point<f32>, b: Point<f32>) {
    assert!((a - b).x.hypot((a - b).y) < 0.03, "{a:?} != {b:?}");
}
fn curve(a: Node, b: Node, t: f32) -> Point<f32> {
    let u = 1. - t;
    a.anchor * (u * u * u)
        + a.outgoing * (3. * u * u * t)
        + b.incoming * (3. * u * t * t)
        + b.anchor * (t * t * t)
}
fn displayed(shape: &Shape) -> Vec<Node> {
    shape
        .world_nodes()
        .iter()
        .map(|n| {
            n.map(|p| {
                crate::scene::rotation::around(
                    p,
                    crate::scene::rotation::center(shape.rect),
                    shape.layer.rotation,
                )
            })
        })
        .collect()
}
fn fixture(cx: &mut TestAppContext) -> WindowHandle<Workspace> {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.view.pan = point(300., 180.);
            this.view.zoom = 1.25;
            let mut shape = Shape::new(
                1,
                None,
                ShapeKind::Bezier,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 1.,
                    height: 1.,
                },
            );
            let mut a = Node::corner(point(0., 80.));
            a.outgoing = point(60., -80.);
            let mut b = Node::corner(point(140., 100.));
            b.incoming = point(80., 200.);
            b.outgoing = point(180., 0.);
            let mut c = Node::corner(point(280., 60.));
            c.incoming = point(240., 140.);
            shape.set_bezier(&[a, b, c], false);
            shape.layer.rotation = 25.;
            shape.stroke.enabled = true;
            shape.fill_enabled = true;
            this.shapes.push(shape);
            this.next_id = 2;
            this.set_selection(BTreeSet::from([1]), cx);
            this.enter_vector_edit(1, window, cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    window
}
fn screen(
    window: WindowHandle<Workspace>,
    visual: &mut VisualTestContext,
    p: Point<f32>,
) -> Point<Pixels> {
    window
        .update(&mut visual.cx, |this, _, _| {
            this.bounds.get().origin + this.view.screen(p).map(px)
        })
        .unwrap()
}
fn knob(visual: &mut VisualTestContext, name: &'static str) -> Point<Pixels> {
    let pivot = visual.debug_bounds("shape-1").unwrap().center();
    let p = visual.debug_bounds(name).unwrap().center();
    crate::scene::rotation::pixels(p, pivot, 25.)
}

#[gpui::test]
fn double_click_subdivides_rotated_curve_without_changing_it_and_undo_restores_node_selection(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let before = window
        .update(cx, |this, _, _| this.shapes[0].clone())
        .unwrap();
    let old = displayed(&before);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = screen(window, &mut visual, curve(old[0], old[1], 0.35));
    visual.simulate_event(gpui::MouseDownEvent {
        position: p,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    draw(&mut visual);
    let inserted = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            let nodes = displayed(shape);
            assert_eq!(nodes.len(), 4);
            assert_eq!(this.active_node(), Some((1, 1)));
            for t in [0., 0.2, 0.5, 0.8, 1.] {
                near(
                    curve(nodes[0], nodes[1], t),
                    curve(old[0], old[1], t * 0.35),
                );
                near(
                    curve(nodes[1], nodes[2], t),
                    curve(old[0], old[1], 0.35 + t * 0.65),
                );
            }
            near(nodes[3].anchor, old[2].anchor);
            assert_eq!(this.history.borrow().undo_len(), 1);
            shape.clone()
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before);
            assert_eq!(this.active_node(), None);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z delete");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 3);
            assert_eq!(this.active_node(), Some((1, 1)));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], inserted)
        })
        .unwrap();
    visual.simulate_keystrokes("shift-right");
    window
        .update(&mut visual.cx, |this, _, _| {
            let now = displayed(&this.shapes[0]);
            let old = displayed(&inserted);
            near(now[1].anchor, old[1].anchor + point(10., 0.));
            near(now[0].anchor, old[0].anchor);
        })
        .unwrap();
}

#[gpui::test]
fn smooth_handles_link_at_constant_opposite_length_corner_conversion_and_cancel_preserve_history(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = knob(&mut visual, "bezier-node-1-0");
    visual.simulate_click(p, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.active_node(), Some((1, 1)));
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    click(&mut visual, "node-smooth");
    let before = window
        .update(&mut visual.cx, |this, _, _| this.shapes[0].clone())
        .unwrap();
    let p = knob(&mut visual, "bezier-node-1-2");
    let end = p + point(px(50.), px(35.));
    visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = displayed(&this.shapes[0])[1];
            let old = displayed(&before)[1];
            let a = n.incoming - n.anchor;
            let b = n.outgoing - n.anchor;
            assert!((a.x * b.y - a.y * b.x).abs() < 0.03);
            assert!(a.x * b.x + a.y * b.y < 0.);
            assert!(
                (a.x.hypot(a.y)
                    - (old.incoming - old.anchor)
                        .x
                        .hypot((old.incoming - old.anchor).y))
                .abs()
                    < 0.01
            );
            near(n.anchor, old.anchor);
            assert!(n.smooth);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    let p = knob(&mut visual, "bezier-node-1-2");
    visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(p, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0], before);
            assert_eq!(this.history.borrow().undo_len(), 1);
            assert!(this.history.borrow().can_redo());
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    click(&mut visual, "node-corner");
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = this.shapes[0].world_nodes()[1];
            assert_eq!(n, Node::corner(n.anchor));
        })
        .unwrap();
}

#[gpui::test]
fn closed_segment_insertion_menu_actions_minimum_nodes_and_hidden_layer_are_safe(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    click(&mut visual, "path-closed");
    let before = window
        .update(&mut visual.cx, |this, _, _| this.shapes[0].clone())
        .unwrap();
    let old = displayed(&before);
    let p = screen(window, &mut visual, curve(old[2], old[0], 0.5));
    window
        .update(&mut visual.cx, |this, _, cx| this.insert_bezier_at(p, cx))
        .unwrap();
    window
        .update(&mut visual.cx, |this, _, _| {
            let nodes = displayed(&this.shapes[0]);
            assert_eq!(nodes.len(), 4);
            assert_eq!(this.active_node(), Some((1, 3)));
            for t in [0.1, 0.5, 0.9] {
                near(curve(nodes[2], nodes[3], t), curve(old[2], old[0], t * 0.5));
                near(
                    curve(nodes[3], nodes[0], t),
                    curve(old[2], old[0], 0.5 + t * 0.5),
                );
            }
        })
        .unwrap();
    draw(&mut visual);
    let p = knob(&mut visual, "bezier-node-3-0");
    visual.simulate_mouse_down(p, MouseButton::Right, Default::default());
    visual.simulate_mouse_up(p, MouseButton::Right, Default::default());
    draw(&mut visual);
    click(&mut visual, "context-nodes");
    click(&mut visual, "context-path-closed");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes[0].closed);
            assert!(this.shapes[0].fill_enabled);
        })
        .unwrap();
    visual.simulate_keystrokes("delete delete");
    let depth = window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 2);
            this.history.borrow().undo_len()
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes.len(), 1);
            assert_eq!(this.history.borrow().undo_len(), depth);
            this.shapes[0].layer.hidden = true;
            this.run_node_action(NodeAction::ToggleClosed, cx);
            assert!(!this.shapes[0].closed);
            assert_eq!(this.history.borrow().undo_len(), depth);
            this.shapes[0].layer.hidden = false;
        })
        .unwrap();
    visual.simulate_keystrokes("escape delete");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.shapes.is_empty()))
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 2)
        })
        .unwrap();
}
