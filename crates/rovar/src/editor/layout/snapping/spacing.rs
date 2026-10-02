use super::super::measurement::{Dimension, interval};
use super::*;

pub(super) struct Match {
    pub correction: f32,
    pub dimensions: [Dimension; 2],
}

// Neighbors are sorted once at drag start. Search only the closest objects in
// the moving object's row/column, not arbitrary gaps elsewhere on the page.
pub(super) fn nearest(
    neighbors: &[Rect],
    moving: Rect,
    axis: usize,
    tolerance: f32,
) -> Option<Match> {
    let (start, end) = interval(moving, axis);
    let (low, high) = interval(moving, 1 - axis);
    let mut before = [None; 2];
    let mut after = [None; 2];
    for &rect in neighbors {
        let (lo, hi) = interval(rect, 1 - axis);
        if lo.max(low) >= hi.min(high) {
            continue;
        }
        let (a, b) = interval(rect, axis);
        if b <= start + tolerance {
            before[0] = before[1];
            before[1] = Some(rect);
        } else if a >= end - tolerance {
            if after[0].is_none() {
                after[0] = Some(rect);
            } else if after[1].is_none() {
                after[1] = Some(rect);
            }
        } else {
            // Do not infer an empty gap through an overlapping sibling.
            return None;
        }
    }
    let mut best: Option<Match> = None;
    let mut consider = |a: Rect, b: Rect, position: usize| {
        let (a0, a1) = interval(a, axis);
        let (b0, b1) = interval(b, axis);
        if a1 > b0 {
            return;
        }
        let (alo, ahi) = interval(a, 1 - axis);
        let (blo, bhi) = interval(b, 1 - axis);
        let cross_low = low.max(alo).max(blo);
        let cross_high = high.min(ahi).min(bhi);
        if cross_low >= cross_high {
            return;
        }
        let width = end - start;
        let target = match position {
            0 => a0 - (b0 - a1) - width,
            1 => a1 + (b0 - a1 - width) / 2.,
            _ => b1 + (b0 - a1),
        };
        if position == 1 && b0 - a1 < width {
            return;
        }
        let correction = target - start;
        if correction.abs() > tolerance
            || best
                .as_ref()
                .is_some_and(|m| m.correction.abs() <= correction.abs())
        {
            return;
        }
        let spans = match position {
            0 => [(target + width, a0), (a1, b0)],
            1 => [(a1, target), (target + width, b0)],
            _ => [(a1, b0), (b1, target)],
        };
        best = Some(Match {
            correction,
            dimensions: spans.map(|(start, end)| Dimension {
                axis,
                start,
                end,
                cross: (cross_low + cross_high) / 2.,
            }),
        });
    };
    if let (Some(a), Some(b)) = (before[1], after[0]) {
        consider(a, b, 1);
    }
    if let (Some(a), Some(b)) = (before[0], before[1]) {
        consider(a, b, 2);
    }
    if let (Some(a), Some(b)) = (after[0], after[1]) {
        consider(a, b, 0);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_vertical_gaps_extend_both_ends_and_ignore_other_columns() {
        let a = Rect {
            x: 100.,
            y: 100.,
            width: 50.,
            height: 40.,
        };
        let b = Rect {
            y: 160.,
            height: 60.,
            ..a
        };
        for (y, expected) in [(43., -3.), (237., 3.)] {
            let moving = Rect { y, ..a };
            let result = nearest(&[a, b], moving, 1, 6.).unwrap();
            assert_eq!(result.correction, expected);
            assert_eq!(result.dimensions.map(|g| g.end - g.start), [20., 20.]);
            assert!(nearest(&[a, b], Rect { x: 200., ..moving }, 1, 6.).is_none());
        }
    }
}
