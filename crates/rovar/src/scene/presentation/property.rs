use super::paint::Paint;
use crate::{
    document::Page,
    scene::{artboard::FillMode, auto_layout, components, text::StyleChange},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(unix, derive(schemars::JsonSchema))]
#[serde(
    tag = "property",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Property {
    Visible(Boolean),
    /// Layer opacity, from 0 to 1.
    Opacity(f32),
    /// Position in the containing frame's coordinates.
    X(f32),
    Y(f32),
    Width(f32),
    Height(f32),
    Rotation(f32),
    Fill(Box<Paint>),
    FillEnabled(Boolean),
    Stroke(Box<Paint>),
    StrokeWidth(f32),
    StrokeEnabled(Boolean),
    Radius(f32),
    Content(String),
    FontFamily(String),
    FontSize(f32),
    FontWeight(f32),
    LineHeight(f32),
    LetterSpacing(f32),
    TextAlign(Align),
    Gap(f32),
    LineGap(f32),
    /// Uniform padding on all four sides.
    Padding(f32),
    LayoutDirection(Direction),
    LayoutWrap(Boolean),
}

/// An explicit boolean value or a toggle of the current playback value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(unix, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Boolean {
    Toggle,
    #[serde(untagged)]
    Set(bool),
}

impl Boolean {
    fn resolve(self, current: bool) -> bool {
        match self {
            Self::Toggle => !current,
            Self::Set(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(unix, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(unix, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    Horizontal,
    Vertical,
    Grid,
}

pub(crate) fn color(value: &str) -> Option<gpui::Rgba> {
    let hex = value.strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(gpui::rgb(value)),
        8 => Some(gpui::rgba(value)),
        _ => None,
    }
}

pub(crate) fn hex(color: gpui::Rgba) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8,
        (color.a * 255.).round() as u8
    )
}

impl Property {
    pub fn paint(&self) -> Option<&Paint> {
        match self {
            Self::Fill(p) | Self::Stroke(p) => Some(p),
            _ => None,
        }
    }
    pub fn paint_mut(&mut self) -> Option<&mut Paint> {
        match self {
            Self::Fill(p) | Self::Stroke(p) => Some(p),
            _ => None,
        }
    }
    pub fn key(&self) -> &'static str {
        match self {
            Self::Visible(_) => "visible",
            Self::Opacity(_) => "opacity",
            Self::X(_) => "x",
            Self::Y(_) => "y",
            Self::Width(_) => "width",
            Self::Height(_) => "height",
            Self::Rotation(_) => "rotation",
            Self::Fill(_) => "fill",
            Self::FillEnabled(_) => "fill_enabled",
            Self::Stroke(_) => "stroke",
            Self::StrokeWidth(_) => "stroke_width",
            Self::StrokeEnabled(_) => "stroke_enabled",
            Self::Radius(_) => "radius",
            Self::Content(_) => "content",
            Self::FontFamily(_) => "font_family",
            Self::FontSize(_) => "font_size",
            Self::FontWeight(_) => "font_weight",
            Self::LineHeight(_) => "line_height",
            Self::LetterSpacing(_) => "letter_spacing",
            Self::TextAlign(_) => "text_align",
            Self::Gap(_) => "gap",
            Self::LineGap(_) => "line_gap",
            Self::Padding(_) => "padding",
            Self::LayoutDirection(_) => "layout_direction",
            Self::LayoutWrap(_) => "layout_wrap",
        }
    }

    pub fn valid(&self) -> bool {
        if let Some((property, value)) = self.numeric() {
            return property.accepts_number(value);
        }
        match self {
            Self::Fill(v) | Self::Stroke(v) => v.valid(),
            Self::FontFamily(v) => !v.trim().is_empty() && v.len() <= 200,
            Self::Content(v) => v.len() <= 65536,
            _ => true,
        }
    }

    /// Shared inspector property and its value in display units.
    pub fn numeric(&self) -> Option<(crate::scene::property::Property, f32)> {
        use crate::scene::property::Property as P;
        Some(match self {
            Self::Opacity(v) => (P::LayerOpacity, *v * 100.),
            Self::X(v) => (P::X, *v),
            Self::Y(v) => (P::Y, *v),
            Self::Width(v) => (P::Width, *v),
            Self::Height(v) => (P::Height, *v),
            Self::Rotation(v) => (P::Rotation, *v),
            Self::StrokeWidth(v) => (P::StrokeWidth, *v),
            Self::Radius(v) => (P::Radius, *v),
            Self::FontSize(v) => (P::FontSize, *v),
            Self::FontWeight(v) => (P::FontWeight, *v),
            Self::LineHeight(v) => (P::LineHeight, *v),
            Self::LetterSpacing(v) => (P::LetterSpacing, *v),
            Self::Gap(v) => (P::Gap, *v),
            Self::LineGap(v) => (P::LineGap, *v),
            Self::Padding(v) => (P::Padding, *v),
            _ => return None,
        })
    }

    pub fn supports(&self, page: &Page, id: usize) -> bool {
        if !self.valid_target(page, id) {
            return false;
        }
        match self {
            Self::X(_) | Self::Y(_) | Self::Width(_) | Self::Height(_) => {
                let geometry = !page.hierarchy.groups.contains_key(&id)
                    || page.hierarchy.layouts.contains_key(&id);
                geometry
                    && (!matches!(self, Self::X(_) | Self::Y(_))
                        || !components::parent(page, id).is_some_and(|parent| {
                            page.hierarchy.layouts.contains_key(&parent)
                                && !page.hierarchy.sizing.get(&id).is_some_and(|s| s.absolute)
                        }))
            }
            Self::Gap(_)
            | Self::LineGap(_)
            | Self::Padding(_)
            | Self::LayoutDirection(_)
            | Self::LayoutWrap(_) => page.hierarchy.layouts.contains_key(&id),
            _ => true,
        }
    }

    // Layout ownership can change after authoring without invalidating saved actions.
    pub fn valid_target(&self, page: &Page, id: usize) -> bool {
        let shape = page.shapes.iter().find(|s| s.id == id);
        let board = page.boards.iter().find(|s| s.id == id);
        let text = page.texts.iter().find(|s| s.id == id);
        let group = page.hierarchy.groups.get(&id);
        if shape.is_none() && board.is_none() && text.is_none() && group.is_none() {
            return false;
        }
        match self {
            Self::Rotation(_) => shape.is_some() || text.is_some(),
            Self::Fill(p) => {
                board.is_some()
                    || (text.is_some() && matches!(p.mode, FillMode::Solid | FillMode::Linear))
                    || shape.is_some_and(|s| {
                        s.can_fill()
                            && (!s.kind.is_asset() || !matches!(p.mode, FillMode::Points(_)))
                    })
            }
            Self::FillEnabled(_) => shape.is_some_and(|s| s.can_fill()),
            Self::Stroke(p) => {
                shape.is_some_and(|s| !s.kind.is_asset())
                    && matches!(p.mode, FillMode::Solid | FillMode::Linear)
            }
            Self::StrokeWidth(_) | Self::StrokeEnabled(_) => {
                shape.is_some_and(|s| !s.kind.is_asset())
            }
            Self::Radius(_) => shape.is_some_and(|s| s.kind.supports_corners()),
            Self::Content(_)
            | Self::FontFamily(_)
            | Self::FontSize(_)
            | Self::FontWeight(_)
            | Self::LineHeight(_)
            | Self::LetterSpacing(_)
            | Self::TextAlign(_) => text.is_some(),
            Self::Gap(_)
            | Self::LineGap(_)
            | Self::Padding(_)
            | Self::LayoutDirection(_)
            | Self::LayoutWrap(_) => board.is_some() || group.is_some(),
            _ => true,
        }
    }

    pub fn apply(&self, page: &mut Page, id: usize) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.valid() && self.supports(page, id),
            "Unsupported property or invalid value"
        );
        if let Some(layer) = page
            .boards
            .iter_mut()
            .find(|n| n.id == id)
            .map(|n| &mut n.layer)
            .or_else(|| {
                page.shapes
                    .iter_mut()
                    .find(|n| n.id == id)
                    .map(|n| &mut n.layer)
            })
            .or_else(|| {
                page.texts
                    .iter_mut()
                    .find(|n| n.id == id)
                    .map(|n| &mut n.layer)
            })
            .or_else(|| page.hierarchy.groups.get_mut(&id).map(|n| &mut n.layer))
        {
            match self {
                Self::Visible(v) => layer.hidden = !v.resolve(!layer.hidden),
                Self::Opacity(v) => layer.opacity = *v,
                Self::Rotation(v) => layer.rotation = crate::scene::rotation::normalize(*v),
                _ => (),
            }
        }
        let mut rects = Vec::new();
        if let Some(n) = page.boards.iter_mut().find(|n| n.id == id) {
            rects.push(&mut n.rect);
            if let Self::Fill(v) = self {
                n.color = color(&v.color).unwrap();
                n.color_style = None;
                n.fill_mode = v.mode;
                n.gradient = v.gradient.clone();
                n.image_fill = v.image.clone();
            }
        }
        if let Some(n) = page.shapes.iter_mut().find(|n| n.id == id) {
            rects.push(&mut n.rect);
            match self {
                Self::Fill(v) => {
                    n.color = color(&v.color).unwrap();
                    n.color_style = None;
                    n.fill_mode = v.mode;
                    n.gradient = v.gradient.clone();
                    n.image_fill = v.image.clone();
                    n.fill_enabled = true;
                }
                Self::FillEnabled(v) => n.fill_enabled = v.resolve(n.fill_enabled),
                Self::Stroke(v) => {
                    n.stroke.color = color(&v.color).unwrap();
                    n.stroke.color_style = None;
                    n.stroke.fill_mode = v.mode;
                    n.stroke.gradient = v.gradient.clone();
                    n.stroke.enabled = true;
                }
                Self::StrokeWidth(v) => {
                    n.stroke.width = *v;
                    n.stroke.enabled = true;
                }
                Self::StrokeEnabled(v) => n.stroke.enabled = v.resolve(n.stroke.enabled),
                Self::Radius(v) => {
                    n.radius = *v;
                    n.independent_corners = false;
                    n.corners = None;
                }
                _ => (),
            }
        }
        if let Some(n) = page.texts.iter_mut().find(|n| n.id == id) {
            rects.push(&mut n.rect);
            if let Self::Content(v) = self {
                n.styles.replace(0..n.content.len(), v.len());
                n.content = v.clone();
            }
            let change = match self {
                Self::FontFamily(v) => Some(StyleChange::Family(v.clone().into())),
                Self::FontSize(v) => Some(StyleChange::Size(*v)),
                Self::FontWeight(v) => Some(StyleChange::Weight(*v)),
                Self::LineHeight(v) => Some(StyleChange::LineHeight(*v)),
                Self::LetterSpacing(v) => Some(StyleChange::Spacing(*v)),
                Self::TextAlign(v) => Some(StyleChange::Align(match v {
                    Align::Left => gpui::TextAlign::Left,
                    Align::Center => gpui::TextAlign::Center,
                    Align::Right => gpui::TextAlign::Right,
                })),
                _ => None,
            };
            if let Some(change) = change {
                n.styles.apply(0..n.content.len(), &change, true);
            }
            if let Self::Fill(v) = self {
                let color = color(&v.color).unwrap();
                for change in [
                    StyleChange::Color(color),
                    StyleChange::Opacity(color.a),
                    StyleChange::FillMode(v.mode),
                    StyleChange::Gradient(v.gradient.clone()),
                ] {
                    n.styles.apply(0..n.content.len(), &change, true);
                }
            }
        }
        if let Some(n) = page.hierarchy.layouts.get_mut(&id) {
            rects.push(&mut n.frame);
            match self {
                Self::Gap(v) => n.gap = *v,
                Self::LineGap(v) => n.line_gap = *v,
                Self::Padding(v) => n.padding = [*v; 4],
                Self::LayoutWrap(v) => n.wrap = v.resolve(n.wrap),
                Self::LayoutDirection(v) => {
                    n.axis = match v {
                        Direction::Horizontal => auto_layout::Axis::Horizontal,
                        Direction::Vertical => auto_layout::Axis::Vertical,
                        Direction::Grid => auto_layout::Axis::Grid,
                    }
                }
                _ => (),
            }
        }
        for rect in rects {
            match self {
                Self::X(v) => rect.x = *v,
                Self::Y(v) => rect.y = *v,
                Self::Width(v) => rect.width = *v,
                Self::Height(v) => rect.height = *v,
                _ => (),
            }
        }
        // Explicit sizes take precedence over a target's hug/fill sizing.
        if matches!(self, Self::Width(_) | Self::Height(_)) {
            let sizing = page.hierarchy.sizing.entry(id).or_default();
            if matches!(self, Self::Width(_)) {
                sizing.width = auto_layout::Mode::Fixed;
            } else {
                sizing.height = auto_layout::Mode::Fixed;
            }
        }
        Ok(())
    }
}

pub(crate) fn defaults(page: &Page, id: usize) -> Vec<Property> {
    use Property::*;
    let shape = page.shapes.iter().find(|n| n.id == id);
    let board = page.boards.iter().find(|n| n.id == id);
    let text = page.texts.iter().find(|n| n.id == id);
    let layout = page.hierarchy.layouts.get(&id);
    let layer = shape
        .map(|n| n.layer)
        .or_else(|| board.map(|n| n.layer))
        .or_else(|| text.map(|n| n.layer))
        .or_else(|| page.hierarchy.groups.get(&id).map(|n| n.layer))
        .unwrap_or_default();
    let mut values = vec![
        Visible(Boolean::Set(!layer.hidden)),
        Opacity(layer.opacity),
        Rotation(layer.rotation),
    ];
    if let Some(rect) = shape
        .map(|n| n.rect)
        .or_else(|| board.map(|n| n.rect))
        .or_else(|| text.map(|n| n.rect))
        .or_else(|| layout.map(|n| n.frame))
    {
        values.extend([X(rect.x), Y(rect.y), Width(rect.width), Height(rect.height)]);
    }
    if let Some(n) = shape {
        values.extend([
            Fill(Paint::new(
                n.color,
                n.fill_mode,
                n.gradient.clone(),
                n.image_fill.clone(),
            )),
            FillEnabled(Boolean::Set(n.fill_enabled)),
            Stroke(Paint::new(
                n.stroke.color,
                n.stroke.fill_mode,
                n.stroke.gradient.clone(),
                Default::default(),
            )),
            StrokeWidth(n.stroke.width),
            StrokeEnabled(Boolean::Set(n.stroke.enabled)),
            Radius(n.radius),
        ]);
    }
    if let Some(n) = board {
        values.push(Fill(Paint::from_board(n)));
    }
    if let Some(n) = text {
        let s = &n.styles.default;
        values.extend([
            Fill(Paint::new(
                s.color,
                s.fill_mode,
                s.gradient.clone(),
                Default::default(),
            )),
            Content(n.content.clone()),
            FontFamily(s.family.to_string()),
            FontSize(s.size),
            FontWeight(s.weight),
            LineHeight(s.line_height),
            LetterSpacing(s.spacing),
            TextAlign(match s.align {
                gpui::TextAlign::Center => Align::Center,
                gpui::TextAlign::Right => Align::Right,
                _ => Align::Left,
            }),
        ]);
    }
    if let Some(n) = layout {
        values.extend([
            Gap(n.gap),
            LineGap(n.line_gap),
            Padding(n.padding[0]),
            LayoutWrap(Boolean::Set(n.wrap)),
            LayoutDirection(match n.axis {
                auto_layout::Axis::Horizontal => Direction::Horizontal,
                auto_layout::Axis::Vertical => Direction::Vertical,
                auto_layout::Axis::Grid => Direction::Grid,
            }),
        ]);
    }
    values.retain(|p| p.supports(page, id));
    values
}
