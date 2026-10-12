use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Svg {
    pub width: u32,
    pub height: u32,
    pub size: [f32; 2],
    pub source: String,
    pub editable: Result<Vec<Node>, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node {
    Group {
        name: String,
        opacity: f32,
        children: Vec<Node>,
    },
    Path {
        name: String,
        color: [f32; 4],
        points: Vec<Point>,
        closed: bool,
        stroke_width: Option<f32>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Point {
    pub anchor: [f32; 2],
    pub incoming: [f32; 2],
    pub outgoing: [f32; 2],
}

#[cfg(feature = "parse")]
mod parse;
#[cfg(feature = "parse")]
pub use parse::parse;
