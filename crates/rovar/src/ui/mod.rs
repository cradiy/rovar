//! Shared application UI. This module must not depend on the editor or application shell.

pub(crate) mod assets;
pub(crate) mod font;
pub(crate) mod font_picker;
pub(crate) mod shortcuts;
pub(crate) mod titlebar;

use gpui::{Styled, px, rgb, svg};
use uic::assets::LucideIcons;

pub(crate) const PANEL: u32 = 0x1d2026;
pub(crate) const WORKSPACE: u32 = 0x15171c;
pub(crate) const BORDER: u32 = 0x30343d;
pub(crate) const TEXT: u32 = 0xdde0e8;
pub(crate) const MUTED: u32 = 0x959ba9;
pub(crate) const ACCENT: u32 = 0xb4a2ee;

pub(crate) fn icon(glyph: LucideIcons, size: f32) -> gpui::Svg {
    svg().path(glyph).size(px(size)).text_color(rgb(TEXT))
}
