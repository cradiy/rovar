use crate::scene::{
    artboard::{Artboard, FillMode, LinearGradient, Rect},
    image_fill::ImageFill,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(unix, windows), derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct Paint {
    /// Color in #RRGGBB or #RRGGBBAA format.
    pub color: String,
    pub mode: FillMode,
    pub gradient: LinearGradient,
    pub image: ImageFill,
    pub image_hash: Option<String>,
}

impl Paint {
    pub fn new(
        color: gpui::Rgba,
        mode: FillMode,
        gradient: LinearGradient,
        image: ImageFill,
    ) -> Box<Self> {
        Box::new(Self {
            color: super::property::hex(color),
            mode,
            gradient,
            image_hash: image.asset.as_ref().map(|a| a.hash.clone()),
            image,
        })
    }

    pub fn valid(&self) -> bool {
        super::property::color(&self.color).is_some()
            && self.gradient.validate().is_ok()
            && self.gradient.stops().iter().all(|s| {
                [s.color.r, s.color.g, s.color.b, s.color.a]
                    .into_iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
            })
            && self.image.placement.validate().is_ok()
            && (0. ..=1.).contains(&self.image.opacity)
            && self
                .image_hash
                .as_ref()
                .is_none_or(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            && match self.mode {
                FillMode::Points(g) => g.validate().is_ok(),
                _ => true,
            }
    }

    // The inspector's existing paint field conversions operate on this detached value.
    pub fn board(&self) -> Artboard {
        Artboard {
            fill_enabled: true,
            fill_opacity: 1.,
            fill_hidden: false,
            fill_layers: Vec::new(),
            uid: uuid::Uuid::nil(),
            id: 0,
            name: String::new(),
            layer: Default::default(),
            color_style: None,
            rect: Rect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
            color: super::property::color(&self.color).unwrap(),
            fill_mode: self.mode,
            gradient: self.gradient.clone(),
            image_fill: self.image.clone(),
        }
    }

    pub fn from_board(board: &Artboard) -> Box<Self> {
        Self::new(
            board.color,
            board.fill_mode,
            board.gradient.clone(),
            board.image_fill.clone(),
        )
    }
}

#[cfg(test)]
impl From<&str> for Box<Paint> {
    fn from(value: &str) -> Self {
        Paint::new(
            super::property::color(value).unwrap(),
            FillMode::Solid,
            Default::default(),
            Default::default(),
        )
    }
}
