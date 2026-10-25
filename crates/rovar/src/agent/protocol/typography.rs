use super::*;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerticalAlign {
    Baseline,
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Style {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<f32>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    /// Line-height multiplier.
    pub line_height: Option<f32>,
    /// Extra spacing in canvas units.
    pub letter_spacing: Option<f32>,
    pub text_align: Option<Align>,
    /// Alignment of differently sized runs on the same line, not box positioning.
    pub vertical_align: Option<VerticalAlign>,
    /// Solid or gradient paint. Omission preserves existing paint and bindings.
    pub paint: Option<appearance::Paint>,
}
