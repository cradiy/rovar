use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use std::sync::Arc;

fn screen(
    window: WindowHandle<Workspace>,
    visual: &mut VisualTestContext,
    p: Point<f32>,
) -> Point<Pixels> {
    window
        .update(&mut visual.cx, |this, _, _| {
            let b = &this.boards[0];
            let p = this.view.screen(point(b.rect.x + p.x, b.rect.y + p.y));
            this.bounds.get().origin + point(px(p.x), px(p.y))
        })
        .unwrap()
}
fn drag(visual: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(visual);
}
fn near(a: Point<f32>, b: Point<f32>) {
    assert!(
        (a.x - b.x).abs() < 0.001 && (a.y - b.y).abs() < 0.001,
        "{a:?} != {b:?}"
    );
}

#[gpui::test]
fn bezier_creation_and_independent_handles_remain_editable_outside_curve_bounds(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let a = screen(window, &mut visual, point(80., 80.));
    let handle = screen(window, &mut visual, point(80., 180.));
    let b = screen(window, &mut visual, point(280., 80.));
    click(&mut visual, "draw-bezier");
    drag(&mut visual, a, handle);
    visual.simulate_click(b, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.is_empty());
            assert_eq!(this.bezier_draft.as_ref().unwrap().nodes.len(), 2);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    assert_eq!(
        visual.debug_bounds("bezier-guides-0").unwrap(),
        visual.debug_bounds("shape-0").unwrap(),
        "draft control guides must share the path's coordinate origin",
    );
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            assert_eq!(shape.kind, ShapeKind::Bezier);
            assert!(!shape.closed);
            near(shape.world_nodes()[0].outgoing, point(80., 180.));
            // An unused incoming handle must not enlarge the geometric bounds.
            assert!((shape.rect.y - 80.).abs() < 0.001);
            assert!((shape.rect.height - 44.44444).abs() < 0.01);
            assert_eq!(this.history.borrow().undo_len(), 2);
            assert!(this.bezier_draft.is_none() && this.draw_tool.is_none());
        })
        .unwrap();
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.enter_vector_edit(2, window, cx)
        })
        .unwrap();
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("bezier-guides-2").unwrap(),
        visual.debug_bounds("shape-2").unwrap(),
        "committed control guides must share the path's coordinate origin",
    );
    let knob = visual.debug_bounds("bezier-node-0-2").unwrap().center();
    let target = screen(window, &mut visual, point(100., 220.));
    drag(&mut visual, knob, target);
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = this.shapes[0].world_nodes()[0];
            near(n.outgoing, point(100., 220.));
            near(n.incoming, point(80., -20.));
            assert_eq!(this.history.borrow().undo_len(), 3);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    let knob = visual.debug_bounds("bezier-node-0-2").unwrap().center();
    visual.simulate_mouse_down(knob, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(target, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(target, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(this.shapes[0].world_nodes()[0].outgoing, point(100., 220.))
        })
        .unwrap();
    let anchor = visual.debug_bounds("bezier-node-1-0").unwrap().center();
    let target = screen(window, &mut visual, point(320., 120.));
    drag(&mut visual, anchor, target);
    window
        .update(&mut visual.cx, |this, _, _| {
            let n = this.shapes[0].world_nodes()[1];
            near(n.anchor, point(320., 120.));
            near(n.incoming, n.anchor);
            near(n.outgoing, n.anchor);
        })
        .unwrap();
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(300., 200.), 0.5);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("bezier-guides-2").unwrap(),
        visual.debug_bounds("shape-2").unwrap(),
        "control guides stay aligned after node edits and zoom",
    );
}

#[gpui::test]
fn closing_bezier_preserves_nodes_and_fill_with_bounding_box_selection(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1400.)));
    create(&mut visual, "add-artboard");
    click(&mut visual, "draw-bezier");
    for p in [
        point(80., 80.),
        point(280., 80.),
        point(80., 280.),
        point(80., 80.),
    ] {
        let p = screen(window, &mut visual, p);
        visual.simulate_click(p, Default::default());
        draw(&mut visual);
    }
    window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            assert!(shape.closed);
            assert_eq!(shape.nodes.0.len(), 3);
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    click(&mut visual, "fill-visibility");
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.inspector.stroke_editing);
            assert_eq!(this.shapes[0].gradient.stops().len(), 2);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes[0].fill_enabled);
            assert_eq!(this.shapes[0].gradient.stops().len(), 3);
            assert_eq!(this.shapes[0].stroke.fill_mode, FillMode::Solid);
        })
        .unwrap();
    let empty = screen(window, &mut visual, point(240., 240.));
    visual.simulate_click(empty, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2))
        })
        .unwrap();
    let inside = screen(window, &mut visual, point(120., 120.));
    visual.simulate_click(inside, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2))
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes[0].closed && this.shapes[0].fill_enabled);
            assert_eq!(this.shapes[0].gradient.stops().len(), 3);
            assert_eq!(this.shapes[0].nodes.0.len(), 3);
        })
        .unwrap();
}

#[gpui::test]
fn draft_node_history_and_escape_leave_document_history_and_freehand_available(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-rectangle");
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    click(&mut visual, "draw-bezier");
    for p in [point(80., 80.), point(280., 80.)] {
        let p = screen(window, &mut visual, p);
        visual.simulate_click(p, Default::default());
        draw(&mut visual);
    }
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.bezier_draft.as_ref().unwrap().nodes.len(), 1);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.bezier_draft.as_ref().unwrap().nodes.len(), 2)
        })
        .unwrap();
    visual.simulate_keystrokes("backspace backspace");
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-node-0-0").is_none());
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.bezier_draft.is_none());
            assert_eq!(this.shapes[0].kind, ShapeKind::Rectangle);
            assert_eq!(this.boards.len(), 1);
        })
        .unwrap();
    click(&mut visual, "draw-bezier");
    for p in [point(80., 80.), point(280., 80.)] {
        let p = screen(window, &mut visual, p);
        visual.simulate_click(p, Default::default());
        draw(&mut visual);
    }
    click(&mut visual, "draw-pen");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].kind, ShapeKind::Bezier);
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Pen)));
            assert!(this.bezier_draft.is_none());
        })
        .unwrap();
    let a = screen(window, &mut visual, point(100., 200.));
    let b = screen(window, &mut visual, point(200., 250.));
    drag(&mut visual, a, b);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[2].kind, ShapeKind::Pen)
        })
        .unwrap();
}

#[gpui::test]
fn hover_previews_next_curve_without_committing_and_tracks_closure_zoom_and_undo(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(300., 200.), 0.5);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    let a = screen(window, &mut visual, point(80., 80.));
    let handle = screen(window, &mut visual, point(80., 180.));
    let b = screen(window, &mut visual, point(280., 100.));
    click(&mut visual, "draw-bezier");
    drag(&mut visual, a, handle);
    let (nodes, mesh) = window
        .update(&mut visual.cx, |this, _, _| {
            (
                this.bezier_draft.as_ref().unwrap().shape.nodes.0.clone(),
                this.shape_paths.borrow()[&0].clone(),
            )
        })
        .unwrap();
    visual.simulate_mouse_move(b, None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_some());
    window
        .update(&mut visual.cx, |this, _, _| {
            let [from, to] = this.bezier_preview_nodes().unwrap();
            near(from.anchor, point(80., 80.));
            near(from.outgoing, point(80., 180.));
            near(to.anchor, point(280., 100.));
            near(to.incoming, to.anchor);
            assert!(Arc::ptr_eq(
                &nodes,
                &this.bezier_draft.as_ref().unwrap().shape.nodes.0
            ));
            assert!(Rc::ptr_eq(&mesh, &this.shape_paths.borrow()[&0]));
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_mouse_move(point(px(-10.), px(-10.)), None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_none());
    visual.simulate_mouse_move(b, None, Default::default());
    visual.simulate_mouse_down(b, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_none());
    visual.simulate_mouse_up(b, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_mouse_move(a + point(px(3.), px(0.)), None, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let [from, to] = this.bezier_preview_nodes().unwrap();
            near(from.anchor, point(280., 100.));
            near(to.anchor, point(80., 80.));
            near(to.incoming, point(80., -20.));
        })
        .unwrap();
    let cursor = screen(window, &mut visual, point(400., 200.));
    visual.simulate_mouse_move(cursor, None, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            near(
                this.bezier_preview_nodes().unwrap()[0].anchor,
                point(80., 80.),
            )
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(300., 200.), 1.);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            let end = this.bezier_preview_nodes().unwrap()[1].anchor;
            let board = &this.boards[0];
            let p = this
                .view
                .screen(point(board.rect.x + end.x, board.rect.y + end.y));
            let p = this.bounds.get().origin + point(px(p.x), px(p.y));
            assert_eq!(p, cursor);
            assert_eq!(this.bezier_draft.as_ref().unwrap().nodes.len(), 2);
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].nodes.0.len(), 2);
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    click(&mut visual, "draw-bezier");
    let a = screen(window, &mut visual, point(100., 100.));
    visual.simulate_click(a, Default::default());
    visual.simulate_mouse_move(a + point(px(80.), px(20.)), None, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_some());
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("bezier-hover-preview").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1)
        })
        .unwrap();
}
