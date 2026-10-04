use super::artboard::Rect;
use gpui::Rgba;

mod paint;
pub(crate) use paint::paint_shadows;

pub(crate) const MAX_SHADOWS: usize = 8;
pub(crate) const MAX_RADIUS: f32 = 256.;
pub(crate) const MAX_OFFSET: f32 = 4096.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShadowKind {
    #[default]
    Drop,
    Inner,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Shadow {
    pub kind: ShadowKind,
    pub enabled: bool,
    pub x: f32,
    pub y: f32,
    /// Blur diameter; Gaussian standard deviation is half this value.
    pub blur: f32,
    pub spread: f32,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            kind: ShadowKind::Drop,
            enabled: true,
            x: 0.,
            y: 4.,
            blur: 8.,
            spread: 0.,
            color: gpui::rgba(0x00000040),
        }
    }
}

impl Shadow {
    pub fn visible(&self) -> bool {
        self.enabled && self.color.a > 0.
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            [self.x, self.y]
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= MAX_OFFSET)
                && self.blur.is_finite()
                && (0. ..=MAX_RADIUS).contains(&self.blur)
                && self.spread.is_finite()
                && self.spread.abs() <= MAX_RADIUS
                && [self.color.r, self.color.g, self.color.b, self.color.a]
                    .into_iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(&v)),
            "Invalid shadow"
        );
        Ok(())
    }

    pub fn padding(&self) -> f32 {
        let spread = match self.kind {
            ShadowKind::Drop => self.spread,
            ShadowKind::Inner => -self.spread,
        };
        self.x.abs().max(self.y.abs()) + spread.max(0.) + self.blur * 1.5 + 2.
    }
}

pub(crate) fn bounds(rect: Rect, shadows: &[Shadow]) -> Rect {
    let (mut l, mut t, mut r, mut b) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    for shadow in shadows
        .iter()
        .filter(|s| s.visible() && s.kind == ShadowKind::Drop)
    {
        let pad = shadow.spread.max(0.) + shadow.blur * 1.5 + 1.;
        l = l.min(rect.x + shadow.x - pad);
        t = t.min(rect.y + shadow.y - pad);
        r = r.max(rect.x + rect.width + shadow.x + pad);
        b = b.max(rect.y + rect.height + shadow.y + pad);
    }
    Rect {
        x: l,
        y: t,
        width: r - l,
        height: b - t,
    }
}
