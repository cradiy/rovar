use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Source {
    Image {
        source: ImageSource,
    },
    Svg {
        source: SvgSource,
        /// False preserves a vector asset. True requires lossless editable conversion.
        #[serde(default)]
        editable: bool,
    },
    Video {
        path: String,
    },
    Audio {
        path: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ImageSource {
    Path {
        path: String,
    },
    /// Standard base64 of the encoded image file, without a data URI prefix.
    Data {
        base64: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SvgSource {
    Path {
        path: String,
    },
    /// Complete SVG XML, without a data URI prefix.
    Data {
        svg: String,
    },
}
