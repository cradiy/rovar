use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Trigger {
    Click,
    Hover,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    SetProperty {
        target: String,
        property: crate::scene::presentation::property::Property,
    },
    None,
    Navigate {
        target: String,
    },
    OpenOverlay {
        target: String,
        settings: OverlaySettings,
    },
    CloseOverlay,
    Back,
    /// Target is an available variant definition UUID, not a page node UUID.
    ChangeVariant {
        target: String,
    },
    Media {
        target: String,
        command: MediaCommand,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OverlaySettings {
    pub position: OverlayPosition,
    pub backdrop: bool,
    pub dismiss_outside: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OverlayPosition {
    Center,
    Bottom,
    Right,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Restart,
    SeekBy {
        seconds: i32,
    },
    SeekTo {
        seconds: u32,
    },
    /// Video only, 25 to 400 percent.
    Rate {
        percent: u16,
    },
    /// 0 to 100 percent.
    Volume {
        percent: u8,
    },
    Mute,
    Unmute,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum HoverExit {
    Restore,
    Keep,
    ChangeVariant { target: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Transition {
    Instant,
    /// Duration must be 50 to 2000 milliseconds.
    Dissolve {
        duration_ms: u16,
    },
}
