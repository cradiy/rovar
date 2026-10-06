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
pub(crate) const DISCLOSURE_DURATION: std::time::Duration = std::time::Duration::from_millis(260);

pub(crate) fn disclosure_icon(
    id: impl Into<gpui::ElementId>,
    expanded: bool,
    size: f32,
    color: gpui::Rgba,
) -> gpui_effects::AnimatedNumber {
    gpui_effects::animated_number(id, if expanded { 1. } else { 0. }, move |progress| {
        icon(LucideIcons::ChevronRight, size)
            .text_color(color)
            .with_transformation(gpui::Transformation::rotate(gpui::radians(
                progress as f32 * std::f32::consts::FRAC_PI_2,
            )))
    })
    .duration(DISCLOSURE_DURATION)
}

pub(crate) fn icon(glyph: LucideIcons, size: f32) -> gpui::Svg {
    svg().path(glyph).size(px(size)).text_color(TEXT.color())
}
