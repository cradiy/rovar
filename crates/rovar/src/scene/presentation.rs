//! Prototype triggers and destinations. Frame handles are page-local; variant IDs are stable.

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    #[default]
    Click,
    Hover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Action {
    Navigate {
        target: usize,
    },
    OpenOverlay {
        target: usize,
        #[serde(default)]
        settings: OverlaySettings,
    },
    CloseOverlay,
    Back,
    ChangeVariant {
        target: uuid::Uuid,
    },
    Media {
        target: usize,
        command: MediaCommand,
    },
}

impl Action {
    pub fn remap(self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        match self {
            Self::Navigate { target } => map(target).map(|target| Self::Navigate { target }),
            Self::OpenOverlay { target, settings } => {
                map(target).map(|target| Self::OpenOverlay { target, settings })
            }
            Self::CloseOverlay => Some(self),
            Self::Back => Some(Self::Back),
            Self::ChangeVariant { target } => (!target.is_nil()).then_some(self),
            Self::Media { target, command } => command
                .valid()
                .then_some(())
                .and_then(|_| map(target))
                .map(|target| Self::Media { target, command }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Restart,
    SeekBy { seconds: i32 },
    SeekTo { seconds: u32 },
    Rate { percent: u16 },
    Volume { percent: u8 },
    Mute,
    Unmute,
}

impl MediaCommand {
    pub fn valid(self) -> bool {
        match self {
            Self::Rate { percent } => (25..=400).contains(&percent),
            Self::Volume { percent } => percent <= 100,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OverlayPosition {
    #[default]
    Center,
    Bottom,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct OverlaySettings {
    pub position: OverlayPosition,
    pub backdrop: bool,
    pub dismiss_outside: bool,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            position: OverlayPosition::Center,
            backdrop: true,
            dismiss_outside: true,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum HoverExit {
    #[default]
    Restore,
    Keep,
    ChangeVariant {
        target: uuid::Uuid,
    },
}

impl HoverExit {
    pub fn is_default(&self) -> bool {
        *self == Self::Restore
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Transition {
    #[default]
    Instant,
    Dissolve {
        duration_ms: u16,
    },
}

impl Transition {
    pub fn is_default(&self) -> bool {
        *self == Self::Instant
    }

    pub fn valid(self) -> bool {
        match self {
            Self::Instant => true,
            Self::Dissolve { duration_ms } => (50..=2000).contains(&duration_ms),
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Interaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Action>,
    #[serde(default, skip_serializing_if = "HoverExit::is_default")]
    pub hover_exit: HoverExit,
    #[serde(default, skip_serializing_if = "Transition::is_default")]
    pub click_transition: Transition,
    #[serde(default, skip_serializing_if = "Transition::is_default")]
    pub hover_transition: Transition,
}

impl Interaction {
    pub fn get(self, trigger: Trigger) -> Option<Action> {
        match trigger {
            Trigger::Click => self.click,
            Trigger::Hover => self.hover,
        }
    }
    pub fn set(&mut self, trigger: Trigger, action: Option<Action>) {
        match trigger {
            Trigger::Click => self.click = action,
            Trigger::Hover => self.hover = action,
        }
        if action.is_none() {
            self.set_transition(trigger, Transition::Instant);
        }
    }
    pub fn transition(self, trigger: Trigger) -> Transition {
        match trigger {
            Trigger::Click => self.click_transition,
            Trigger::Hover => self.hover_transition,
        }
    }
    pub fn set_transition(&mut self, trigger: Trigger, transition: Transition) {
        match trigger {
            Trigger::Click => self.click_transition = transition,
            Trigger::Hover => self.hover_transition = transition,
        }
    }
    pub fn is_empty(self) -> bool {
        self.click.is_none() && self.hover.is_none()
    }
    pub fn remap(self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        let value = Self {
            click: self.click.and_then(|a| a.remap(&mut map)),
            hover: self.hover.and_then(|a| a.remap(&mut map)),
            ..self
        };
        (!value.is_empty()).then_some(value)
    }
}
