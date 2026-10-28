use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Member {
    /// Definition UUID or an earlier component reference, not a node UUID.
    pub component: String,
    pub name: String,
}
