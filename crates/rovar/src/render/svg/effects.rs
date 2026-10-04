use crate::scene::{
    artboard::Rect,
    effects::{Effect, Shadow, ShadowKind},
};
use std::fmt::Write;

pub(super) fn filter(defs: &mut String, id: usize, rect: Rect, effects: &[Effect]) -> bool {
    if !effects
        .iter()
        .any(|e| e.visible() && !matches!(e, Effect::BackgroundBlur { .. }))
    {
        return false;
    }
    let shadows: Vec<_> = effects.iter().filter_map(Effect::shadow).collect();
    let blur = crate::scene::effects::blur_radius(effects);
    // Intermediate dilation and blur must survive outside the original contour
    // before offsetting. Inner shadows are clipped back by SourceAlpha.
    let padding = shadows
        .iter()
        .filter(|s| s.visible())
        .map(|s| Shadow::padding(s))
        .fold(0., f32::max)
        + blur * 1.5;
    let bounds = Rect {
        x: rect.x - padding,
        y: rect.y - padding,
        width: rect.width + padding * 2.,
        height: rect.height + padding * 2.,
    };
    write!(defs, "<filter id=\"shadow-{id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\">", bounds.x, bounds.y, bounds.width, bounds.height).unwrap();
    for (i, s) in shadows.iter().enumerate().filter(|(_, s)| s.visible()) {
        let spread = if s.kind == ShadowKind::Inner {
            -s.spread
        } else {
            s.spread
        };
        if s.spread != 0. {
            write!(defs, "<feMorphology in=\"SourceAlpha\" operator=\"{}\" radius=\"{}\" result=\"spread-{i}\"/>", if spread > 0. { "dilate" } else { "erode" }, spread.abs()).unwrap();
        }
        let input = if s.spread == 0. {
            "SourceAlpha".into()
        } else {
            format!("spread-{i}")
        };
        write!(defs, "<feGaussianBlur in=\"{input}\" stdDeviation=\"{}\"/><feOffset dx=\"{}\" dy=\"{}\" result=\"alpha-{i}\"/>", s.blur / 2., s.x, s.y).unwrap();
        if s.kind == ShadowKind::Inner {
            write!(defs, "<feComposite in=\"SourceAlpha\" in2=\"alpha-{i}\" operator=\"out\" result=\"alpha-{i}\"/>").unwrap();
        }
        write!(defs, "<feFlood flood-color=\"#{:02x}{:02x}{:02x}\" flood-opacity=\"{}\"/><feComposite in2=\"alpha-{i}\" operator=\"in\" result=\"shadow-{i}\"/>", (s.color.r * 255.).round() as u8, (s.color.g * 255.).round() as u8, (s.color.b * 255.).round() as u8, s.color.a).unwrap();
    }
    defs.push_str("<feMerge>");
    for (i, _) in shadows
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, s)| s.visible() && s.kind == ShadowKind::Drop)
    {
        write!(defs, "<feMergeNode in=\"shadow-{i}\"/>").unwrap();
    }
    defs.push_str("<feMergeNode in=\"SourceGraphic\"/>");
    for (i, _) in shadows
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, s)| s.visible() && s.kind == ShadowKind::Inner)
    {
        write!(defs, "<feMergeNode in=\"shadow-{i}\"/>").unwrap();
    }
    defs.push_str("</feMerge>");
    if blur > 0. {
        write!(defs, "<feGaussianBlur stdDeviation=\"{}\"/>", blur / 2.).unwrap();
    }
    defs.push_str("</filter>");
    true
}
