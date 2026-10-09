use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

fn screen(visual: &mut VisualTestContext, p: Point<f32>) -> Point<Pixels> {
    visual.update(|window, cx| {
        let root = window.root::<Workspace>().unwrap().unwrap();
        let this = root.read(cx);
        let p = this.view.screen(p);
        this.bounds.get().origin + point(px(p.x), px(p.y))
    })
}
fn drag(visual: &mut VisualTestContext, from: Point<f32>, to: Point<f32>, shift: bool) {
    let from = screen(visual, from);
    let to = screen(visual, to);
    let modifiers = Modifiers {
        shift,
        ..Default::default()
    };
    visual.simulate_mouse_down(from, MouseButton::Left, modifiers);
    visual.simulate_mouse_move(to, MouseButton::Left, modifiers);
    visual.simulate_mouse_up(to, MouseButton::Left, modifiers);
    draw(visual);
}
fn make(visual: &mut VisualTestContext, tool: &'static str, from: Point<f32>, to: Point<f32>) {
    click(visual, tool);
    drag(visual, from, to, false);
}

#[gpui::test]
fn tools_only_activate_and_all_components_can_be_drawn_without_boards(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for tool in [
        "add-artboard",
        "add-text",
        "add-rectangle",
        "add-ellipse",
        "draw-line",
        "draw-pen",
        "draw-bezier",
    ] {
        click(&mut visual, tool);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert!(this.boards.is_empty() && this.texts.is_empty() && this.shapes.is_empty());
                assert_eq!(this.history.borrow().undo_len(), 0);
                assert_eq!(this.next_id, 1);
            })
            .unwrap();
    }
    for (i, (tool, kind)) in [
        ("add-rectangle", ShapeKind::Rectangle),
        ("add-ellipse", ShapeKind::Ellipse),
        ("draw-line", ShapeKind::Line),
        ("draw-pen", ShapeKind::Pen),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 80. + i as f32 * 150.;
        make(&mut visual, tool, point(x, 80.), point(x + 80., 140.));
        window
            .update(&mut visual.cx, |this, _, _| {
                let shape = this.shapes.last().unwrap();
                assert_eq!(shape.kind, kind);
                assert_eq!(shape.board, None);
                assert_eq!(
                    shape.rect,
                    Rect {
                        x,
                        y: 80.,
                        width: 80.,
                        height: 60.
                    }
                );
                assert!(this.draw_tool.is_none());
            })
            .unwrap();
        assert!(visual.debug_bounds("property-3").is_some());
    }
    click(&mut visual, "draw-bezier");
    for p in [point(100., 220.), point(220., 270.), point(300., 220.)] {
        let p = screen(&mut visual, p);
        visual.simulate_click(p, Default::default());
        draw(&mut visual);
    }
    assert!(visual.debug_bounds("bezier-hover-preview").is_some());
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    make(
        &mut visual,
        "add-text",
        point(380., 220.),
        point(600., 320.),
    );
    visual.simulate_input("独立文本");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.boards.is_empty());
            assert_eq!(this.shapes.len(), 5);
            assert_eq!(this.shapes[4].board, None);
            assert_eq!(this.texts[0].board, None);
            assert_eq!(this.texts[0].rect.width, 220.);
            assert_eq!(this.texts[0].editor.read(cx).content, "独立文本");
            assert!(this.texts[0].editor.read(cx).focus.is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn reverse_drag_zoom_square_cancel_and_creation_history(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.view.zoom = 0.5;
            this.view.pan = point(100., 60.);
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    for tool in ["add-artboard", "add-text", "add-rectangle", "add-ellipse"] {
        click(&mut visual, tool);
        let a = screen(&mut visual, point(80., 80.));
        let b = screen(&mut visual, point(300., 200.));
        visual.simulate_click(a, Default::default());
        draw(&mut visual);
        visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
        visual.simulate_mouse_move(b, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert!(this.boards.is_empty() && this.texts.is_empty() && this.shapes.is_empty());
                assert_eq!(this.history.borrow().undo_len(), 0);
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(b, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert!(this.box_draft.is_none() && this.draft.is_none());
                assert_eq!(this.next_id, 1);
            })
            .unwrap();
    }
    click(&mut visual, "add-rectangle");
    drag(&mut visual, point(300., 220.), point(100., 140.), true);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.shapes[0].rect,
                Rect {
                    x: 100.,
                    y: 20.,
                    width: 200.,
                    height: 200.
                }
            );
            assert_eq!(this.history.borrow().undo_len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    // A cancelled creation must preserve the redo branch and next object ID.
    click(&mut visual, "add-artboard");
    let a = screen(&mut visual, point(80., 40.));
    visual.simulate_mouse_down(a, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(a, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1);
            assert!(this.boards.is_empty());
            assert_eq!(this.next_id, 2);
        })
        .unwrap();
    let before = window
        .update(&mut visual.cx, |this, _, _| (this.view.zoom, this.view.pan))
        .unwrap();
    make(
        &mut visual,
        "add-artboard",
        point(600., 380.),
        point(350., 80.),
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(
                this.boards[0].rect,
                Rect {
                    x: 350.,
                    y: 80.,
                    width: 250.,
                    height: 300.
                }
            );
            assert_eq!((this.view.zoom, this.view.pan), before);
        })
        .unwrap();
}

#[gpui::test]
fn moving_components_attaches_detaches_and_undo_restores_parent_and_position(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    make(
        &mut visual,
        "add-artboard",
        point(230., 230.),
        point(30., 30.),
    );
    make(
        &mut visual,
        "add-artboard",
        point(320., 30.),
        point(520., 230.),
    );
    make(
        &mut visual,
        "add-rectangle",
        point(620., 60.),
        point(700., 120.),
    );
    make(
        &mut visual,
        "add-text",
        point(620., 180.),
        point(740., 240.),
    );
    visual.simulate_input("保留样式和内容");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    // Shape: root -> first board -> second board -> root. Each drop is one step.
    let mut center = point(660., 90.);
    for (target, parent) in [
        (point(130., 100.), Some(1)),
        (point(420., 100.), Some(2)),
        (point(660., 90.), None),
    ] {
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                (this.shapes[0].clone(), this.history.borrow().undo_len())
            })
            .unwrap();
        drag(&mut visual, center, target, false);
        let after = window
            .update(&mut visual.cx, |this, _, _| {
                let shape = &this.shapes[0];
                assert_eq!(shape.board, parent);
                let p = this.parent_origin(parent) + point(shape.rect.x + 40., shape.rect.y + 30.);
                assert_eq!(p, target);
                assert_eq!(this.history.borrow().undo_len(), before.1 + 1);
                shape.clone()
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0], before.0)
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-shift-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0], after)
            })
            .unwrap();
        center = target;
    }
    // Text: root -> board -> root -> board, with matching ownership on undo/redo.
    let mut center = point(680., 210.);
    for (target, parent) in [
        (point(420., 170.), Some(2)),
        (point(680., 210.), None),
        (point(420., 170.), Some(2)),
    ] {
        let before = window
            .update(&mut visual.cx, |this, _, _| {
                (this.texts[0].board, this.texts[0].rect)
            })
            .unwrap();
        drag(&mut visual, center, target, false);
        window
            .update(&mut visual.cx, |this, _, cx| {
                let text = &this.texts[0];
                assert_eq!(text.board, parent);
                assert_eq!(
                    this.parent_origin(parent) + point(text.rect.x + 60., text.rect.y + 30.),
                    target
                );
                assert_eq!(text.editor.read(cx).content, "保留样式和内容");
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!((this.texts[0].board, this.texts[0].rect), before)
            })
            .unwrap();
        visual.simulate_keystrokes("secondary-shift-z");
        draw(&mut visual);
        center = target;
    }
    // Deleting a board affects current children only, never detached objects.
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.select(Some(2), cx);
            this.focus.focus(window, cx);
        })
        .unwrap();
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 1);
            assert!(this.texts.is_empty());
            assert_eq!(this.shapes.len(), 1);
            assert_eq!(this.shapes[0].board, None);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards.len(), 2);
            assert_eq!(this.texts[0].board, Some(2));
            assert_eq!(this.texts[0].editor.read(cx).content, "保留样式和内容");
        })
        .unwrap();
}
