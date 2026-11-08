//! Prototype triggers and destinations. Frame handles are page-local; variant IDs are stable.
pub(crate) mod paint;
pub(crate) mod property;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    #[default]
    Click,
    Hover,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Action {
    Multiple {
        actions: Vec<Action>,
    },
    SetProperty {
        target: usize,
        property: property::Property,
    },
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
    ScrollTo {
        target: usize,
    },
    ChangeVariant {
        target: uuid::Uuid,
    },
    Media {
        target: usize,
        command: MediaCommand,
    },
}

impl Action {
    pub fn steps(&self) -> &[Self] {
        match self {
            Self::Multiple { actions } => actions,
            _ => std::slice::from_ref(self),
        }
    }

    pub fn steps_mut(&mut self) -> &mut [Self] {
        match self {
            Self::Multiple { actions } => actions,
            _ => std::slice::from_mut(self),
        }
    }

    pub fn into_steps(self) -> Vec<Self> {
        match self {
            Self::Multiple { actions } => actions,
            action => vec![action],
        }
    }

    pub fn from_steps(mut actions: Vec<Self>) -> Option<Self> {
        match actions.len() {
            0 => None,
            1 => actions.pop(),
            _ => Some(Self::Multiple { actions }),
        }
    }

    pub fn remap(&self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        self.remap_inner(&mut map)
    }

    fn remap_inner(&self, map: &mut dyn FnMut(usize) -> Option<usize>) -> Option<Self> {
        match self.clone() {
            Self::Multiple { actions } => Self::from_steps(
                actions
                    .iter()
                    .filter_map(|action| action.remap_inner(map))
                    .collect(),
            ),
            Self::SetProperty { target, property } => property
                .valid()
                .then_some(())
                .and_then(|_| map(target))
                .map(|target| Self::SetProperty { target, property }),
            Self::Navigate { target } => map(target).map(|target| Self::Navigate { target }),
            Self::ScrollTo { target } => map(target).map(|target| Self::ScrollTo { target }),
            Self::OpenOverlay { target, settings } => {
                map(target).map(|target| Self::OpenOverlay { target, settings })
            }
            Self::CloseOverlay => Some(self.clone()),
            Self::Back => Some(Self::Back),
            Self::ChangeVariant { target } => (!target.is_nil()).then(|| self.clone()),
            Self::Media { target, command } => command
                .valid()
                .then_some(())
                .and_then(|_| map(target))
                .map(|target| Self::Media { target, command }),
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(unix, windows), derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScrollAxis {
    #[default]
    None,
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxis {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }
    pub fn vertical(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
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
    pub fn supports(self, kind: super::shape::ShapeKind) -> bool {
        self.valid()
            && kind.is_playable()
            && !(kind == super::shape::ShapeKind::Audio && matches!(self, Self::Rate { .. }))
    }
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
#[cfg_attr(any(unix, windows), derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum WipeDirection {
    Left,
    #[default]
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Transition {
    #[default]
    Instant,
    Dissolve {
        duration_ms: u16,
    },
    Blur {
        duration_ms: u16,
    },
    Wipe {
        direction: WipeDirection,
        duration_ms: u16,
    },
}

impl Transition {
    pub fn is_default(&self) -> bool {
        *self == Self::Instant
    }

    pub fn valid(self) -> bool {
        self.duration_ms()
            .is_none_or(|duration| (50..=2000).contains(&duration))
    }

    pub fn duration_ms(self) -> Option<u16> {
        match self {
            Self::Instant => None,
            Self::Dissolve { duration_ms }
            | Self::Blur { duration_ms }
            | Self::Wipe { duration_ms, .. } => Some(duration_ms),
        }
    }

    pub fn with_duration(self, duration_ms: u16) -> Self {
        match self {
            Self::Instant => Self::Instant,
            Self::Dissolve { .. } => Self::Dissolve { duration_ms },
            Self::Blur { .. } => Self::Blur { duration_ms },
            Self::Wipe { direction, .. } => Self::Wipe {
                direction,
                duration_ms,
            },
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Interaction {
    #[serde(default, skip_serializing_if = "ScrollAxis::is_none")]
    pub scroll: ScrollAxis,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pass_through: bool,
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
    pub fn get(&self, trigger: Trigger) -> Option<Action> {
        match trigger {
            Trigger::Click => self.click.clone(),
            Trigger::Hover => self.hover.clone(),
        }
    }
    pub fn set(&mut self, trigger: Trigger, action: Option<Action>) {
        let empty = action.is_none();
        match trigger {
            Trigger::Click => self.click = action,
            Trigger::Hover => self.hover = action,
        }
        if empty {
            self.set_transition(trigger, Transition::Instant);
        }
    }
    pub fn transition(&self, trigger: Trigger) -> Transition {
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
    pub fn is_empty(&self) -> bool {
        self.click.is_none()
            && self.hover.is_none()
            && self.scroll.is_none()
            && !self.fixed
            && !self.pass_through
    }
    pub fn remap(&self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        let value = Self {
            click: self.click.as_ref().and_then(|a| a.remap(&mut map)),
            hover: self.hover.as_ref().and_then(|a| a.remap(&mut map)),
            ..self.clone()
        };
        (!value.is_empty()).then_some(value)
    }
}
