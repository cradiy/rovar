use super::{StyleChange, StyledText, TextEditor, element, layout::TextLayout};
use gpui::{
    ClipboardItem, Context, Entity, EntityInputHandler, IntoElement, KeyDownEvent, Keystroke,
    Render, TestAppContext, Window, prelude::*, px, size,
};

#[gpui::test]
fn wrapping_alignment_spacing_and_zoom_share_the_caret_hit_test_layout(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |_, window, _| {
            let content = "中文AB";
            let bounds = gpui::Bounds::new(gpui::point(px(40.), px(30.)), size(px(400.), px(10.)));
            let mut style = StyledText::default();
            style.replace(0..0, content.len());
            let left = TextLayout::new(content, &style, 1., bounds, window);
            let width = left.caret_bounds(content.len()).left() - bounds.left();
            style.apply(
                0..content.len(),
                &StyleChange::Align(gpui::TextAlign::Center),
                true,
            );
            style.apply(0..content.len(), &StyleChange::Spacing(5.), true);
            let centered = TextLayout::new(content, &style, 1., bounds, window);
            let center_start = centered.caret_bounds(0).left();
            assert!(
                (f32::from(
                    center_start - bounds.left() - (bounds.size.width - width - px(15.)) / 2.
                ))
                .abs()
                    < 0.01
            );
            for index in [0, 3, 6, 7, 8] {
                let caret = centered.caret_bounds(index);
                assert_eq!(
                    centered.index(caret.origin + gpui::point(px(0.), px(5.))),
                    index
                );
            }
            let narrow = TextLayout::new(
                content,
                &style,
                1.,
                gpui::Bounds::new(bounds.origin, size(px(1.), px(10.))),
                window,
            );
            assert!(narrow.caret_bounds(content.len()).top() > bounds.bottom());
            assert_eq!(narrow.navigate(3, "up"), 0);
            let half = TextLayout::new(
                content,
                &style,
                0.5,
                gpui::Bounds::new(bounds.origin, size(px(200.), px(5.))),
                window,
            );
            assert!(
                (f32::from(half.caret_bounds(0).left() - bounds.left()) * 2.
                    - f32::from(center_start - bounds.left()))
                .abs()
                    < 0.1
            );
        })
        .unwrap();
}

struct TestView {
    editor: Entity<TextEditor>,
}

#[gpui::test]
fn color_styles_apply_to_selected_text_and_manual_color_detaches_with_undo(
    cx: &mut TestAppContext,
) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, window, cx| {
            view.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "A中😀B", window, cx);
                text.move_cursor(1, false);
                text.move_cursor(8, true);
                let id = uuid::Uuid::new_v4().to_string();
                text.apply_style(
                    StyleChange::ColorStyle(
                        Some(id.clone()),
                        crate::scene::color_styles::ColorStyle {
                            name: "Brand".into(),
                            color: gpui::rgb(0x334455),
                            gradient: None,
                        },
                    ),
                    cx,
                );
                assert!(text.styles.at(0).color_style.is_none());
                assert_eq!(text.styles.at(1).color_style.as_deref(), Some(id.as_str()));
                assert!(text.styles.at(8).color_style.is_none());
                text.history.borrow_mut().break_group();
                text.apply_style(StyleChange::Opacity(0.5), cx);
                assert!(text.styles.at(1).color_style.is_none());
                assert_eq!(text.styles.at(1).color.a, 0.5);
                replay(text, false);
                assert_eq!(text.styles.at(1).color_style.as_deref(), Some(id.as_str()));
                assert_eq!(text.styles.at(1).color.a, 1.);
            });
        })
        .unwrap();
}

#[gpui::test]
fn merged_typing_style_edits_and_replay_advance_document_revision(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, _, cx| {
            view.editor.update(cx, |text, cx| {
                let mut revision = text.history.borrow().revision();
                for (index, value) in ["A", "B", "C"].into_iter().enumerate() {
                    text.edit(index..index, value, true);
                    let next = text.history.borrow().revision();
                    assert!(next > revision);
                    assert_eq!(text.history.borrow().undo_len(), 1);
                    revision = next;
                }
                text.edit(0..3, "ABC", true);
                assert_eq!(text.history.borrow().revision(), revision);
                text.move_cursor(0, false);
                text.move_cursor(3, true);
                for size in [32., 48.] {
                    text.apply_style(StyleChange::Size(size), cx);
                    let next = text.history.borrow().revision();
                    assert!(next > revision);
                    revision = next;
                }
                assert_eq!(text.history.borrow().undo_len(), 2);
                replay(text, false);
                assert_eq!(text.styles.at(0).size, 24.);
                assert!(text.history.borrow().revision() > revision);
                revision = text.history.borrow().revision();
                replay(text, true);
                assert_eq!(text.styles.at(0).size, 48.);
                assert!(text.history.borrow().revision() > revision);
            });
        })
        .unwrap();
}

#[gpui::test]
fn rich_styles_follow_unicode_edits_composition_and_undo(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, window, cx| {
            view.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "A中😀B", window, cx);
                text.move_cursor(1, false);
                text.move_cursor(8, true);
                text.apply_style(StyleChange::Size(48.), cx);
                text.apply_style(StyleChange::Color(gpui::rgb(0xff0000)), cx);
                text.apply_style(
                    StyleChange::FillMode(crate::scene::artboard::FillMode::Linear),
                    cx,
                );
                let original = text.styles.clone();
                let history_len = text.history.borrow().undo_len();
                text.replace_text_in_range(Some(0..5), "A中😀B", window, cx);
                assert!(text.styles == original);
                assert_eq!(text.history.borrow().undo_len(), history_len);
                assert_eq!(text.styles.at(0).size, 24.);
                assert_eq!(text.styles.at(1).size, 48.);
                assert_eq!(text.styles.at(8).size, 24.);
                text.move_cursor(8, false);
                text.replace_text_in_range(None, "好", window, cx);
                assert_eq!(text.styles.at(8).size, 48.);
                assert_eq!(text.styles.at(11).size, 24.);
                text.replace_and_mark_text_in_range(Some(2..4), "世", Some(1..1), window, cx);
                text.replace_text_in_range(None, "界", window, cx);
                assert_eq!(text.content, "A中界好B");
                assert_eq!(text.styles.at(4).size, 48.);
                replay(text, false);
                assert_eq!(text.content, "A中😀好B");
                replay(text, false);
                assert_eq!(text.content, "A中😀B");
                assert!(text.styles == original);
                // Replacing across differently styled runs keeps the suffix's original style.
                text.replace_text_in_range(Some(0..2), "中文", window, cx);
                assert_eq!(text.content, "中文😀B");
                assert_eq!(text.styles.at(0).size, 24.);
                assert_eq!(text.styles.at(6).size, 48.);
                assert_eq!(text.styles.at(10).size, 24.);
                let mut end = 0;
                for run in &text.styles.runs {
                    assert_eq!(run.range.start, end);
                    assert!(text.content.is_char_boundary(run.range.start));
                    end = run.range.end;
                }
                assert_eq!(end, text.content.len());
            })
        })
        .unwrap();
}

#[gpui::test]
fn mixed_sizes_wrap_and_align_to_one_baseline_and_paragraph_styles_do_not_leak(
    cx: &mut TestAppContext,
) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, window, cx| {
            view.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "小大\n另一段", window, cx);
                text.move_cursor(3, false);
                text.move_cursor(6, true);
                text.apply_style(StyleChange::Size(64.), cx);
                text.apply_style(StyleChange::LineHeight(2.), cx);
                text.apply_style(StyleChange::Align(gpui::TextAlign::Right), cx);
                assert_eq!(text.styles.at(0).line_height, 2.);
                assert_eq!(text.styles.at(7).line_height, 1.5);
                assert_eq!(text.styles.at(7).align, gpui::TextAlign::Left);
                let bounds =
                    gpui::Bounds::new(gpui::point(px(0.), px(0.)), size(px(400.), px(30.)));
                let layout = TextLayout::new(&text.content, &text.styles, 1., bounds, window);
                assert_eq!(layout.caret_bounds(0).size.height, px(128.));
                assert_eq!(layout.caret_bounds(7).top(), px(128.));
                assert_eq!(layout.caret_bounds(7).left(), px(0.));
                for index in [0, 3, 6, 7, 10, 13, 16] {
                    let caret = layout.caret_bounds(index);
                    assert_eq!(
                        layout.index(caret.origin + gpui::point(px(0.), caret.size.height / 2.)),
                        index
                    );
                }
            })
        })
        .unwrap();
}
impl Render for TestView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        element(
            &self.editor,
            1.,
            self.editor.read(cx).focus.clone(),
            Vec::new(),
            cx,
        )
    }
}

#[gpui::test]
fn composition_utf16_replacement_is_one_undo_step_and_paste_is_separate(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, window, cx| {
            view.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "A😀B", window, cx);
                text.move_cursor("A😀".len(), false);
                text.replace_and_mark_text_in_range(Some(1..3), "你好", Some(1..2), window, cx);
                assert_eq!(text.content, "A你好B");
                assert_eq!(
                    text.selected_text_range(false, window, cx).unwrap().range,
                    2..3
                );
                text.replace_and_mark_text_in_range(None, "您好", Some(2..2), window, cx);
                text.replace_text_in_range(None, "您好", window, cx);
                assert_eq!(text.content, "A您好B");
                let event = |key: &str| KeyDownEvent {
                    keystroke: Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                replay(text, false);
                assert_eq!(text.content, "A😀B");
                replay(text, true);
                assert_eq!(text.content, "A您好B");
                cx.write_to_clipboard(ClipboardItem::new_string("\r\n👩‍💻".to_owned()));
                text.key_down(&event("ctrl-v"), window, cx);
                assert_eq!(text.content, "A您好\n👩‍💻B");
                text.key_down(&event("backspace"), window, cx);
                assert_eq!(text.content, "A您好\nB");
                replay(text, false);
                replay(text, false);
                assert_eq!(text.content, "A您好B");
                let history_len = text.history.borrow().undo_len();
                text.replace_and_mark_text_in_range(None, "ni", None, window, cx);
                text.replace_text_in_range(None, "", window, cx);
                assert_eq!(text.content, "A您好B");
                assert_eq!(text.history.borrow().undo_len(), history_len);
                replay(text, true);
                assert_eq!(text.content, "A您好\n👩‍💻B");
            });
        })
        .unwrap();
}

#[gpui::test]
fn cached_layout_tracks_movement_resize_style_undo_and_composition(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(500.), px(400.)), |window, cx| TestView {
        editor: cx.new(|cx| TextEditor::new(0, Default::default(), window, cx)),
    });
    window
        .update(cx, |view, window, cx| {
            view.editor.update(cx, |text, cx| {
                text.replace_text_in_range(None, "A中文😀B", window, cx);
                let bounds =
                    gpui::Bounds::new(gpui::point(px(10.), px(20.)), size(px(320.), px(120.)));
                let original = text.layout_for_bounds(1., bounds, window);
                text.move_cursor(1, false);
                text.move_cursor(7, true);
                assert!(original.shares_rows(&text.layout_for_bounds(1., bounds, window)));

                let mut moved_bounds = bounds;
                moved_bounds.origin += gpui::point(px(80.), px(40.));
                moved_bounds.size.height = px(50.);
                let moved = text.layout_for_bounds(1., moved_bounds, window);
                assert!(original.shares_rows(&moved));
                for index in [0, 1, 4, 7, 11, 12] {
                    assert_eq!(
                        moved.caret_bounds(index).origin,
                        original.caret_bounds(index).origin + gpui::point(px(80.), px(40.))
                    );
                    assert_eq!(moved.index(moved.caret_bounds(index).origin), index);
                }

                let mut narrow_bounds = moved_bounds;
                narrow_bounds.size.width = px(1.);
                let narrow = text.layout_for_bounds(1., narrow_bounds, window);
                assert!(!moved.shares_rows(&narrow));
                assert!(narrow.caret_bounds(12).top() > narrow.caret_bounds(0).top());
                let zoomed = text.layout_for_bounds(0.5, bounds, window);
                assert!(!narrow.shares_rows(&zoomed));
                assert_eq!(
                    zoomed.caret_bounds(0).size.height * 2.,
                    original.caret_bounds(0).size.height
                );

                text.apply_style(StyleChange::Size(48.), cx);
                let styled = text.layout_for_bounds(1., bounds, window);
                assert!(!zoomed.shares_rows(&styled));
                assert!(styled.caret_bounds(1).size.height > original.caret_bounds(1).size.height);
                replay(text, false);
                let undone = text.layout_for_bounds(1., bounds, window);
                assert!(!styled.shares_rows(&undone));
                assert_eq!(undone.caret_bounds(7), original.caret_bounds(7));

                text.replace_and_mark_text_in_range(Some(1..3), "试", None, window, cx);
                let preedit = text.layout_for_bounds(1., bounds, window);
                assert!(!undone.shares_rows(&preedit));
                text.replace_text_in_range(None, "测试", window, cx);
                let committed = text.layout_for_bounds(1., bounds, window);
                assert!(!preedit.shares_rows(&committed));
                assert_eq!(
                    committed.caret_bounds(text.content.len()),
                    original.caret_bounds(12)
                );
            });
        })
        .unwrap();
}

// Exercise the same inverse changes as the workspace without constructing its UI.
fn replay(text: &mut TextEditor, redo: bool) {
    use crate::scene::history::Change;
    let changes = text.history.borrow_mut().take(redo).unwrap();
    let mut inverse = Vec::new();
    for change in changes.into_iter().rev() {
        let Change::Text { id, value } = change else {
            panic!("expected text edit")
        };
        inverse.push(Change::Text {
            id,
            value: text.snapshot(),
        });
        text.restore(value);
    }
    text.history.borrow_mut().finish_replay(inverse, redo);
}
