#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextProperty {
    Family,
    Color,
    Opacity,
    Size,
    LineHeight,
    Spacing,
    Weight,
    Align,
    FillMode,
    Gradient,
    VerticalAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}
impl Corner {
    pub fn index(self) -> usize {
        match self {
            Self::TopLeft => 0,
            Self::TopRight => 1,
            Self::BottomRight => 2,
            Self::BottomLeft => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Property {
    LayerOpacity,
    Name,
    X,
    Y,
    Width,
    Height,
    StartX,
    StartY,
    EndX,
    EndY,
    Color,
    Opacity,
    GradientAngle,
    GradientPosition,
    PointX,
    PointY,
    PointRadius,
    Radius,
    Vertices,
    InnerRadius,
    Corner(Corner),
    StrokeWidth,
    Rotation,
    FontSize,
    LineHeight,
    LetterSpacing,
    FontWeight,
    Gap,
    LineGap,
    Padding,
}
impl Property {
    /// Minimum, maximum, drag units per pixel, and drag quantization.
    pub fn numeric_limits(self) -> (f32, f32, f32, f32) {
        use super::artboard::{MAX_SIZE, MIN_SIZE};
        use Property::*;
        match self {
            X | Y | StartX | StartY | EndX | EndY | Rotation => (-1_000_000., 1_000_000., 1., 1.),
            Width | Height => (MIN_SIZE, MAX_SIZE, 1., 1.),
            Opacity | LayerOpacity | GradientPosition | PointX | PointY => (0., 100., 1., 1.),
            PointRadius => (1., 400., 1., 1.),
            GradientAngle => (0., 360., 1., 1.),
            FontSize => (1., 1000., 1., 1.),
            LineHeight => (0.5, 10., 0.01, 0.01),
            LetterSpacing => (-1000., 1000., 0.1, 0.1),
            FontWeight => (100., 900., 10., 100.),
            Vertices => (3., 60., 0.1, 1.),
            InnerRadius => (1., 99., 1., 1.),
            Radius | Corner(_) | StrokeWidth | Gap | LineGap | Padding => (0., MAX_SIZE, 1., 1.),
            Name | Color => unreachable!("non-numeric property"),
        }
    }

    pub fn accepts_number(self, value: f32) -> bool {
        let (min, max, _, _) = self.numeric_limits();
        value.is_finite() && (min..=max).contains(&value)
    }

    pub fn text_style(self) -> Option<TextProperty> {
        Some(match self {
            Self::Color => TextProperty::Color,
            Self::Opacity => TextProperty::Opacity,
            Self::FontSize => TextProperty::Size,
            Self::LineHeight => TextProperty::LineHeight,
            Self::LetterSpacing => TextProperty::Spacing,
            Self::FontWeight => TextProperty::Weight,
            Self::GradientAngle | Self::GradientPosition => TextProperty::Gradient,
            _ => return None,
        })
    }
    pub fn is_geometry(self) -> bool {
        matches!(self, Self::X | Self::Y | Self::Width | Self::Height)
    }
}
