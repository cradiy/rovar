#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayerState {
    #[serde(default, skip_serializing_if = "super::blend::Mode::is_normal")]
    pub blend: super::blend::Mode,
    #[serde(
        default = "super::blend::opaque",
        skip_serializing_if = "super::blend::is_opaque"
    )]
    pub opacity: f32,
    pub locked: bool,
    pub hidden: bool,
    pub aspect_locked: bool,
    /// Clockwise degrees about the object's center; never inherited by children.
    pub rotation: f32,
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            blend: Default::default(),
            opacity: 1.,
            locked: false,
            hidden: false,
            aspect_locked: false,
            rotation: 0.,
        }
    }
}

impl LayerState {
    pub fn composited(self) -> bool {
        !self.blend.is_normal() || self.opacity != 1.
    }
    pub fn editable(self) -> bool {
        !self.locked && !self.hidden
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LayerGroup {
    /// Direct child whose closed contour clips the other children.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean: Option<crate::scene::boolean::Operation>,
    #[serde(default, skip_serializing_if = "uuid::Uuid::is_nil")]
    pub uid: uuid::Uuid,
    pub name: String,
    pub board: Option<usize>,
    pub layer: LayerState,
}

/// Group membership is independent of the board coordinate system.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Hierarchy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub interactions: std::collections::BTreeMap<usize, super::presentation::Interaction>,
    #[serde(
        default,
        alias = "shadows",
        deserialize_with = "crate::scene::effects::deserialize_list",
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub effects: std::collections::BTreeMap<usize, Vec<crate::scene::effects::Effect>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub exports: std::collections::BTreeMap<usize, Vec<crate::document::export::Preset>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub components: std::collections::BTreeMap<usize, crate::scene::components::Binding>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub layouts: std::collections::BTreeMap<usize, crate::scene::auto_layout::Container>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sizing: std::collections::BTreeMap<usize, crate::scene::auto_layout::Sizing>,
    pub groups: std::collections::BTreeMap<usize, LayerGroup>,
    pub parents: std::collections::BTreeMap<usize, usize>,
    pub names: std::collections::BTreeMap<usize, String>,
    /// Bottom to top; newly created objects not in this list sort above it.
    pub order: Vec<usize>,
}

impl Hierarchy {
    pub fn retain_interactions(&mut self, ids: &std::collections::BTreeSet<usize>) {
        self.start = self.start.filter(|id| ids.contains(id));
        self.interactions.retain(|id, interaction| {
            if let Some(value) = interaction.remap(|target| ids.contains(&target).then_some(target))
            {
                *interaction = value;
                ids.contains(id)
            } else {
                false
            }
        });
    }
}
