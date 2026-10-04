use crate::scene::{artboard::Rect, effects::Shadow};
use std::fmt::Write;

pub(super) fn filter(defs: &mut String, id: usize, rect: Rect, shadows: &[Shadow]) -> bool {
    if !shadows.iter().any(Shadow::visible) {
        return false;
    }
    let bounds = crate::scene::effects::bounds(rect, shadows);
    write!(defs, "<filter id=\"shadow-{id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\">", bounds.x, bounds.y, bounds.width, bounds.height).unwrap();
    for (i, s) in shadows.iter().enumerate().filter(|(_, s)| s.visible()) {
        if s.spread != 0. {
            write!(defs, "<feMorphology in=\"SourceAlpha\" operator=\"{}\" radius=\"{}\" result=\"spread-{i}\"/>", if s.spread > 0. { "dilate" } else { "erode" }, s.spread.abs()).unwrap();
        }
        let input = if s.spread == 0. {
            "SourceAlpha".into()
        } else {
            format!("spread-{i}")
        };
        write!(defs, "<feGaussianBlur in=\"{input}\" stdDeviation=\"{}\"/><feOffset dx=\"{}\" dy=\"{}\" result=\"alpha-{i}\"/><feFlood flood-color=\"#{:02x}{:02x}{:02x}\" flood-opacity=\"{}\"/><feComposite in2=\"alpha-{i}\" operator=\"in\" result=\"shadow-{i}\"/>", s.blur / 2., s.x, s.y, (s.color.r * 255.).round() as u8, (s.color.g * 255.).round() as u8, (s.color.b * 255.).round() as u8, s.color.a).unwrap();
    }
    defs.push_str("<feMerge>");
    for (i, _) in shadows
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, s)| s.visible())
    {
        write!(defs, "<feMergeNode in=\"shadow-{i}\"/>").unwrap();
    }
    defs.push_str("<feMergeNode in=\"SourceGraphic\"/></feMerge></filter>");
    true
}
