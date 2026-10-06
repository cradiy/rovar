use crate::i18n::t;

/// Separable blend functions, evaluated in sRGB before source-over compositing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
}

impl Mode {
    pub const ALL: [Self; 6] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
    ];

    pub fn is_normal(&self) -> bool {
        *self == Self::Normal
    }

    pub fn css(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "blend-normal",
            Self::Multiply => "blend-multiply",
            Self::Screen => "blend-screen",
            Self::Overlay => "blend-overlay",
            Self::Darken => "blend-darken",
            Self::Lighten => "blend-lighten",
        }
    }

    pub fn label(self) -> &'static str {
        t(self.key())
    }
}

pub(crate) fn opaque() -> f32 {
    1.
}
pub(crate) fn is_opaque(value: &f32) -> bool {
    *value == 1.
}
