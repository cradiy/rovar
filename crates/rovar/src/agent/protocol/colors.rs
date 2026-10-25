use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Target {
    /// Index into the returned bottom-to-top fills list, including hidden layers.
    Fill { index: usize },
    /// Index into the returned bottom-to-top strokes list, including hidden layers.
    Stroke { index: usize },
    /// Omit both offsets for the whole box. Otherwise use Unicode scalar offsets.
    Text {
        start: Option<usize>,
        end: Option<usize>,
    },
}
