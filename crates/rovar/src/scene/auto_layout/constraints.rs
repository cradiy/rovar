use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Constraint {
    #[default]
    Start,
    End,
    Center,
    Stretch,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Constraints {
    pub horizontal: Constraint,
    pub vertical: Constraint,
    /// Geometry relative to the parent when the constraints were authored.
    pub rect: Rect,
    pub parent_size: [f32; 2],
}

impl Constraints {
    pub fn validate(self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.parent_size
                .iter()
                .all(|v| v.is_finite() && *v >= 1. && *v <= MAX_SIZE)
                && [self.rect.x, self.rect.y, self.rect.width, self.rect.height]
                    .iter()
                    .all(|v| v.is_finite())
                && self.rect.width >= 0.
                && self.rect.height >= 0.
                && self.rect.width <= MAX_SIZE
                && self.rect.height <= MAX_SIZE,
            "Invalid layout constraints"
        );
        Ok(())
    }

    pub(super) fn apply(self, style: &mut Style) {
        for (constraint, position, size, parent, start, end, dimension, margin) in [
            (
                self.horizontal,
                self.rect.x,
                self.rect.width,
                self.parent_size[0],
                &mut style.inset.left,
                &mut style.inset.right,
                &mut style.size.width,
                &mut style.margin.left,
            ),
            (
                self.vertical,
                self.rect.y,
                self.rect.height,
                self.parent_size[1],
                &mut style.inset.top,
                &mut style.inset.bottom,
                &mut style.size.height,
                &mut style.margin.top,
            ),
        ] {
            *start = auto();
            *end = auto();
            match constraint {
                Constraint::Start => *start = length(position),
                Constraint::End => *end = length(parent - position - size),
                Constraint::Center => {
                    *start = percent(0.5_f32);
                    *margin = length(position - parent / 2.);
                }
                Constraint::Stretch => {
                    *start = length(position);
                    *end = length(parent - position - size);
                    *dimension = auto();
                }
                Constraint::Scale => {
                    *start = percent(position / parent);
                    *dimension = percent(size / parent);
                }
            }
        }
    }
}
