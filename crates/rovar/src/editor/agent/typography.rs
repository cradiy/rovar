use super::*;
use crate::{
    agent::protocol::typography as dto,
    document::{Page, Text},
    scene::{
        artboard::FillMode,
        text::{StyleChange, TextStyle},
    },
};
use std::ops::Range;

pub(super) fn from_properties(props: &p::Properties) -> dto::Style {
    dto::Style {
        font_family: props.font_family.clone(),
        font_size: props.font_size,
        font_weight: props.font_weight,
        italic: props.italic,
        underline: props.underline,
        strikethrough: props.strikethrough,
        line_height: props.line_height,
        letter_spacing: props.letter_spacing,
        text_align: props.text_align,
        vertical_align: props.vertical_align,
        paint: props.text_paint.clone(),
    }
}

fn describe_style(style: &TextStyle) -> Value {
    json!({"font_family":style.family,"font_size":style.size,"font_weight":style.weight,
        "italic":style.italic,"underline":style.underline,"strikethrough":style.strikethrough,
        "line_height":style.line_height,"letter_spacing":style.spacing,
        "text_align":format!("{:?}", style.align).to_lowercase(),
        "vertical_align":format!("{:?}", style.vertical_align).to_lowercase(),
        "paint":appearance::describe_paint(style.color,style.fill_mode,&style.gradient,&Default::default(),None)})
}

pub(super) fn describe(text: &Text) -> Value {
    let offsets: Vec<_> = text
        .content
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.content.len()))
        .collect();
    json!({"offset_unit":"unicode_scalar","length":offsets.len()-1,
        "default_style":describe_style(&text.styles.default),"default_color_style":text.styles.default.color_style,
        "runs":text.styles.runs.iter().map(|run|json!({
            "start":offsets.binary_search(&run.range.start).unwrap(),
            "end":offsets.binary_search(&run.range.end).unwrap(),"style":describe_style(&run.style),"color_style":run.style.color_style})).collect::<Vec<_>>()})
}

pub(super) fn range(content: &str, start: usize, end: usize) -> Result<Range<usize>> {
    let offsets: Vec<_> = content
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(content.len()))
        .collect();
    if start > end || end >= offsets.len() {
        return Err(Error::invalid("Text range is outside the current content"));
    }
    Ok(offsets[start]..offsets[end])
}

fn changes(page: &Page, style: &dto::Style) -> Result<Vec<StyleChange>> {
    for (value, min, max, name) in [
        (style.font_size, 1., 1000., "font_size"),
        (style.font_weight, 100., 900., "font_weight"),
        (style.line_height, 0.5, 10., "line_height"),
        (style.letter_spacing, -1000., 1000., "letter_spacing"),
    ] {
        if value.is_some_and(|v| !v.is_finite() || !(min..=max).contains(&v)) {
            return Err(Error::invalid(format!("Invalid {name}")));
        }
    }
    let mut result = Vec::new();
    if let Some(v) = &style.font_family {
        result.push(StyleChange::Family(v.clone().into()));
    }
    if let Some(v) = style.font_size {
        result.push(StyleChange::Size(v));
    }
    if let Some(v) = style.font_weight {
        result.push(StyleChange::Weight(v));
    }
    if let Some(v) = style.italic {
        result.push(StyleChange::Italic(v));
    }
    if let Some(v) = style.underline {
        result.push(StyleChange::Underline(v));
    }
    if let Some(v) = style.strikethrough {
        result.push(StyleChange::Strikethrough(v));
    }
    if let Some(v) = style.line_height {
        result.push(StyleChange::LineHeight(v));
    }
    if let Some(v) = style.letter_spacing {
        result.push(StyleChange::Spacing(v));
    }
    if let Some(v) = style.text_align {
        result.push(StyleChange::Align(match v {
            dto::Align::Left => gpui::TextAlign::Left,
            dto::Align::Center => gpui::TextAlign::Center,
            dto::Align::Right => gpui::TextAlign::Right,
        }));
    }
    if let Some(v) = style.vertical_align {
        result.push(StyleChange::VerticalAlign(match v {
            dto::VerticalAlign::Baseline => crate::scene::text::VerticalAlign::Baseline,
            dto::VerticalAlign::Top => crate::scene::text::VerticalAlign::Top,
            dto::VerticalAlign::Center => crate::scene::text::VerticalAlign::Center,
            dto::VerticalAlign::Bottom => crate::scene::text::VerticalAlign::Bottom,
        }));
    }
    if let Some(paint) = &style.paint {
        if !matches!(
            paint,
            p::appearance::Paint::Solid { .. } | p::appearance::Paint::Gradient { .. }
        ) {
            return Err(Error::invalid("Text paint must be solid or gradient"));
        }
        let fill = appearance::paint(page, paint)?;
        result.push(StyleChange::FillMode(fill.fill_mode));
        if fill.fill_mode == FillMode::Solid {
            result.extend([
                StyleChange::Color(fill.color),
                StyleChange::Opacity(fill.color.a),
            ]);
        } else {
            result.push(StyleChange::Gradient(fill.gradient));
        }
    }
    Ok(result)
}

fn apply_changes(text: &mut Text, range: Range<usize>, changes: &[StyleChange], whole: bool) {
    for change in changes {
        let range = if change.is_paragraph() && !whole {
            crate::scene::text::styles::paragraph_range(&text.content, range.clone())
        } else {
            range.clone()
        };
        text.styles.apply(range, change, whole);
    }
}

pub(super) fn whole(page: &mut Page, id: usize, props: &p::Properties) -> Result<()> {
    let mut style = from_properties(props);
    if style.paint.is_some() && props.fill.is_some() {
        return Err(Error::invalid("fill conflicts with text_paint"));
    }
    if let Some(color) = &props.fill {
        style.paint = Some(p::appearance::Paint::Solid {
            color: color.clone(),
        });
    }
    let changes = changes(page, &style)?;
    let text = page.texts.iter_mut().find(|t| t.id == id).unwrap();
    if let Some(content) = &props.content {
        if content.len() > 65536 {
            return Err(Error::invalid("Text exceeds 64 KiB"));
        }
        if content != &text.content {
            text.styles.replace(0..text.content.len(), content.len());
            text.content = content.clone();
        }
    }
    apply_changes(text, 0..text.content.len(), &changes, true);
    Ok(())
}

pub(super) fn edit(
    page: &mut Page,
    id: usize,
    start: usize,
    end: usize,
    content: Option<String>,
    style: Option<dto::Style>,
) -> Result<()> {
    model::editable(page, id)?;
    if content.is_none() && style.is_none() {
        return Err(Error::invalid("edit_text requires content or style"));
    }
    let changes = style
        .as_ref()
        .map(|s| changes(page, s))
        .transpose()?
        .unwrap_or_default();
    let text = page
        .texts
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| Error::invalid("edit_text requires a text node"))?;
    let mut range = range(&text.content, start, end)?;
    if let Some(content) = content {
        if text.content.len() - range.len() + content.len() > 65536 {
            return Err(Error::invalid("Text exceeds 64 KiB"));
        }
        if text.content[range.clone()] != content {
            text.styles.replace(range.clone(), content.len());
            text.content.replace_range(range.clone(), &content);
        }
        range.end = range.start + content.len();
    }
    if range.is_empty() && !changes.is_empty() && !text.content.is_empty() {
        return Err(Error::invalid(
            "Styling requires a nonempty range or an empty text box",
        ));
    }
    let whole = text.content.is_empty();
    apply_changes(text, range, &changes, whole);
    Ok(())
}
