use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};

fn screen(
    window: WindowHandle<Workspace>,
    visual: &mut VisualTestContext,
    local: Point<f32>,
) -> Point<Pixels> {
    window
        .update(&mut visual.cx, |this, _, _| {
            let board = &this.boards[0];
            let p = this
                .view
                .screen(point(board.rect.x + local.x, board.rect.y + local.y));
            this.bounds.get().origin + point(px(p.x), px(p.y))
        })
        .unwrap()
}
fn drag(visual: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>, shift: bool) {
    let modifiers = Modifiers {
        shift,
        ..Default::default()
    };
    visual.simulate_mouse_down(from, MouseButton::Left, modifiers);
    visual.simulate_mouse_move(to, MouseButton::Left, modifiers);
    visual.simulate_mouse_up(to, MouseButton::Left, modifiers);
    draw(visual);
}

#[gpui::test]
fn line_draw_endpoint_crossing_and_properties_use_board_coordinates_and_one_step_history(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "draw-line");
    assert!(visual.debug_bounds("drawing-surface").is_some());
    create(&mut visual, "add-artboard");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom_at(point(300., 200.), 0.5);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    let a = screen(window, &mut visual, point(20., 40.));
    let b = screen(window, &mut visual, point(220., 40.));
    click(&mut visual, "draw-line");
    drag(&mut visual, a, b, false);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].kind, ShapeKind::Line);
            assert_eq!(this.shapes[0].path_point(0), point(20., 40.));
            assert_eq!(this.shapes[0].path_point(1), point(220., 40.));
            assert_eq!(this.shapes[0].rect.height, 0.);
            assert!(this.inspector.stroke_editing);
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    assert!(visual.debug_bounds("shape-fill").is_none());
    assert!(visual.debug_bounds("stroke-outside").is_none());
    let handle = visual.debug_bounds("line-end-1").unwrap().center();
    assert_eq!(handle, b);
    let crossed = screen(window, &mut visual, point(-80., 80.));
    drag(&mut visual, handle, crossed, false);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].path_point(0), point(20., 40.));
            assert_eq!(this.shapes[0].path_point(1), point(-80., 80.));
            assert_eq!(this.history.borrow().undo_len(), 3);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    assert_eq!(visual.debug_bounds("line-end-1").unwrap().center(), b);
    visual.simulate_mouse_down(b, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(crossed, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(crossed, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert_eq!(visual.debug_bounds("line-end-1").unwrap().center(), b);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    assert_eq!(visual.debug_bounds("line-end-1").unwrap().center(), crossed);
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a - 1 2 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].path_point(1), point(-120., 80.))
        })
        .unwrap();
    click(&mut visual, "fill-linear");
    click(&mut visual, "gradient-add");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].stroke.fill_mode, FillMode::Linear);
            assert_eq!(this.shapes[0].stroke.gradient.stops().len(), 3);
            assert!(!this.shapes[0].fill_enabled);
        })
        .unwrap();
    // Direction constraints work in world coordinates after zoom.
    click(&mut visual, "draw-line");
    let target = screen(window, &mut visual, point(20., 200.));
    drag(&mut visual, a, target, true);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[1].rect.width, 0.);
            assert_eq!(this.shapes[1].path_point(1), point(20., 200.));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1)
        })
        .unwrap();
}

#[gpui::test]
fn diagonal_lines_select_by_bounds_and_respect_stacking(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    for (from, to) in [
        (point(80., 80.), point(280., 280.)),
        (point(80., 280.), point(280., 80.)),
    ] {
        let from = screen(window, &mut visual, from);
        let to = screen(window, &mut visual, to);
        click(&mut visual, "draw-line");
        drag(&mut visual, from, to, false);
    }
    let outside = screen(window, &mut visual, point(320., 180.));
    visual.simulate_click(outside, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, None);
        })
        .unwrap();
    // The top line's empty rectangle overlaps the lower line's actual stroke.
    let overlap = screen(window, &mut visual, point(110., 110.));
    visual.simulate_click(overlap, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(3));
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    // With the top line removed, empty space inside the lower line still selects it.
    let empty = screen(window, &mut visual, point(110., 240.));
    visual.simulate_click(empty, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2));
        })
        .unwrap();
}

#[gpui::test]
fn pen_keeps_bends_and_moves_from_bounding_box_with_shared_geometry(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let a = screen(window, &mut visual, point(80., 80.));
    let b = screen(window, &mut visual, point(180., 180.));
    let c = screen(window, &mut visual, point(280., 80.));
    click(&mut visual, "draw-pen");
    visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(b, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(c, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(c, MouseButton::Left, Default::default());
    draw(&mut visual);
    let (points, geometry) = window
        .update(&mut visual.cx, |this, _, _| {
            let shape = &this.shapes[0];
            assert_eq!(shape.kind, ShapeKind::Pen);
            assert_eq!(shape.points.0.len(), 3);
            assert_eq!(shape.path_point(1), point(180., 180.));
            assert_eq!(this.history.borrow().undo_len(), 2);
            (
                shape.points.0.clone(),
                this.shape_paths.borrow()[&2].clone(),
            )
        })
        .unwrap();
    // Empty space inside the bounding box selects and drags the whole path.
    let empty = screen(window, &mut visual, point(180., 90.));
    visual.simulate_click(empty, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2))
        })
        .unwrap();
    drag(&mut visual, empty, empty + point(px(20.), px(10.)), false);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(2));
            assert!(Rc::ptr_eq(&points, &this.shapes[0].points.0));
            assert!(Rc::ptr_eq(&geometry, &this.shape_paths.borrow()[&2]));
            assert_eq!(this.shapes[0].stroke.width, 3.);
            assert_eq!(this.shapes[0].path_point(0), point(100., 90.));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    // A cancelled draft must leave the redo branch and IDs intact.
    click(&mut visual, "draw-pen");
    visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(c, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(c, MouseButton::Left, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1);
            assert_eq!(this.next_id, 3);
            assert!(this.draft.is_none());
            assert!(!this.shape_paths.borrow().contains_key(&0));
            assert!(this.shapes[0].rect.x > 80.);
        })
        .unwrap();
    click(&mut visual, "draw-pen");
    visual.simulate_click(a, Default::default());
    draw(&mut visual);
    visual.simulate_keystrokes("escape");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1)
        })
        .unwrap();
    // Board deletion restores both the path points and its style in one step.
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.select(Some(1), cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].points.0, points);
            assert_eq!(this.shapes[0].stroke.width, 3.);
        })
        .unwrap();
}
