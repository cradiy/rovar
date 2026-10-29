use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn text_format_buttons_preserve_unicode_selection_history_and_saved_runs(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("ab中cd");
    visual.simulate_keystrokes("secondary-home right shift-right shift-right");
    for id in [
        "text-bold",
        "text-italic",
        "text-underline",
        "text-strikethrough",
    ] {
        click(&mut visual, id);
    }
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.inspector
                .inspector_scroll
                .set_offset(gpui::point(gpui::px(0.), gpui::px(-450.)));
            cx.notify();
        })
        .unwrap();
    click(&mut visual, "text-background-toggle");
    window
        .update(&mut visual.cx, |this, window, cx| {
            let text = this.texts[0].editor.read(cx);
            assert_eq!(text.style_range(), 1..5);
            let page = this.snapshot_page(cx).0;
            let styles = &page.texts[0].styles;
            for offset in [0, 5] {
                let style = styles.at(offset);
                assert_eq!(style.weight, 400.);
                assert!(!style.italic && !style.underline && !style.strikethrough);
                assert!(style.background.is_none());
            }
            let selected = styles.at(1);
            assert_eq!(selected.weight, 700.);
            assert!(selected.italic && selected.underline && selected.strikethrough);
            assert!(selected.background.is_some());
            let loaded: crate::document::Page =
                serde_json::from_slice(&serde_json::to_vec(&page).unwrap()).unwrap();
            assert!(loaded == page);
            for _ in 0..5 {
                this.replay_history(false, window, cx);
            }
            let plain = this.snapshot_page(cx).0;
            assert!(
                plain.texts[0]
                    .styles
                    .runs
                    .iter()
                    .all(|run| run.style.weight == 400.
                        && !run.style.italic
                        && !run.style.underline
                        && !run.style.strikethrough
                        && run.style.background.is_none())
            );
            for _ in 0..5 {
                this.replay_history(true, window, cx);
            }
            assert!(this.snapshot_page(cx).0 == page);
        })
        .unwrap();
    // A mixed selection first becomes uniformly bold, then toggles back to regular.
    visual.simulate_keystrokes("secondary-a secondary-b");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let text = this.texts[0].editor.read(cx);
            assert!(!text.mixed(crate::scene::property::TextProperty::Weight));
            assert_eq!(text.effective_style().weight, 700.);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-b");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            let page = this.snapshot_page(cx).0;
            assert!(
                page.texts[0]
                    .styles
                    .runs
                    .iter()
                    .all(|run| run.style.weight == 400.)
            );
            assert!(page.texts[0].styles.at(1).italic);
            assert!(!page.texts[0].styles.at(0).italic);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-home right shift-right shift-right");
    click(&mut visual, "property-23");
    visual.simulate_keystrokes("secondary-a");
    visual.simulate_input("45");
    visual.simulate_keystrokes("enter");
    click(&mut visual, "property-drag-22");
    window
        .update(&mut visual.cx, |this, _, cx| {
            let page = this.snapshot_page(cx).0;
            assert_eq!(page.texts[0].styles.at(1).background.unwrap().a, 0.45);
            assert!(page.texts[0].styles.at(0).background.is_none());
            assert!(this.inspector.paint_popovers[2].read(cx).is_open());
            assert!((this.inspector.background_picker.read(cx).value().a - 0.45).abs() < 0.001);
        })
        .unwrap();
    click(&mut visual, "text-background-close");
    click(&mut visual, "text-background-toggle");
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert!(
                this.snapshot_page(cx).0.texts[0]
                    .styles
                    .runs
                    .iter()
                    .all(|run| run.style.background.is_none())
            );
            this.replay_history(false, window, cx);
            assert_eq!(
                this.snapshot_page(cx).0.texts[0]
                    .styles
                    .at(1)
                    .background
                    .unwrap()
                    .a,
                0.45
            );
        })
        .unwrap();
}

#[gpui::test]
fn search_only_filters_and_choosing_changes_only_the_selected_text(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-artboard");
    create(&mut visual, "add-text");
    visual.simulate_input("first");
    create(&mut visual, "add-text");
    visual.simulate_input("second");
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.inspector.font_picker.update(cx, |picker, cx| {
                picker.set_test_catalog(vec!["Fixture Sans".into(), "Fixture Serif".into()], cx);
            })
        })
        .unwrap();
    click(&mut visual, "font-picker");
    click(&mut visual, "font-search");
    visual.simulate_input("serif");
    draw(&mut visual);
    assert!(visual.debug_bounds("property-0").is_none());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(
                this.texts[1].editor.read(cx).effective_style().family,
                crate::scene::text::TextStyle::default().family
            );
        })
        .unwrap();
    assert!(visual.debug_bounds("font-option-1").is_none());
    click(&mut visual, "font-option-0");
    draw(&mut visual);
    assert!(visual.debug_bounds("font-search").is_none());
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(
                this.texts[0].editor.read(cx).effective_style().family,
                crate::scene::text::TextStyle::default().family
            );
            assert_eq!(
                this.texts[1]
                    .editor
                    .read(cx)
                    .effective_style()
                    .family
                    .as_ref(),
                "Fixture Serif"
            );
            assert_eq!(this.texts[1].editor.read(cx).content, "second");
        })
        .unwrap();
    click(&mut visual, "font-picker");
    click(&mut visual, "font-search");
    visual.simulate_input("not-installed");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert_eq!(
                this.texts[1]
                    .editor
                    .read(cx)
                    .effective_style()
                    .family
                    .as_ref(),
                "Fixture Serif"
            )
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("font-search").is_none());
}
