use super::*;
use crate::workspace::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

fn start(visual: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    draw(visual);
    let p = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
    p
}
fn finish(visual: &mut VisualTestContext, p: Point<Pixels>, delta: f32) {
    let end = p + point(px(delta), px(0.));
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    draw(visual);
}

#[gpui::test]
fn gradient_popup_stays_open_during_scrub_release_and_cancel(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    click(&mut visual, "stroke-visibility");
    for (slot, swatch) in [(0, "property-drag-5"), (1, "property-drag-16")] {
        click(&mut visual, swatch);
        click(&mut visual, "fill-linear");
        // Begin from an input inside the popover as well as the popover's own focus.
        if slot == 0 {
            click(&mut visual, "property-7");
        }
        let depth = window
            .update(&mut visual.cx, |this, _, _| {
                this.history.borrow().undo_len()
            })
            .unwrap();
        let origin = start(&mut visual, "property-drag-7");
        draw(&mut visual);
        assert!(visual.debug_bounds("color-panel").is_some());
        let panel = visual.debug_bounds("color-panel").unwrap();
        let end = point(panel.right() + px(20.), origin.y);
        let expected_angle = (90. + f32::from(end.x - origin.x)).clamp(0., 360.);
        assert!(!panel.contains(&end));
        visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(this.paint_popovers[slot].read(cx).is_open());
                assert_eq!(
                    this.selected_shape()
                        .unwrap()
                        .paint_gradient(slot == 1)
                        .angle,
                    expected_angle
                );
                assert_eq!(this.history.borrow().undo_len(), depth);
            })
            .unwrap();
        visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(this.paint_popovers[slot].read(cx).is_open());
                assert!(this.gesture.is_none());
                assert_eq!(this.history.borrow().undo_len(), depth + 1);
            })
            .unwrap();
        visual.simulate_keystrokes("ctrl-z");
        draw(&mut visual);
        let start = start(&mut visual, "property-drag-8");
        visual.simulate_mouse_move(
            start + point(px(25.), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
        visual.simulate_keystrokes("escape");
        visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert!(this.paint_popovers[slot].read(cx).is_open());
                assert!(this.gesture.is_none());
                assert_eq!(
                    this.selected_shape()
                        .unwrap()
                        .paint_gradient(slot == 1)
                        .stops()[0]
                        .position,
                    0.
                );
                assert!(this.history.borrow().can_redo());
            })
            .unwrap();
        visual.simulate_keystrokes("ctrl-shift-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(
                    this.selected_shape()
                        .unwrap()
                        .paint_gradient(slot == 1)
                        .angle,
                    expected_angle
                );
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("color-panel").is_none());
    }
}

#[gpui::test]
fn numeric_drag_previews_clamps_and_commits_once_and_cancel_preserves_redo(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let p = start(&mut visual, "property-drag-3");
    for delta in [10., 40., 80.] {
        visual.simulate_mouse_move(
            p + point(px(delta), px(0.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.boards[0].rect.width, 640. + delta);
                assert_eq!(this.history.borrow().undo_len(), 1);
            })
            .unwrap();
    }
    // Release outside the field, over the editor, rather than its original hitbox.
    finish(&mut visual, p, -100.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 540.);
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    let p = start(&mut visual, "property-drag-3");
    visual.simulate_mouse_move(
        p + point(px(50.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 640.)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 540.)
        })
        .unwrap();
    // A click and a round trip to the starting value do not add history entries.
    let p = start(&mut visual, "property-drag-3");
    finish(&mut visual, p, 0.);
    let p = start(&mut visual, "property-drag-3");
    visual.simulate_mouse_move(
        p + point(px(20.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    finish(&mut visual, p, 0.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), 2)
        })
        .unwrap();
    let p = start(&mut visual, "property-drag-3");
    finish(&mut visual, p, -2000.);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 1.)
        })
        .unwrap();
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a 3 2 0");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 320.)
        })
        .unwrap();
}

#[gpui::test]
fn text_scrubbing_preserves_style_selection_fractional_values_and_history(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-text");
    visual.simulate_input("abcd");
    visual.simulate_keystrokes("ctrl-home shift-right shift-right");
    draw(&mut visual);
    let before = window
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    let p = start(&mut visual, "property-drag-7");
    finish(&mut visual, p, 16.);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert_eq!(editor.style_range(), 0..2);
            assert_eq!(editor.effective_style().size, 40.);
            assert_eq!(this.history.borrow().undo_len(), before + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 24.)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    // Return focus to the text and inspect the untouched half of the text.
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0]
                .editor
                .read(cx)
                .focus
                .clone()
                .focus(window, cx)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-end shift-left shift-left");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 24.)
        })
        .unwrap();
    let p = start(&mut visual, "property-drag-8");
    finish(&mut visual, p, 15.);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(
                (this.texts[0].editor.read(cx).effective_style().line_height - 1.65).abs() < 0.001
            );
        })
        .unwrap();
    let p = start(&mut visual, "property-drag-10");
    finish(&mut visual, p, 20.);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).effective_style().weight, 600.)
        })
        .unwrap();
    // Cancelling a text-style preview must restore the selected range and style.
    let p = start(&mut visual, "property-drag-7");
    visual.simulate_mouse_move(
        p + point(px(30.), px(0.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert_eq!(editor.style_range(), 2..4);
            assert_eq!(editor.effective_style().size, 24.);
            assert_eq!(editor.content, "abcd");
        })
        .unwrap();
}
