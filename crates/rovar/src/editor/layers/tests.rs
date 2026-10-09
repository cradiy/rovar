use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn layers_select_children_fold_without_changing_selection_and_delete_with_undo(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("标题");
    create(&mut visual, "add-rectangle");
    click(&mut visual, "toggle-layers");
    let board = visual.debug_bounds("layer-1").unwrap();
    let text = visual.debug_bounds("layer-2").unwrap();
    let shape = visual.debug_bounds("layer-3").unwrap();
    assert!(board.top() < shape.top() && shape.top() < text.top());
    let panel = visual.debug_bounds("layers-panel").unwrap();
    assert_eq!(board.right(), panel.right() - px(9.));
    assert_eq!(text.size.width, board.size.width);
    assert_eq!(shape.size.width, board.size.width);
    visual.simulate_click(
        point(text.right() - px(64.), text.center().y),
        Default::default(),
    );
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.selected_text, Some(2));
            assert_eq!(this.selected, Some(1));
            assert_eq!(this.selected_shape, None);
            assert!(!this.texts[0].editor.read(cx).editing);
            assert!(this.focus.is_focused(window));
        })
        .unwrap();
    click(&mut visual, "fold-layer-1");
    assert!(visual.debug_bounds("layer-2").is_none());
    assert!(visual.debug_bounds("layer-3").is_none());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_text, Some(2))
        })
        .unwrap();
    click(&mut visual, "fold-layer-1");
    click(&mut visual, "layer-3");
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-3").is_none());
    assert!(visual.debug_bounds("layer-2").is_some());
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-3").is_some());
    click(&mut visual, "layer-1");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected, Some(1));
            assert_eq!(this.selected_text, None);
            assert_eq!(this.selected_shape, None);
        })
        .unwrap();
}

#[gpui::test]
fn floating_sidebar_blocks_drawing_and_scroll_and_resources_remain_empty(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let window = cx.open_window(size(px(840.), px(520.)), Workspace::new);
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "add-rectangle");
    let panel = visual.debug_bounds("layers-panel").unwrap();
    let editor = visual.debug_bounds("editor-area").unwrap();
    let toolbar = visual.debug_bounds("tool-bar").unwrap();
    assert!(panel.left() > editor.left() && panel.right() < editor.right());
    let properties = visual.debug_bounds("properties-panel").unwrap();
    assert_eq!(panel.bottom(), toolbar.bottom());
    assert_eq!(properties.top(), panel.top());
    assert_eq!(properties.bottom(), panel.bottom());
    assert_eq!(toolbar.center().x, editor.center().x);
    let from = panel.center();
    let to = from + point(px(45.), px(30.));
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Default::default());
    let pan = window
        .update(&mut visual.cx, |this, _, _| this.view.pan)
        .unwrap();
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: from,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-60.))),
        ..Default::default()
    });
    click(&mut visual, "resources-tab");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.boards.is_empty() && this.texts.is_empty() && this.shapes.is_empty());
            assert!(this.sidebar.resources);
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Rectangle)));
            assert_eq!(this.view.pan, pan);
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    let collapsed = visual.debug_bounds("layers-panel").unwrap();
    assert!(collapsed.size.width < px(60.));
    click(&mut visual, "toggle-layers");
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.sidebar.resources))
        .unwrap();
    click(&mut visual, "layers-tab");
    // Drawing in the uncovered canvas still works while the panel is open.
    let from = point(panel.right() + px(30.), panel.top() + px(90.));
    let to = from + point(px(100.), px(80.));
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(to, MouseButton::Left, Default::default());
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-1").is_some());
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1);
            assert_eq!(this.shapes[0].board, None);
        })
        .unwrap();
}

#[gpui::test]
fn locking_exits_text_editing_and_blocks_canvas_layer_and_keyboard_edits(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("保留文字");
    click(&mut visual, "toggle-layers");
    let position = visual.debug_bounds("text-box-2").unwrap().center();
    click(&mut visual, "layer-lock-2");
    click(&mut visual, "layer-2");
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        modifiers: Default::default(),
        first_mouse: false,
    });
    visual.simulate_mouse_up(position, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("a backspace delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.texts[0].layer.locked);
            assert_eq!(this.texts[0].editor.read(cx).content, "保留文字");
            assert!(!this.texts[0].editor.read(cx).editing);
            assert!(this.selected_text.is_none());
            assert!(this.selected.is_none());
        })
        .unwrap();
    // Undo restores editability; redo must clear selection again.
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    click(&mut visual, "layer-2");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_text, Some(2))
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.texts[0].layer.locked);
            assert!(this.selected_text.is_none());
        })
        .unwrap();
    click(&mut visual, "layer-lock-2");
    click(&mut visual, "toggle-layers");
    create(&mut visual, "add-rectangle");
    click(&mut visual, "toggle-layers");
    let shape_bounds = visual.debug_bounds("shape-3").unwrap();
    let before = window
        .update(&mut visual.cx, |this, _, _| this.shapes[0].rect)
        .unwrap();
    click(&mut visual, "layer-lock-1");
    click(&mut visual, "layer-3");
    let start = shape_bounds.center();
    let end = start + point(px(45.), px(30.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes[0].rect, before);
            assert!(!this.layer_editable(1) && !this.layer_editable(2) && !this.layer_editable(3));
            assert!(!this.shapes[0].layer.locked && !this.texts[0].layer.locked);
            assert!(this.selected_shape.is_none());
            assert!(this.gesture.is_none());
        })
        .unwrap();
    click(&mut visual, "layer-lock-1");
    click(&mut visual, "layer-3");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.selected_shape, Some(3))
        })
        .unwrap();
}

#[gpui::test]
fn hiding_boards_preserves_child_flags_and_overlapping_root_layers_through_history(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle"); // Root layer remains independent of the later board.
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("独立隐藏状态");
    create(&mut visual, "add-ellipse");
    click(&mut visual, "toggle-layers");
    click(&mut visual, "layer-lock-3");
    click(&mut visual, "layer-4");
    click(&mut visual, "layer-visibility-3");
    click(&mut visual, "layer-visibility-2");
    assert!(visual.debug_bounds("artboard-2").is_none());
    assert!(visual.debug_bounds("text-box-3").is_none());
    assert!(visual.debug_bounds("shape-4").is_none());
    assert!(visual.debug_bounds("shape-1").is_some());
    assert!(visual.debug_bounds("layer-4").is_some());
    click(&mut visual, "layer-4");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.selected_shape.is_none());
            assert!(this.selected.is_none());
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-4").is_some());
    assert!(visual.debug_bounds("text-box-3").is_none());
    visual.simulate_keystrokes("secondary-shift-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-4").is_none());
    click(&mut visual, "layer-visibility-2");
    assert!(visual.debug_bounds("shape-4").is_some());
    assert!(visual.debug_bounds("text-box-3").is_none());
    // Deleting and restoring the board also restores each child's own metadata.
    click(&mut visual, "layer-2");
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    assert!(visual.debug_bounds("shape-1").is_some());
    assert!(visual.debug_bounds("layer-3").is_none());
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    assert!(visual.debug_bounds("layer-3").is_some());
    assert!(visual.debug_bounds("text-box-3").is_none());
    click(&mut visual, "layer-visibility-3");
    assert!(visual.debug_bounds("text-box-3").is_some());
    window
        .update(&mut visual.cx, |this, _, cx| {
            let text = &this.texts[0];
            assert!(text.layer.locked && !text.layer.hidden);
            assert_eq!(text.editor.read(cx).content, "独立隐藏状态");
            assert_eq!(text.board, Some(2));
            assert_eq!(this.shapes[0].board, None);
        })
        .unwrap();
}
