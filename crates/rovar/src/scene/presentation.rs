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
    Navigate { target: usize },
    Back,
    ChangeVariant { target: uuid::Uuid },
}

impl Action {
    pub fn remap(self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        match self {
            Self::Navigate { target } => map(target).map(|target| Self::Navigate { target }),
            Self::Back => Some(Self::Back),
            Self::ChangeVariant { target } => (!target.is_nil()).then_some(self),
        }
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Interaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Action>,
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
    }
    pub fn is_empty(self) -> bool {
        self.click.is_none() && self.hover.is_none()
    }
    pub fn remap(self, mut map: impl FnMut(usize) -> Option<usize>) -> Option<Self> {
        let value = Self {
            click: self.click.and_then(|a| a.remap(&mut map)),
            hover: self.hover.and_then(|a| a.remap(&mut map)),
        };
        (!value.is_empty()).then_some(value)
    }
}
