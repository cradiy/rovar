use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn one() -> f32 {
    1.
}
fn angle() -> f32 {
    90.
}
fn midpoint() -> f32 {
    0.5
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fill {
    pub paint: Paint,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stroke {
    pub paint: Paint,
    #[serde(default = "one")]
    pub width: f32,
    #[serde(default)]
    pub align: StrokeAlign,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GradientKind {
    #[default]
    Linear,
    Radial,
    Angular,
    Diamond,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stop {
    /// Omit for new stops. Retain IDs returned by reads when editing an existing gradient.
    pub id: Option<usize>,
    pub position: f32,
    pub color: String,
    #[serde(default = "midpoint")]
    pub midpoint: f32,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Paint {
    Solid {
        color: String,
    },
    Gradient {
        #[serde(default)]
        kind: GradientKind,
        #[serde(default = "angle")]
        angle: f32,
        /// Stops must be sorted by position.
        stops: Vec<Stop>,
        #[serde(default = "crate::scene::artboard::default_seam_width")]
        seam_width: f32,
    },
    Points {
        gradient: crate::scene::point_gradient::PointGradient,
    },
    Image {
        /// Hash of an image/SVG already imported on this page, including earlier in this batch.
        hash: String,
        #[serde(default)]
        fit: crate::scene::image_fill::ImageFit,
        #[serde(default)]
        placement: crate::scene::image_fill::Placement,
        #[serde(default = "one")]
        opacity: f32,
    },
}
