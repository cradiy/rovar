//! Additional paints are stored bottom-to-top, above the original paint.
use super::{
    artboard::{Artboard, FillMode, LinearGradient},
    image_fill::ImageFill,
    shape::Shape,
};
use gpui::{Background, Rgba};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub enabled: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default = "opacity")]
    pub opacity: f32,
    pub color_style: Option<String>,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub fill_mode: FillMode,
    pub gradient: LinearGradient,
    pub image_fill: ImageFill,
    pub image_hash: Option<String>,
}

impl Fill {
    pub fn from_shape(s: &Shape) -> Self {
        Self {
            enabled: s.fill_enabled,
            hidden: s.fill_hidden,
            opacity: s.fill_opacity,
            color_style: s.color_style.clone(),
            color: s.color,
            fill_mode: s.fill_mode,
            gradient: s.gradient.clone(),
            image_fill: s.image_fill.clone(),
            image_hash: s.image_fill.asset.as_ref().map(|a| a.hash.clone()),
        }
    }
    pub fn from_board(b: &Artboard) -> Self {
        Self {
            enabled: b.fill_enabled,
            hidden: b.fill_hidden,
            opacity: b.fill_opacity,
            color_style: b.color_style.clone(),
            color: b.color,
            fill_mode: b.fill_mode,
            gradient: b.gradient.clone(),
            image_fill: b.image_fill.clone(),
            image_hash: b.image_fill.asset.as_ref().map(|a| a.hash.clone()),
        }
    }
    pub fn apply_shape(&self, s: &mut Shape) {
        s.fill_enabled = self.enabled;
        s.fill_hidden = self.hidden;
        s.fill_opacity = self.opacity;
        s.color_style = self.color_style.clone();
        s.color = self.color;
        s.fill_mode = self.fill_mode;
        s.gradient = self.gradient.clone();
        s.image_fill = self.image_fill.clone();
    }
    pub fn apply_board(&self, b: &mut Artboard) {
        b.fill_enabled = self.enabled;
        b.fill_hidden = self.hidden;
        b.fill_opacity = self.opacity;
        b.color_style = self.color_style.clone();
        b.color = self.color;
        b.fill_mode = self.fill_mode;
        b.gradient = self.gradient.clone();
        b.image_fill = self.image_fill.clone();
    }
    pub fn background(&self) -> Background {
        if self.hidden {
            return gpui::rgba(0).into();
        }
        let background: Background = match self.fill_mode {
            FillMode::Solid => self.color.into(),
            FillMode::Linear => self.gradient.background(),
            _ => gpui::rgba(0).into(),
        };
        background.opacity(self.opacity)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        self.gradient.validate()?;
        anyhow::ensure!((0. ..=1.).contains(&self.opacity), "Invalid paint opacity");
        self.image_fill.placement.validate()?;
        anyhow::ensure!(
            [
                self.color.r,
                self.color.g,
                self.color.b,
                self.color.a,
                self.image_fill.opacity
            ]
            .into_iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(&v)),
            "Invalid paint layer"
        );
        anyhow::ensure!(
            self.image_hash
                .as_ref()
                .is_none_or(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid paint image"
        );
        if let FillMode::Points(g) = self.fill_mode {
            g.validate()?;
        }
        Ok(())
    }
    pub fn paint(
        &self,
        bounds: gpui::Bounds<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &gpui::App,
    ) {
        if !self.enabled || self.hidden {
            return;
        }
        match self.fill_mode {
            FillMode::Image => window.with_subtree_effect_chain(
                bounds,
                &[],
                self.image_fill.opacity * self.opacity,
                |window| self.image_fill.paint(bounds, window, cx),
            ),
            FillMode::Points(g) => point_opacity(g, self.opacity).paint(bounds, window),
            _ => window.paint_quad(gpui::fill(bounds, self.background())),
        }
    }
}

pub fn enabled() -> bool {
    true
}
pub fn opacity() -> f32 {
    1.
}
pub fn point_opacity(
    mut gradient: super::point_gradient::PointGradient,
    opacity: f32,
) -> super::point_gradient::PointGradient {
    for point in &mut gradient.points {
        point.color.a *= opacity;
    }
    gradient
}
