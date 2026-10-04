use gpui::{App, Global, Rgba, Window, WindowAppearance};
use std::cell::Cell;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn label(self) -> &'static str {
        crate::i18n::t(match self {
            Self::System => "theme-system",
            Self::Light => "theme-light",
            Self::Dark => "theme-dark",
        })
    }
    fn dark(self, appearance: WindowAppearance) -> bool {
        match self {
            Self::Dark => true,
            Self::Light => false,
            Self::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        }
    }
}

struct Theme(Mode);
impl Global for Theme {}

// Colors are resolved on the UI thread. This also isolates parallel test apps;
// document rendering on worker threads never reads application theme colors.
thread_local! { static DARK: Cell<bool> = const { Cell::new(true) }; }

pub(crate) fn is_dark() -> bool {
    DARK.get()
}

pub(crate) fn preference(cx: &App) -> Mode {
    cx.try_global::<Theme>()
        .map_or(Mode::default(), |theme| theme.0)
}

pub(crate) fn activate(window: &Window, cx: &App) {
    DARK.set(preference(cx).dark(window.appearance()));
}

pub(crate) fn init(cx: &mut App) -> std::io::Result<()> {
    cx.set_global(Theme(load(crate::settings::path().as_deref())?));
    Ok(())
}

fn load(path: Option<&Path>) -> std::io::Result<Mode> {
    let config = crate::settings::read(path)?;
    Ok(config
        .get("theme")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or_default())
}

pub(crate) fn set(mode: Mode, window: &Window, cx: &mut App) -> std::io::Result<()> {
    let path = crate::settings::path()
        .ok_or_else(|| std::io::Error::other("Could not locate settings directory"))?;
    set_at(mode, &path, window, cx)
}

fn set_at(mode: Mode, path: &Path, window: &Window, cx: &mut App) -> std::io::Result<()> {
    let mut config = crate::settings::read(Some(path))?;
    config["theme"] = serde_json::to_value(mode)?;
    crate::settings::write(path, &config)?;
    cx.set_global(Theme(mode));
    activate(window, cx);
    cx.refresh_windows();
    Ok(())
}

pub(crate) fn input_appearance() -> uic::components::input::InputAppearance {
    uic::components::input::InputAppearance {
        placeholder: Color::Muted.color().into(),
        focus_border: Color::Accent.color().into(),
        caret: Color::Accent.color().into(),
        selection: Color::Selection.color().into(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(crate) enum Color {
    Panel,
    Workspace,
    Border,
    Text,
    Muted,
    Accent,
    Surface,
    Input,
    Hover,
    Selected,
    OnAccent,
    Danger,
    DangerSurface,
    Warning,
    Success,
    Overlay,
    Selection,
    #[cfg(not(target_family = "wasm"))]
    Glass,
    CanvasGrid,
    PixelGrid,
    Handle,
    Checker,
    Transparent,
    Component,
    Guide,
    Shadow,
}

impl Color {
    pub fn color(self) -> Rgba {
        let (dark, light) = match self {
            Self::Panel => (0x1d2026ff, 0xfafafdff),
            Self::Workspace => (0x15171cff, 0xe9eaf0ff),
            Self::Border => (0x30343dff, 0xd5d7e0ff),
            Self::Text => (0xdde0e8ff, 0x252734ff),
            Self::Muted => (0x959ba9ff, 0x686e80ff),
            Self::Accent => (0xb4a2eeff, 0x7250bdff),
            Self::Surface => (0x25252eff, 0xffffffff),
            Self::Input => (0x282b33ff, 0xedeef4ff),
            Self::Hover => (0x30333dff, 0xe1e3ecff),
            Self::Selected => (0x353044ff, 0xe6dff5ff),
            Self::OnAccent => (0x171321ff, 0xffffffff),
            Self::Danger => (0xff877bff, 0xbb302dff),
            Self::DangerSurface => (0x823f49ff, 0xf6dcdcff),
            Self::Warning => (0xd5b777ff, 0x826006ff),
            Self::Success => (0x98c6adff, 0x267b4fff),
            Self::Overlay => (0x00000066, 0x15182840),
            Self::Selection => (0xb4a2ee44, 0x7250bd33),
            #[cfg(not(target_family = "wasm"))]
            Self::Glass => (0x20202acc, 0xf8f8fce8),
            Self::CanvasGrid => (0x2b2e36ff, 0xc3c6d0ff),
            Self::PixelGrid => (0x808080ff, 0x808080ff),
            Self::Handle => (0xffffffff, 0xffffffff),
            Self::Checker => (0xd9dce2ff, 0xd0d4dfff),
            Self::Transparent => (0, 0),
            Self::Component => (0xf28bd9ff, 0x9b3287ff),
            Self::Guide => (0xff4466ff, 0xd21f4aff),
            Self::Shadow => (0x00000040, 0x161b3024),
        };
        gpui::rgba(if is_dark() { dark } else { light })
    }
}
