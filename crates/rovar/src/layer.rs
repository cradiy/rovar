#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayerState {
    pub locked: bool,
    pub hidden: bool,
    pub aspect_locked: bool,
    /// Clockwise degrees about the object's center; never inherited by children.
    pub rotation: f32,
}

impl LayerState {
    pub fn editable(self) -> bool {
        !self.locked && !self.hidden
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LayerGroup {
    pub name: String,
    pub board: Option<usize>,
    pub layer: LayerState,
}

/// Group membership is independent of the board coordinate system.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Hierarchy {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub layouts: std::collections::BTreeMap<usize, crate::auto_layout::Container>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sizing: std::collections::BTreeMap<usize, crate::auto_layout::Sizing>,
    pub groups: std::collections::BTreeMap<usize, LayerGroup>,
    pub parents: std::collections::BTreeMap<usize, usize>,
    pub names: std::collections::BTreeMap<usize, String>,
    /// Bottom to top; newly created objects not in this list sort above it.
    pub order: Vec<usize>,
}
