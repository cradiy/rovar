use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub(crate) mod import;
pub(crate) mod interaction;

pub(crate) const MAX_OPERATIONS: usize = 128;
pub(crate) const MAX_RECEIPTS: usize = 256;
pub(crate) const MAX_RECEIPT_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_NODES: usize = 200;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub request_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub document_id: Option<String>,
    pub method: String,
    #[serde(default = "empty_object")]
    pub params: Value,
}
fn empty_object() -> Value {
    json!({})
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Error {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<usize>,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            operation: None,
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
}
pub(crate) type Result<T> = std::result::Result<T, Error>;

pub(crate) enum Reply {
    Ready(Value),
    PrepareImports(Vec<(usize, import::Source)>),
    Capture {
        job: Box<crate::render::export::Job>,
        scale: u32,
        metadata: Value,
    },
}
impl Reply {
    /// Rasterization is run on the background executor, after taking a UI snapshot.
    pub fn finish(self) -> Result<Value> {
        use base64::Engine as _;
        match self {
            Self::Ready(value) => Ok(value),
            Self::PrepareImports(_) => Err(Error::new(
                "INTERNAL",
                "Imports require background preparation",
            )),
            Self::Capture {
                job,
                scale,
                mut metadata,
            } => {
                let options = crate::render::svg::render_options(!job.text.is_empty())
                    .map_err(|e| Error::new("CAPTURE_FAILED", e.to_string()))?;
                let bytes = job
                    .render(crate::document::export::Format::Png, scale, &options)
                    .map_err(|e| Error::new("CAPTURE_FAILED", e.to_string()))?;
                metadata["data_base64"] =
                    json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                Ok(metadata)
            }
        }
    }
}
pub(crate) fn response(request: &Request, result: Result<Value>) -> Value {
    match result {
        Ok(value) => json!({"request_id":request.request_id,"result":value}),
        Err(error) => json!({"request_id":request.request_id,"error":error}),
    }
}
pub(crate) fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::invalid(e.to_string()))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Empty {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schema {
    /// Method whose input schema is needed; omit for the request envelope.
    pub method: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    pub page_id: String,
    /// Must match the current document revision, including when paging results.
    pub revision: String,
    /// Return these UUIDs, or all nodes if omitted. Names are never IDs.
    #[serde(default)]
    pub ids: Vec<String>,
    /// Case-insensitive substring match, for nodes.find only.
    pub name: Option<String>,
    /// Filter by the node type returned by nodes.get.
    pub node_type: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    50
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl From<crate::scene::artboard::Rect> for Rect {
    fn from(r: crate::scene::artboard::Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}
impl From<Rect> for crate::scene::artboard::Rect {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NodeType {
    Frame,
    Rectangle,
    Ellipse,
    Text,
    Group,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layout {
    pub direction: Axis,
    pub gap: f32,
    /// Top, right, bottom, left in logical canvas units.
    pub padding: [f32; 4],
    pub main_align: Align,
    pub cross_align: Align,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Properties {
    pub name: Option<String>,
    /// Unrotated rectangle in page coordinates, independent of viewport zoom.
    pub rect: Option<Rect>,
    /// Solid sRGB color, exactly #RRGGBBAA. Text applies to the whole text box.
    pub fill: Option<String>,
    /// Layer opacity in [0, 1].
    pub opacity: Option<f32>,
    /// Clockwise degrees around the node center, not inherited by children.
    pub rotation: Option<f32>,
    pub hidden: Option<bool>,
    /// Uniform corner radius for rectangles.
    pub radius: Option<f32>,
    /// Replaces all text. Existing style runs are retained using editor semantics.
    pub content: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<f32>,
    /// Replaces the layout configuration. Only frames and groups support layout.
    pub layout: Option<Layout>,
    /// Explicitly remove auto layout; cannot be combined with layout.
    #[serde(default)]
    pub remove_layout: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Operation {
    Import {
        #[serde(rename = "ref")]
        reference: String,
        parent: Option<String>,
        /// Explicit destination in page coordinates.
        rect: Rect,
        name: Option<String>,
        source: import::Source,
    },
    /// Change one trigger without replacing the node's other interaction.
    SetInteraction {
        id: String,
        trigger: interaction::Trigger,
        action: interaction::Action,
        /// Omit to preserve the transition; clearing an action resets it.
        transition: Option<interaction::Transition>,
        /// Only valid for a nonempty hover action. Omit to preserve it.
        hover_exit: Option<interaction::HoverExit>,
    },
    /// Set the prototype starting frame; null restores the editor default.
    SetStartFrame {
        target: Option<String>,
    },
    Create {
        /// Unique batch-local reference, beginning with $. Later operations may use it.
        #[serde(rename = "ref")]
        reference: String,
        #[serde(rename = "type")]
        kind: NodeType,
        /// UUID, an earlier $reference, or null for the page root.
        parent: Option<String>,
        properties: Properties,
    },
    Update {
        id: String,
        properties: Properties,
    },
    /// Removes the entire subtree. Cannot delete locked or component-owned content.
    Delete {
        id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Apply {
    pub page_id: String,
    pub base_revision: String,
    pub operations: Vec<Operation>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Targets {
    pub page_id: String,
    pub revision: String,
    /// Explicit UUIDs, at most 200. Empty clears selection or fits all visible content.
    /// canvas.focus requires at least one ID.
    pub ids: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capture {
    pub page_id: String,
    pub revision: String,
    /// Render one node's subtree using the export renderer, without editor chrome.
    pub node_id: String,
    /// Integer raster scale, 1 through 4. Maximum output is 4 million pixels.
    #[serde(default = "one")]
    pub scale: u32,
}
fn one() -> u32 {
    1
}

pub(crate) fn schema(method: Option<&str>) -> Result<Value> {
    Ok(match method {
        None => json!(schemars::schema_for!(Request)),
        Some("session.describe") => json!(schemars::schema_for!(Empty)),
        Some("schema.get") => json!(schemars::schema_for!(Schema)),
        Some("nodes.get" | "nodes.find") => json!(schemars::schema_for!(Read)),
        Some("document.apply") => json!(schemars::schema_for!(Apply)),
        Some("canvas.capture") => json!(schemars::schema_for!(Capture)),
        Some("selection.set" | "canvas.focus" | "canvas.fit") => {
            json!(schemars::schema_for!(Targets))
        }
        Some(_) => return Err(Error::new("UNSUPPORTED", "Unknown method")),
    })
}
