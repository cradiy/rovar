use super::*;
use crate::workspace::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

fn drag(visual: &mut VisualTestContext, selector: &'static str, delta: f32, cancel: bool) {
    draw(visual);
    let toolbar_center = visual.debug_bounds("tool-bar").unwrap().center();
    let start = visual.debug_bounds(selector).unwrap().center();
    let end = start + point(px(delta), px(0.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    let (side, displayed, limit) = visual.update(|window, cx| {
        let root = window.root::<Workspace>().unwrap().unwrap();
        let GestureKind::Panel {
            side,
            displayed,
            limit,
            ..
        } = root.read(cx).gesture.unwrap().kind
        else {
            panic!("missing panel gesture")
        };
        (side, displayed, limit)
    });
    for fraction in [0.25, 0.5, 1.] {
        let movement = delta * fraction;
        visual.simulate_mouse_move(
            start + point(px(movement), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(visual);
        let panel = if side == Side::Left {
            "layers-panel"
        } else {
            "properties-panel"
        };
        let expected = (displayed
            + if side == Side::Left {
                movement
            } else {
                -movement
            })
        .clamp(side.minimum(), limit);
        assert_eq!(visual.debug_bounds(panel).unwrap().size.width, px(expected));
    }
    if cancel {
        visual.simulate_keystrokes("escape");
    }
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(visual);
    assert_eq!(
        visual.debug_bounds("tool-bar").unwrap().center(),
        toolbar_center
    );
}

#[gpui::test]
fn text_commits_refresh_cached_canvas_during_resize_and_remain_editable_afterwards(
    cx: &mut TestAppContext,
) {
    use gpui::EntityInputHandler;
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-text");
    visual.simulate_input("a");
    click(&mut visual, "toggle-layers");
    let start = visual.debug_bounds("resize-left-panel").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    for dx in [10., 20.] {
        visual.simulate_mouse_move(
            start + point(px(dx), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
    }
    let before = window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0].editor.update(cx, |text, cx| {
                let before = text
                    .bounds_for_range(1..1, Bounds::default(), window, cx)
                    .unwrap();
                // Simulate an already pending IME commit arriving after the drag took focus.
                text.replace_text_in_range(None, "bc", window, cx);
                before
            })
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(this.gesture.is_some());
            this.texts[0].editor.update(cx, |text, cx| {
                let after = text
                    .bounds_for_range(3..3, Bounds::default(), window, cx)
                    .unwrap();
                assert!(after.left() > before.left());
            });
        })
        .unwrap();
    visual.simulate_mouse_up(
        start + point(px(20.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    let position = visual.debug_bounds("text-box-1").unwrap().center();
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        modifiers: Default::default(),
        first_mouse: false,
    });
    visual.simulate_mouse_up(position, MouseButton::Left, Default::default());
    visual.simulate_keystrokes("d");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "abcd");
            assert!(this.gesture.is_none());
        })
        .unwrap();
}

#[gpui::test]
fn panel_drags_capture_pointer_preserve_document_and_adapt_to_window(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let (board, history) = window
        .update(&mut visual.cx, |this, _, _| {
            (this.boards[0].clone(), this.history.borrow().undo_len())
        })
        .unwrap();
    click(&mut visual, "toggle-layers");
    // The draw tool must not create anything when resizing over the canvas.
    click(&mut visual, "add-rectangle");
    let initial_width = visual.debug_bounds("layers-panel").unwrap().size.width;
    drag(&mut visual, "resize-left-panel", 80., false);
    assert_eq!(
        visual.debug_bounds("layers-panel").unwrap().size.width,
        initial_width + px(80.)
    );
    drag(&mut visual, "resize-right-panel", -100., false);
    assert_eq!(
        visual.debug_bounds("properties-panel").unwrap().size.width,
        px(388.)
    );
    drag(&mut visual, "resize-right-panel", 40., true);
    assert_eq!(
        visual.debug_bounds("properties-panel").unwrap().size.width,
        px(388.)
    );
    drag(&mut visual, "resize-right-panel", 1000., false);
    assert_eq!(
        visual.debug_bounds("properties-panel").unwrap().size.width,
        px(256.)
    );
    drag(&mut visual, "resize-right-panel", -132., false);
    drag(&mut visual, "resize-left-panel", -1000., false);
    assert_eq!(
        visual.debug_bounds("layers-panel").unwrap().size.width,
        px(216.)
    );
    drag(&mut visual, "resize-left-panel", 1000., false);
    assert_eq!(
        visual.debug_bounds("layers-panel").unwrap().size.width,
        px(420.)
    );
    click(&mut visual, "toggle-layers");
    assert!(visual.debug_bounds("resize-left-panel").is_none());
    click(&mut visual, "toggle-layers");
    assert_eq!(
        visual.debug_bounds("layers-panel").unwrap().size.width,
        px(420.)
    );
    visual.simulate_resize(size(px(840.), px(520.)));
    draw(&mut visual);
    let left = visual.debug_bounds("layers-panel").unwrap();
    let right = visual.debug_bounds("properties-panel").unwrap();
    assert!(right.left() - left.right() >= px(CANVAS_SPACE));
    assert!(left.size.width >= px(Side::Left.minimum()));
    assert!(right.size.width >= px(Side::Right.minimum()));
    visual.simulate_resize(size(px(1280.), px(800.)));
    draw(&mut visual);
    assert_eq!(
        visual.debug_bounds("layers-panel").unwrap().size.width,
        px(420.)
    );
    assert_eq!(
        visual.debug_bounds("properties-panel").unwrap().size.width,
        px(388.)
    );
    window
        .update(&mut visual.cx, |this, window, _| {
            assert_eq!(this.boards, vec![board]);
            assert!(this.shapes.is_empty());
            assert_eq!(this.selected, Some(1));
            assert_eq!(this.history.borrow().undo_len(), history);
            assert!(this.gesture.is_none());
            assert!(window.captured_hitbox().is_none());
        })
        .unwrap();
}
