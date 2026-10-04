//! Shared application UI. This module must not depend on the editor or application shell.

pub(crate) mod assets;
pub(crate) mod font;
pub(crate) mod font_picker;
pub(crate) mod shortcuts;
pub(crate) mod theme;
pub(crate) mod titlebar;

use gpui::{Styled, px, svg};
use uic::assets::LucideIcons;

pub(crate) const PANEL: theme::Color = theme::Color::Panel;
pub(crate) const WORKSPACE: theme::Color = theme::Color::Workspace;
pub(crate) const BORDER: theme::Color = theme::Color::Border;
pub(crate) const TEXT: theme::Color = theme::Color::Text;
pub(crate) const MUTED: theme::Color = theme::Color::Muted;
pub(crate) const ACCENT: theme::Color = theme::Color::Accent;

pub(crate) fn icon(glyph: LucideIcons, size: f32) -> gpui::Svg {
    svg().path(glyph).size(px(size)).text_color(TEXT.color())
}
