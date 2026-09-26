use super::super::tests::{click, create, draw, open};
use super::*;
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext};

#[gpui::test]
fn shortcuts_follow_one_history_across_text_properties_and_creation(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_keystrokes("a b c ctrl-home shift-right");
    click(&mut visual, "property-7");
    visual.simulate_keystrokes("ctrl-a 4 8");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "abc");
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 48.);
            assert_eq!(this.history.borrow().undo_len(), 4);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            let editor = this.texts[0].editor.read(cx);
            assert_eq!(editor.style_range(), 0..1);
            assert_eq!(editor.effective_style().size, 24.);
            assert_eq!(this.fields[7].read(cx).value().as_ref(), "24");
            assert!(this.fields[7].focus_handle(cx).is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.texts[0].editor.read(cx).content.is_empty())
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, _| {
            assert!(this.texts.is_empty());
            assert_eq!(this.boards.len(), 1);
            assert!(this.focus.is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.boards.is_empty()))
        .unwrap();
    for _ in 0..4 {
        visual.simulate_keystrokes("ctrl-shift-z");
        draw(&mut visual);
    }
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards[0].id, 1);
            assert_eq!(this.texts[0].id, 2);
            assert_eq!(this.texts[0].editor.read(cx).content, "abc");
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 48.);
        })
        .unwrap();
}

#[gpui::test]
fn deleting_a_board_restores_its_children_styles_and_order_as_one_step(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("A中😀B");
    visual.simulate_keystrokes("ctrl-home shift-right");
    click(&mut visual, "fill-linear");
    create(&mut visual, "add-text");
    visual.simulate_input("second");
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("unrelated");
    let (saved, board, depth) = window
        .update(&mut visual.cx, |this, window, cx| {
            this.select(Some(1), cx);
            let saved: Vec<_> = (0..this.texts.len())
                .map(|i| this.saved_text(i, cx))
                .collect();
            this.focus.focus(window, cx);
            (
                saved,
                this.boards[0].clone(),
                this.history.borrow().undo_len(),
            )
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards.len(), 1);
            assert_eq!(this.texts.iter().map(|t| t.id).collect::<Vec<_>>(), vec![5]);
            assert_eq!(this.history.borrow().undo_len(), depth + 1);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.boards[0], board);
            assert_eq!(this.selected, Some(1));
            assert!(this.selected_text.is_none());
            assert_eq!(
                this.texts.iter().map(|t| t.id).collect::<Vec<_>>(),
                vec![2, 3, 5]
            );
            for (index, old) in saved.iter().enumerate() {
                let restored = this.saved_text(index, cx);
                assert_eq!(restored.board, old.board);
                assert_eq!(restored.rect, old.rect);
                assert!(restored.text == old.text);
            }
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts.len(), 1);
            assert_eq!(this.texts[0].editor.read(cx).content, "unrelated");
        })
        .unwrap();
}

#[gpui::test]
fn pointer_drag_is_one_step_and_cancel_does_not_destroy_redo(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    let original = window
        .update(&mut visual.cx, |this, _, _| this.boards[0].rect)
        .unwrap();
    let start = visual.debug_bounds("artboard-1").unwrap().origin + point(px(15.), px(15.));
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    for i in 1..5 {
        visual.simulate_mouse_move(
            start + point(px(i as f32 * 10.), px(20.)),
            MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
    }
    visual.simulate_mouse_up(
        start + point(px(40.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    let moved = window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.history.borrow().undo_len(), 2);
            this.boards[0].rect
        })
        .unwrap();
    assert_ne!(original, moved);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start + point(px(20.), px(10.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_keystrokes("escape");
    visual.simulate_mouse_up(start, MouseButton::Left, Default::default());
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect, original)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect, moved)
        })
        .unwrap();
    // Invalid inspector input must not discard redo, but a new valid edit must.
    visual.simulate_keystrokes("ctrl-z");
    click(&mut visual, "property-3");
    visual.simulate_keystrokes("ctrl-a 0 ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect, moved)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z ctrl-a 3 2 0 ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.boards[0].rect.width, 320.);
            assert_eq!(this.boards[0].rect.x, original.x);
        })
        .unwrap();
}

#[gpui::test]
fn picker_preview_commit_groups_board_and_text_colors(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    for text in [false, true] {
        if text {
            visual.simulate_keystrokes("escape");
            draw(&mut visual);
            create(&mut visual, "add-text");
            visual.simulate_input("colors");
        }
        click(&mut visual, "fill-linear");
        click(&mut visual, "gradient-stop-1");
        let (before, depth) = window
            .update(&mut visual.cx, |this, _, cx| {
                let gradient = if text {
                    this.texts[0]
                        .editor
                        .read(cx)
                        .effective_style()
                        .gradient
                        .clone()
                } else {
                    this.boards[0].gradient.clone()
                };
                (gradient, this.history.borrow().undo_len())
            })
            .unwrap();
        for color in [rgb(0xff0000), rgb(0x00ff00), rgb(0x0000ff)] {
            window
                .update(&mut visual.cx, |this, _, cx| {
                    this.picker
                        .update(cx, |_, cx| cx.emit(ColorPickerEvent::Preview(color)));
                })
                .unwrap();
            draw(&mut visual);
        }
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.picker
                    .update(cx, |_, cx| cx.emit(ColorPickerEvent::Commit(rgb(0x0000ff))));
            })
            .unwrap();
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.history.borrow().undo_len(), depth + 1)
            })
            .unwrap();
        visual.simulate_keystrokes("ctrl-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                let gradient = if text {
                    &this.texts[0].editor.read(cx).effective_style().gradient
                } else {
                    &this.boards[0].gradient
                };
                assert_eq!(*gradient, before);
                assert_eq!(this.active_stop, 1);
            })
            .unwrap();
        visual.simulate_keystrokes("ctrl-shift-z");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                let gradient = if text {
                    &this.texts[0].editor.read(cx).effective_style().gradient
                } else {
                    &this.boards[0].gradient
                };
                assert_eq!(gradient.stop(1).unwrap().color, rgb(0x0000ff));
            })
            .unwrap();
    }
}

#[gpui::test]
fn composition_keeps_shortcuts_until_commit_and_undo_restores_unicode(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("A😀B");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0].editor.update(cx, |text, cx| {
                text.replace_and_mark_text_in_range(Some(1..3), "ni", None, window, cx)
            });
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0].editor.update(cx, |text, cx| {
                assert_eq!(text.content, "AniB");
                assert!(text.is_composing());
                text.replace_text_in_range(None, "你", window, cx);
            });
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "A😀B")
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "A你B")
        })
        .unwrap();
}

#[gpui::test]
fn resized_text_can_be_deleted_restored_and_undone_through_its_previous_edits(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("fixed box");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    let original = window
        .update(&mut visual.cx, |this, _, _| this.texts[0].rect)
        .unwrap();
    let start = visual.debug_bounds("text-handle-4").unwrap().center();
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        start + point(px(-45.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_up(
        start + point(px(-45.), px(20.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    let resized = window
        .update(&mut visual.cx, |this, _, _| this.texts[0].rect)
        .unwrap();
    assert_ne!(resized, original);
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.texts[0].rect, original)
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    visual.simulate_keystrokes("delete");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| assert!(this.texts.is_empty()))
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].rect, resized);
            assert_eq!(this.texts[0].editor.read(cx).content, "fixed box");
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].rect, original);
            assert!(!this.texts[0].editor.read(cx).editing);
            assert_eq!(this.texts[0].editor.read(cx).effective_style().size, 24.);
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.texts[0].editor.read(cx).content.is_empty())
        })
        .unwrap();
}

#[gpui::test]
fn empty_preedit_after_chinese_commit_allows_backspace_without_refocusing(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("😀");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0].editor.update(cx, |text, cx| {
                // Wayland may deliver CommitString, then an empty PreeditString at Done.
                text.replace_and_mark_text_in_range(None, "ni", None, window, cx);
                text.replace_text_in_range(None, "你", window, cx);
                text.replace_and_mark_text_in_range(None, "", None, window, cx);
            });
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("backspace");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            let text = this.texts[0].editor.read(cx);
            assert_eq!(text.content, "😀");
            assert!(!text.is_composing());
            assert!(text.focus.is_focused(window));
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "😀你");
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.texts[0].editor.update(cx, |text, cx| {
                assert_eq!(text.content, "😀");
                // Cancelling another preedit must not replace the pending redo branch.
                text.replace_and_mark_text_in_range(None, "hao", None, window, cx);
                text.replace_and_mark_text_in_range(None, "", None, window, cx);
            });
        })
        .unwrap();
    draw(&mut visual);
    visual.simulate_keystrokes("ctrl-shift-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(this.texts[0].editor.read(cx).content, "😀你");
        })
        .unwrap();
    visual.simulate_keystrokes("backspace backspace");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            let text = this.texts[0].editor.read(cx);
            assert!(text.content.is_empty());
            assert!(text.focus.is_focused(window));
            assert_eq!(this.texts.len(), 1);
        })
        .unwrap();
}
