//! Property controls, bindings, and paint editors for the current selection.

mod bindings;
mod fields;
#[cfg(test)]
mod fill_tests;
#[cfg(test)]
mod font_tests;
mod frame_presets;
mod image_fill;
mod name_input;
mod paint;
mod properties;
mod scrub;
mod state;
mod view;

use super::*;
use crate::i18n::t;
use crate::scene::artboard::{LinearGradient, MAX_SIZE, MIN_SIZE};
pub(super) use state::State;
use uic::components::{
    color_picker::{ColorPicker, ColorPickerAppearance},
    input::{Input, InputAppearance},
    popover::{Popover, PopoverPlacement},
};

pub(super) const PROPERTY_COUNT: usize = 18;
// Independent inputs for the sidebar, solid, gradient, and image editors.
pub(super) const PROPERTY_SURFACES: usize = 4;

pub(super) fn inspector_section(title: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .px(px(14.))
        .py(px(14.))
        .border_b_1()
        .border_color(rgb(BORDER))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .line_height(px(16.))
                .child(title),
        )
}

pub(super) fn property_caption(label: &'static str) -> Div {
    div().text_size(px(10.)).text_color(rgb(MUTED)).child(label)
}

pub(super) fn hex(color: gpui::Rgba) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8
    )
}

pub(super) fn icon_button(
    id: impl Into<gpui::SharedString>,
    label: &'static str,
    glyph: LucideIcons,
    active: bool,
) -> gpui::Stateful<Div> {
    let id = id.into();
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .size(px(30.))
        .flex_shrink_0()
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(0x353044)))
        .hover(|s| s.bg(rgb(BORDER)))
        .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
        .child(icon(glyph, 17.).text_color(rgb(if active { ACCENT } else { MUTED })))
}

pub(super) fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
