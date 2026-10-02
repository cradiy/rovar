//! Persisted per-object export configuration, independent of the rendering backend.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Format {
    #[default]
    Png,
    Svg,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg => "SVG",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Preset {
    pub format: Format,
    pub scale: u32,
    pub suffix: String,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            format: Format::Png,
            scale: 1,
            suffix: String::new(),
        }
    }
}
