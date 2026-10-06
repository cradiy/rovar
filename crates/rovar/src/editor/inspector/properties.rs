use super::*;
use crate::scene::property::Property::*;

impl Workspace {
    // Slots belong to persistent UI inputs, not to document properties. Resolve
    // their meaning once at the inspector boundary before editing any object.
    pub(in crate::editor) fn field_property(&self, slot: usize) -> Option<Property> {
        if slot == 18 {
            return Some(LayerOpacity);
        }
        if !self.multi_selection.is_empty() {
            return match slot {
                1 => Some(X),
                2 => Some(Y),
                3 => Some(Width),
                4 => Some(Height),
                5 => Some(Color),
                6 => Some(Opacity),
                _ => None,
            };
        }
        if self.selected_text.is_some() {
            return match slot {
                1 => Some(X),
                2 => Some(Y),
                3 => Some(Width),
                4 => Some(Height),
                5 => Some(Color),
                6 => Some(Opacity),
                7 => Some(FontSize),
                8 => Some(LineHeight),
                9 => Some(LetterSpacing),
                10 => Some(Property::FontWeight),
                12 => Some(GradientAngle),
                13 => Some(GradientPosition),
                15 => Some(Rotation),
                _ => None,
            };
        }
        if let Some(shape) = self.selected_shape() {
            if shape.kind.is_line() {
                match slot {
                    1 => return Some(StartX),
                    2 => return Some(StartY),
                    3 => return Some(EndX),
                    4 => return Some(EndY),
                    _ => {}
                }
            }
            if shape.kind.supports_corners() {
                match slot {
                    9 => return Some(Radius),
                    10 => return Some(Corner(crate::scene::property::Corner::TopLeft)),
                    11 => return Some(Corner(crate::scene::property::Corner::TopRight)),
                    12 => return Some(Corner(crate::scene::property::Corner::BottomRight)),
                    13 => return Some(Corner(crate::scene::property::Corner::BottomLeft)),
                    _ => {}
                }
            }
            match slot {
                9 if shape.kind.is_polygon() => return Some(Vertices),
                10 if shape.kind == ShapeKind::Star => return Some(InnerRadius),
                14 => return Some(StrokeWidth),
                15 => return Some(Rotation),
                16 => return Some(Color),
                17 => return Some(Opacity),
                _ => {}
            }
        }
        match slot {
            0 => Some(Name),
            1 => Some(X),
            2 => Some(Y),
            3 => Some(Width),
            4 => Some(Height),
            5 => Some(Color),
            6 => Some(Opacity),
            7 => Some(GradientAngle),
            8 => Some(GradientPosition),
            15 => Some(Rotation),
            _ => None,
        }
    }
}

pub(super) fn apply_shape_field(
    shape: &mut Shape,
    stroke: bool,
    stop: usize,
    property: Property,
    text: &str,
) -> bool {
    if property == Name {
        shape.name = text.to_owned();
        return true;
    }
    if property == Color {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(value) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        let Some(target) = shape.paint_color_mut(stop, stroke) else {
            return false;
        };
        let alpha = target.a;
        *target = rgb(value);
        target.a = alpha;
        return true;
    }
    let Ok(value) = text.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    if matches!(property, StartX | StartY | EndX | EndY) {
        if value.abs() > 1_000_000. {
            return false;
        }
        let end = usize::from(matches!(property, EndX | EndY));
        let pivot = crate::scene::rotation::center(shape.rect);
        let mut p = shape.display_path_point(end);
        if matches!(property, StartX | EndX) {
            p.x = value;
        } else {
            p.y = value;
        }
        shape.set_endpoint(
            end,
            crate::scene::rotation::around(p, pivot, -shape.layer.rotation),
        );
        shape.preserve_rotation_pivot(pivot);
        return true;
    }
    match property {
        X if value.abs() <= 1_000_000. => shape.rect.x = value,
        Y if value.abs() <= 1_000_000. => shape.rect.y = value,
        Width | Height => {
            return editing::set_dimension(
                &mut shape.rect,
                property,
                value,
                shape.layer.aspect_locked,
            );
        }
        Opacity
            if !stroke && shape.fill_mode == FillMode::Image && (0. ..=100.).contains(&value) =>
        {
            shape.image_fill.opacity = value / 100.
        }
        Opacity if (0. ..=100.).contains(&value) => {
            let Some(target) = shape.paint_color_mut(stop, stroke) else {
                return false;
            };
            target.a = value / 100.;
        }
        GradientAngle if (0. ..=360.).contains(&value) => {
            shape.paint_gradient_mut(stroke).angle = value
        }
        GradientPosition if (0. ..=100.).contains(&value) => {
            return shape
                .paint_gradient_mut(stroke)
                .set_position(stop, value / 100.);
        }
        Radius if shape.kind.supports_corners() && (0. ..=MAX_SIZE / 2.).contains(&value) => {
            shape.radius = value
        }
        Vertices
            if shape.kind.is_polygon() && (3. ..=60.).contains(&value) && value.fract() == 0. =>
        {
            shape.vertices = value as usize
        }
        InnerRadius if shape.kind == ShapeKind::Star && (1. ..=99.).contains(&value) => {
            shape.inner_radius = value / 100.
        }
        Corner(corner)
            if shape.kind.supports_corners() && (0. ..=MAX_SIZE / 2.).contains(&value) =>
        {
            shape.corners.get_or_insert([shape.radius; 4])[corner.index()] = value;
        }
        StrokeWidth if (0. ..=MAX_SIZE / 2.).contains(&value) => shape.stroke.width = value,
        _ => return false,
    }
    true
}

pub(super) fn apply_text_field(
    text: &mut TextBox,
    property: Property,
    value: &str,
    stop: usize,
    cx: &mut Context<Workspace>,
) -> bool {
    if property == Color {
        let hex = value.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(color) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        text.editor.update(cx, |editor, cx| {
            editor.apply_color(rgb(color), stop, false, cx)
        });
        return true;
    }
    let Ok(value) = value.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    match property {
        X if value.abs() <= 1_000_000. => text.rect.x = value,
        Y if value.abs() <= 1_000_000. => text.rect.y = value,
        Width | Height => {
            return editing::set_dimension(
                &mut text.rect,
                property,
                value,
                text.layer.aspect_locked,
            );
        }
        Opacity if (0. ..=100.).contains(&value) => text.editor.update(cx, |editor, cx| {
            let mut color = editor.effective_style().editable_color(stop);
            color.a = value / 100.;
            editor.apply_color(color, stop, true, cx);
        }),
        FontSize if (1. ..=1000.).contains(&value) => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Size(value), cx)),
        LineHeight if (0.5..=5.).contains(&value) => text.editor.update(cx, |e, cx| {
            e.apply_style(StyleChange::LineHeight(value), cx)
        }),
        LetterSpacing if (0. ..=200.).contains(&value) => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Spacing(value), cx)),
        Property::FontWeight if (100. ..=900.).contains(&value) && value % 100. == 0. => text
            .editor
            .update(cx, |e, cx| e.apply_style(StyleChange::Weight(value), cx)),
        GradientAngle if (0. ..=360.).contains(&value) => text.editor.update(cx, |e, cx| {
            let mut gradient = e.effective_style().gradient.clone();
            gradient.angle = value;
            e.apply_style(StyleChange::Gradient(gradient), cx);
        }),
        GradientPosition if (0. ..=100.).contains(&value) => text.editor.update(cx, |e, cx| {
            let mut gradient = e.effective_style().gradient.clone();
            gradient.set_position(stop, value / 100.);
            e.apply_style(StyleChange::Gradient(gradient), cx);
        }),
        _ => return false,
    }
    true
}

pub(super) fn apply_field(
    board: &mut Artboard,
    stop: usize,
    property: Property,
    text: &str,
) -> bool {
    if property == Name {
        board.name = text.to_owned();
        return true;
    }
    if property == Color {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(value) = u32::from_str_radix(hex, 16) else {
            return false;
        };
        let Some(target) = board.editable_color_mut(stop) else {
            return false;
        };
        let alpha = target.a;
        *target = rgb(value);
        target.a = alpha;
        return true;
    }
    let Ok(value) = text.trim().parse::<f32>() else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    match property {
        X | Y if value.abs() <= 1_000_000. => {
            if property == X {
                board.rect.x = value;
            } else {
                board.rect.y = value;
            }
        }
        Width | Height if (MIN_SIZE..=MAX_SIZE).contains(&value) => {
            return editing::set_dimension(
                &mut board.rect,
                property,
                value,
                board.layer.aspect_locked,
            );
        }
        Opacity if board.fill_mode == FillMode::Image && (0. ..=100.).contains(&value) => {
            board.image_fill.opacity = value / 100.
        }
        Opacity if (0. ..=100.).contains(&value) => {
            let Some(target) = board.editable_color_mut(stop) else {
                return false;
            };
            target.a = value / 100.;
        }
        GradientAngle if (0. ..=360.).contains(&value) => board.gradient.angle = value,
        GradientPosition if (0. ..=100.).contains(&value) => {
            return board.gradient.set_position(stop, value / 100.);
        }
        _ => return false,
    }
    true
}
