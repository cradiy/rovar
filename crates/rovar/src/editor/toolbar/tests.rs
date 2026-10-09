use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn toolbar_menus_preserve_selection_remember_tools_and_fit_small_windows(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "add-ellipse");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.shapes.is_empty()))
        .unwrap();
    create(&mut visual, "add-artboard");
    click(&mut visual, "shapes-menu");
    click(&mut visual, "option-add-ellipse");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.is_empty());
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Ellipse)));
            assert_eq!(this.toolbar.shape, Tool::Ellipse);
        })
        .unwrap();
    create(&mut visual, "add-ellipse");
    create(&mut visual, "add-ellipse");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 2);
            assert_eq!(this.selected_shape, Some(3));
        })
        .unwrap();
    visual.simulate_resize(size(px(840.), px(520.)));
    draw(&mut visual);
    let editor = visual.debug_bounds("editor-area").unwrap();
    let bar = visual.debug_bounds("tool-bar").unwrap();
    assert!(bar.left() >= editor.left() && bar.right() <= editor.right());
    assert!(bar.bottom() < editor.bottom());
    assert_eq!(bar.center().x, editor.center().x);
    for _ in 0..2 {
        click(&mut visual, "toggle-layers");
        assert_eq!(
            visual.debug_bounds("tool-bar").unwrap().center(),
            bar.center()
        );
    }
    for (group, popup) in [
        ("navigation-menu", "navigation-menu-popup"),
        ("shapes-menu", "shapes-menu-popup"),
        ("pen-menu", "pen-menu-popup"),
    ] {
        click(&mut visual, group);
        let menu = visual.debug_bounds(popup).unwrap();
        assert!(menu.bottom() < bar.top());
        assert!(menu.left() >= editor.left() && menu.right() <= editor.right());
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(this.selected_shape, Some(3));
                assert_eq!(
                    this.toolbar
                        .menus
                        .iter()
                        .filter(|m| m.read(cx).is_open())
                        .count(),
                    1
                );
            })
            .unwrap();
    }
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("pen-menu-popup").is_none());
}

#[gpui::test]
fn hand_tool_pans_over_objects_without_editing_and_shortcuts_leave_text_input_alone(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let (rect, pan, history) = window
        .update(&mut visual.cx, |this, _, _| {
            (
                this.boards[0].rect,
                this.view.pan,
                this.history.borrow().undo_len(),
            )
        })
        .unwrap();
    click(&mut visual, "tool-hand");
    let start = visual.debug_bounds("artboard-1").unwrap().center();
    let delta = point(px(30.), px(20.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(start + delta, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(start + delta, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect, rect);
            assert_eq!(this.view.pan, pan + point(30., 20.));
            assert_eq!(this.history.borrow().undo_len(), history);
        })
        .unwrap();
    visual.simulate_keystrokes("v l");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.toolbar.hand);
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Line)));
            assert!(this.draft.is_none());
        })
        .unwrap();
    click(&mut visual, "pen-menu");
    click(&mut visual, "option-draw-pen");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Pen)));
            assert!(this.draft.is_none());
            assert_eq!(this.history.borrow().undo_len(), history);
        })
        .unwrap();
    visual.simulate_keystrokes("v shift-p");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Pen)))
        })
        .unwrap();
    create(&mut visual, "add-text");
    visual.simulate_keystrokes("v h f r o l p t");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "vhfrolpt");
            assert_eq!(this.boards.len(), 1);
            assert!(this.shapes.is_empty());
            assert!(this.draw_tool.is_none());
            assert!(!this.toolbar.hand);
        })
        .unwrap();
}

#[gpui::test]
fn dismissing_tool_menu_keeps_pen_draft_and_switching_tools_commits_once(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    click(&mut visual, "draw-bezier");
    let board = visual.debug_bounds("artboard-1").unwrap();
    for offset in [point(px(80.), px(80.)), point(px(180.), px(140.))] {
        visual.simulate_click(board.origin + offset, Default::default());
        draw(&mut visual);
    }
    click(&mut visual, "shapes-menu");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, _| {
            assert!(this.bezier_draft.is_some());
            assert!(this.shapes.is_empty());
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Bezier)));
            assert_eq!(this.history.borrow().undo_len(), 1);
            assert!(this.focus.is_focused(window));
        })
        .unwrap();
    click(&mut visual, "shapes-menu");
    click(&mut visual, "shapes-menu");
    draw(&mut visual);
    assert!(visual.debug_bounds("shapes-menu-popup").is_none());
    click(&mut visual, "tool-move");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.bezier_draft.is_none());
            assert_eq!(this.shapes.len(), 1);
            assert_eq!(this.shapes[0].nodes.0.len(), 2);
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes.is_empty());
            assert_eq!(this.boards.len(), 1);
        })
        .unwrap();
}
