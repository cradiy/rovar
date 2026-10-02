use super::*;
use crate::editor::{
    layout::tests::fixture,
    tests::{click, draw},
};
use gpui::{TestAppContext, VisualTestContext};

fn right(visual: &mut VisualTestContext, position: Point<Pixels>) {
    visual.simulate_mouse_down(position, MouseButton::Right, Default::default());
    visual.simulate_mouse_up(position, MouseButton::Right, Default::default());
    draw(visual);
}
#[gpui::test]
fn context_preserves_multiselection_and_runs_glass_submenu_actions(cx: &mut TestAppContext) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let p = visual.debug_bounds("shape-2").unwrap().center();
    right(&mut visual, p);
    assert!(visual.debug_bounds("editor-context-glass-0").is_some());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selection_ids().len(), 3)
        })
        .unwrap();
    click(&mut visual, "context-layout");
    assert!(visual.debug_bounds("editor-context-glass-1").is_some());
    click(&mut visual, "distribute-x");
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.shapes[1].rect.x, 130.);
            assert!(!context_menu::is_open(cx));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    right(&mut visual, p);
    click(&mut visual, "context-duplicate");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 6);
            assert_eq!(this.selection_ids().len(), 3);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 3)
        })
        .unwrap();
}
#[gpui::test]
fn locked_hidden_tree_targets_can_be_restored_and_menu_dismissal_never_draws(
    cx: &mut TestAppContext,
) {
    let window = fixture(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.set_selection(BTreeSet::from([1]), cx)
        })
        .unwrap();
    draw(&mut visual);
    let p = visual.debug_bounds("shape-1").unwrap().center();
    right(&mut visual, p);
    click(&mut visual, "context-lock");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.shapes[0].layer.locked);
            assert!(this.selection_ids().is_empty());
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    let row = visual.debug_bounds("layer-1").unwrap().center();
    right(&mut visual, row);
    click(&mut visual, "context-lock");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes[0].layer.locked)
        })
        .unwrap();
    right(&mut visual, row);
    click(&mut visual, "context-hide");
    right(&mut visual, row);
    click(&mut visual, "context-hide");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(!this.shapes[0].layer.hidden);
            this.choose_tool(toolbar::Tool::Rectangle, window, cx);
        })
        .unwrap();
    draw(&mut visual);
    let edge = point(px(1270.), px(790.));
    // Open directly at a window edge to exercise placement independent of the inspector overlay.
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.open_context_menu(None, false, edge, window, cx)
        })
        .unwrap();
    draw(&mut visual);
    let bounds = visual.debug_bounds("editor-context-glass-0").unwrap();
    assert!(bounds.right() <= px(1272.) && bounds.bottom() <= px(792.));
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(!context_menu::is_open(cx));
            assert_eq!(this.shapes.len(), 3);
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Rectangle)));
        })
        .unwrap();
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.open_context_menu(None, false, point(px(600.), px(400.)), window, cx)
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_click(point(px(700.), px(160.)), Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(!context_menu::is_open(cx));
            assert!(this.box_draft.is_none());
            assert!(this.gesture.is_none());
        })
        .unwrap();
}
