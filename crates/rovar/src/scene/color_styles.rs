use crate::{
    document::Page,
    scene::artboard::{FillMode, LinearGradient},
};
use gpui::Rgba;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) type Palette = BTreeMap<String, ColorStyle>;
pub(crate) const DEFAULT_STYLE_COLOR: u32 = 0xb4a2ee;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ColorStyle {
    pub name: String,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub gradient: Option<LinearGradient>,
}

impl ColorStyle {
    pub fn background(&self) -> gpui::Background {
        self.gradient
            .as_ref()
            .map_or_else(|| self.color.into(), LinearGradient::background)
    }

    pub fn apply(&self, color: &mut Rgba, mode: &mut FillMode, gradient: &mut LinearGradient) {
        *color = self.color;
        *mode = if let Some(value) = &self.gradient {
            *gradient = value.clone();
            FillMode::Linear
        } else {
            FillMode::Solid
        };
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 200,
            "Color name must contain 1–200 characters"
        );
        anyhow::ensure!(
            [self.color.r, self.color.g, self.color.b, self.color.a]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "Invalid color"
        );
        if let Some(gradient) = &self.gradient {
            gradient.validate()?;
            anyhow::ensure!(
                gradient
                    .stops()
                    .iter()
                    .all(|s| [s.color.r, s.color.g, s.color.b, s.color.a]
                        .iter()
                        .all(|v| v.is_finite() && (0. ..=1.).contains(v))),
                "Invalid gradient color"
            );
        }
        Ok(())
    }
}

/// Materialize referenced colors so renderers and exports use the same values.
/// Removing a definition detaches its uses without changing their appearance.
pub(crate) fn resolve(page: &mut Page, palette: &Palette) {
    visit(page, |reference, color, mode, gradient| {
        if let Some(id) = reference.as_ref() {
            if let Some(style) = palette.get(id) {
                style.apply(color, mode, gradient);
            } else {
                *reference = None;
            }
        }
    });
}

pub(crate) fn visit(
    page: &mut Page,
    mut f: impl FnMut(&mut Option<String>, &mut Rgba, &mut FillMode, &mut LinearGradient),
) {
    for board in &mut page.boards {
        f(
            &mut board.color_style,
            &mut board.color,
            &mut board.fill_mode,
            &mut board.gradient,
        );
    }
    for shape in &mut page.shapes {
        f(
            &mut shape.color_style,
            &mut shape.color,
            &mut shape.fill_mode,
            &mut shape.gradient,
        );
        f(
            &mut shape.stroke.color_style,
            &mut shape.stroke.color,
            &mut shape.stroke.fill_mode,
            &mut shape.stroke.gradient,
        );
    }
    for text in &mut page.texts {
        for style in std::iter::once(&mut text.styles.default)
            .chain(text.styles.runs.iter_mut().map(|r| &mut r.style))
        {
            f(
                &mut style.color_style,
                &mut style.color,
                &mut style.fill_mode,
                &mut style.gradient,
            );
        }
    }
}
