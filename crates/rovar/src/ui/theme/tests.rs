use super::*;
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, div, prelude::*, px, size};

struct Swatch {
    painted: std::rc::Rc<Cell<Rgba>>,
}
impl Render for Swatch {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let color = Color::Panel.color();
        self.painted.set(color);
        div().size_full().bg(color)
    }
}
struct Preview(Entity<Swatch>);
impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        activate(window, cx);
        self.0.clone().cached(div().size_full().style().clone())
    }
}

#[gpui::test]
fn switching_refreshes_cached_views_in_all_windows_and_persists(cx: &mut TestAppContext) {
    let root = rovar_storage::tempfile::tempdir().unwrap();
    let path = root.path().join("settings.json");
    crate::settings::write(&path, &serde_json::json!({"language": "zh-CN"})).unwrap();
    assert_eq!(load(Some(&path)).unwrap(), Mode::System);
    cx.update(|cx| cx.set_global(Theme(Mode::Dark)));
    let painted =
        std::array::from_fn::<_, 2, _>(|_| std::rc::Rc::new(Cell::new(gpui::Rgba::default())));
    let windows = painted.clone().map(|painted| {
        cx.open_window(size(px(200.), px(100.)), |_, cx| {
            Preview(cx.new(|_| Swatch { painted }))
        })
    });
    for mode in [Mode::Dark, Mode::Light, Mode::Dark] {
        windows[0]
            .update(cx, |_, window, cx| set_at(mode, &path, window, cx).unwrap())
            .unwrap();
        cx.run_until_parked();
        for window in windows {
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
                .unwrap();
        }
        let expected = Color::Panel.color();
        assert!(painted.iter().all(|painted| painted.get() == expected));
        assert_eq!(load(Some(&path)).unwrap(), mode);
        assert_eq!(
            crate::settings::read(Some(&path)).unwrap()["language"],
            "zh-CN"
        );
    }
    let dark = painted[0].get();
    crate::settings::write(&path, &serde_json::json!("invalid settings")).unwrap();
    windows[0]
        .update(cx, |_, window, cx| {
            assert!(set_at(Mode::Light, &path, window, cx).is_err());
            assert_eq!(preference(cx), Mode::Dark);
            assert_eq!(Color::Panel.color(), dark);
        })
        .unwrap();
    assert!(load(Some(&path)).is_err());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&rovar_storage::fs::read(&path).unwrap())
            .unwrap(),
        "invalid settings"
    );
}

#[test]
fn explicit_modes_override_system_appearance() {
    for appearance in [
        WindowAppearance::Light,
        WindowAppearance::VibrantLight,
        WindowAppearance::Dark,
        WindowAppearance::VibrantDark,
    ] {
        assert!(Mode::Dark.dark(appearance));
        assert!(!Mode::Light.dark(appearance));
        assert_eq!(
            Mode::System.dark(appearance),
            matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            )
        );
    }
}

#[test]
fn interface_text_has_readable_contrast_in_both_palettes() {
    fn luminance(color: Rgba) -> f32 {
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }
    let original = is_dark();
    for dark in [true, false] {
        DARK.set(dark);
        for (foreground, background) in [
            (Color::Text, Color::Panel),
            (Color::Text, Color::Input),
            (Color::Muted, Color::Panel),
            (Color::Accent, Color::Selected),
            (Color::OnAccent, Color::Accent),
        ] {
            let a = luminance(foreground.color());
            let b = luminance(background.color());
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5);
        }
    }
    DARK.set(original);
}
