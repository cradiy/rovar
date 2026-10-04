use super::artboard::Rect;
use gpui::Rgba;

pub(crate) mod backdrop;
mod paint;
pub(crate) use paint::{paint_effects, paint_shadows};

pub(crate) const MAX_EFFECTS: usize = 8;
pub(crate) const MAX_RADIUS: f32 = 256.;
pub(crate) const MAX_OFFSET: f32 = 4096.;

fn upgrade_published_shadow(value: &mut serde_json::Value) {
    if let Some(fields) = value.as_object_mut()
        && !fields.contains_key("type")
        && !fields.contains_key("kind")
    {
        fields.insert("type".into(), "shadow".into());
        fields.insert("kind".into(), "drop".into());
    }
}

pub(crate) fn deserialize_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::collections::BTreeMap<usize, Vec<Effect>>, D::Error> {
    use serde::Deserialize;
    let mut map =
        std::collections::BTreeMap::<usize, Vec<serde_json::Value>>::deserialize(deserializer)?;
    for effects in map.values_mut() {
        for effect in effects {
            upgrade_published_shadow(effect);
        }
    }
    map.into_iter()
        .map(|(id, effects)| {
            effects
                .into_iter()
                .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
                .collect::<Result<Vec<_>, _>>()
                .map(|effects| (id, effects))
        })
        .collect()
}

/// Normalize published shadow lists in component and synchronization baselines.
pub(crate) fn upgrade_json_effects(value: &mut serde_json::Value) {
    if !["boards", "shapes", "texts"]
        .iter()
        .all(|key| value.get(*key).is_some())
    {
        return;
    }
    let Some(hierarchy) = value.get_mut("hierarchy").and_then(|h| h.as_object_mut()) else {
        return;
    };
    if hierarchy.contains_key("effects") {
        return;
    }
    if let Some(mut effects) = hierarchy.remove("shadows") {
        if let Some(map) = effects.as_object_mut() {
            for list in map.values_mut().filter_map(|list| list.as_array_mut()) {
                for effect in list {
                    upgrade_published_shadow(effect);
                }
            }
        }
        hierarchy.insert("effects".into(), effects);
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Effect {
    Shadow(Shadow),
    LayerBlur { enabled: bool, radius: f32 },
    BackgroundBlur { enabled: bool, radius: f32 },
}

impl Default for Effect {
    fn default() -> Self {
        Self::Shadow(Shadow::default())
    }
}

impl Effect {
    pub fn shadow(&self) -> Option<&Shadow> {
        match self {
            Self::Shadow(shadow) => Some(shadow),
            Self::LayerBlur { .. } | Self::BackgroundBlur { .. } => None,
        }
    }

    pub fn shadow_mut(&mut self) -> Option<&mut Shadow> {
        match self {
            Self::Shadow(shadow) => Some(shadow),
            Self::LayerBlur { .. } | Self::BackgroundBlur { .. } => None,
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Self::Shadow(shadow) => shadow.enabled,
            Self::LayerBlur { enabled, .. } | Self::BackgroundBlur { enabled, .. } => *enabled,
        }
    }

    pub fn toggle(&mut self) {
        let enabled = match self {
            Self::Shadow(shadow) => &mut shadow.enabled,
            Self::LayerBlur { enabled, .. } | Self::BackgroundBlur { enabled, .. } => enabled,
        };
        *enabled = !*enabled;
    }

    pub fn visible(&self) -> bool {
        match self {
            Self::Shadow(shadow) => shadow.visible(),
            Self::LayerBlur { enabled, radius } | Self::BackgroundBlur { enabled, radius } => {
                *enabled && *radius > 0.
            }
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            Self::Shadow(shadow) => shadow.validate(),
            Self::LayerBlur { radius, .. } | Self::BackgroundBlur { radius, .. } => {
                anyhow::ensure!(
                    radius.is_finite() && (0. ..=MAX_RADIUS).contains(radius),
                    "Invalid blur radius"
                );
                Ok(())
            }
        }
    }
}

/// Independent Gaussian layer blurs compose by adding their variances.
pub(crate) fn blur_radius(effects: &[Effect]) -> f32 {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::LayerBlur {
                enabled: true,
                radius,
            } => Some(radius * radius),
            _ => None,
        })
        .sum::<f32>()
        .sqrt()
}

pub(crate) fn padding(effects: &[Effect]) -> f32 {
    effects
        .iter()
        .filter_map(Effect::shadow)
        .filter(|s| s.visible() && s.kind == ShadowKind::Drop)
        .map(Shadow::padding)
        .fold(0., f32::max)
        + blur_radius(effects) * 1.5
}

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

pub(crate) fn bounds(rect: Rect, effects: &[Effect]) -> Rect {
    let (mut l, mut t, mut r, mut b) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    for shadow in effects
        .iter()
        .filter_map(Effect::shadow)
        .filter(|s| s.visible() && s.kind == ShadowKind::Drop)
    {
        let pad = shadow.spread.max(0.) + shadow.blur * 1.5 + 1.;
        l = l.min(rect.x + shadow.x - pad);
        t = t.min(rect.y + shadow.y - pad);
        r = r.max(rect.x + rect.width + shadow.x + pad);
        b = b.max(rect.y + rect.height + shadow.y + pad);
    }
    let blur = blur_radius(effects) * 1.5;
    Rect {
        x: l - blur,
        y: t - blur,
        width: r - l + 2. * blur,
        height: b - t + 2. * blur,
    }
}
