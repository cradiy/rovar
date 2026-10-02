use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

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
