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
}
impl Property {
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
