use super::{TextStyle, VerticalAlign};
use crate::artboard::{FillMode, LinearGradient};
use gpui::{Rgba, SharedString, TextAlign};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(crate) enum StyleChange {
    Family(SharedString),
    Weight(f32),
    Size(f32),
    Color(Rgba),
    Opacity(f32),
    LineHeight(f32),
    Spacing(f32),
    Align(TextAlign),
    VerticalAlign(VerticalAlign),
    FillMode(FillMode),
    Gradient(LinearGradient),
}

impl StyleChange {
    pub fn is_paragraph(&self) -> bool {
        matches!(self, Self::LineHeight(_) | Self::Align(_))
    }
    pub fn key(&self) -> usize {
        match self {
            Self::Family(_) => 0,
            Self::Color(_) => 5,
            Self::Opacity(_) => 6,
            Self::Size(_) => 7,
            Self::LineHeight(_) => 8,
            Self::Spacing(_) => 9,
            Self::Weight(_) => 10,
            Self::Align(_) => 11,
            Self::FillMode(_) => 12,
            Self::Gradient(_) => 13,
            Self::VerticalAlign(_) => 14,
        }
    }
    fn apply(&self, style: &mut TextStyle) {
        match self {
            Self::Family(value) => style.family = value.clone(),
            Self::Weight(value) => style.weight = *value,
            Self::Size(value) => style.size = *value,
            Self::Opacity(value) => style.color.a = *value,
            Self::Color(value) => {
                style.color.r = value.r;
                style.color.g = value.g;
                style.color.b = value.b;
            }
            Self::LineHeight(value) => style.line_height = *value,
            Self::Spacing(value) => style.spacing = *value,
            Self::Align(value) => style.align = *value,
            Self::VerticalAlign(value) => style.vertical_align = *value,
            Self::FillMode(value) => style.fill_mode = *value,
            Self::Gradient(value) => style.gradient = value.clone(),
        }
    }
}

impl TextStyle {
    pub fn same_property(&self, other: &Self, index: usize) -> bool {
        match index {
            0 => self.family == other.family,
            5 => {
                self.color.r == other.color.r
                    && self.color.g == other.color.g
                    && self.color.b == other.color.b
            }
            6 => self.color.a == other.color.a,
            7 => self.size == other.size,
            8 => self.line_height == other.line_height,
            9 => self.spacing == other.spacing,
            10 => self.weight == other.weight,
            11 => self.align == other.align,
            12 => self.fill_mode == other.fill_mode,
            13 => self.gradient == other.gradient,
            14 => self.vertical_align == other.vertical_align,
            _ => true,
        }
    }
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct StyleRun {
    pub range: Range<usize>,
    pub style: TextStyle,
}

/// Non-overlapping UTF-8 ranges covering every byte of the document.
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct StyledText {
    pub default: TextStyle,
    pub runs: Vec<StyleRun>,
}

impl StyledText {
    pub fn at(&self, offset: usize) -> &TextStyle {
        let index = self.runs.partition_point(|run| run.range.end <= offset);
        self.runs
            .get(index)
            .or_else(|| self.runs.last())
            .map_or(&self.default, |run| &run.style)
    }
    pub fn runs_in(&self, range: Range<usize>) -> &[StyleRun] {
        let start = self
            .runs
            .partition_point(|run| run.range.end <= range.start);
        let end = self.runs.partition_point(|run| run.range.start < range.end);
        &self.runs[start..end.max(start)]
    }
    pub fn in_range(&self, range: Range<usize>) -> impl Iterator<Item = &TextStyle> {
        let runs = self.runs_in(range);
        runs.iter()
            .map(|run| &run.style)
            .chain(runs.is_empty().then_some(&self.default))
    }
    pub fn apply(&mut self, range: Range<usize>, change: &StyleChange, whole: bool) {
        if whole {
            change.apply(&mut self.default);
        }
        let mut result = Vec::new();
        for run in self.runs.drain(..) {
            if run.range.start >= range.end || run.range.end <= range.start {
                result.push(run);
                continue;
            }
            if run.range.start < range.start {
                result.push(StyleRun {
                    range: run.range.start..range.start,
                    style: run.style.clone(),
                });
            }
            let mut changed = run.style.clone();
            change.apply(&mut changed);
            result.push(StyleRun {
                range: run.range.start.max(range.start)..run.range.end.min(range.end),
                style: changed,
            });
            if run.range.end > range.end {
                result.push(StyleRun {
                    range: range.end..run.range.end,
                    style: run.style,
                });
            }
        }
        self.runs = result;
        self.normalize();
    }
    pub fn replace(&mut self, range: Range<usize>, inserted: usize) {
        let inherit = if range.is_empty() {
            range.start.saturating_sub(1)
        } else {
            range.start
        };
        let style = self.at(inherit).clone();
        let mut result = Vec::new();
        for run in &self.runs {
            let end = run.range.end.min(range.start);
            if run.range.start < end {
                result.push(StyleRun {
                    range: run.range.start..end,
                    style: run.style.clone(),
                });
            }
        }
        if inserted > 0 {
            result.push(StyleRun {
                range: range.start..range.start + inserted,
                style: style.clone(),
            });
        }
        for run in &self.runs {
            let start = run.range.start.max(range.end);
            if start < run.range.end {
                result.push(StyleRun {
                    range: start - range.end + range.start + inserted
                        ..run.range.end - range.end + range.start + inserted,
                    style: run.style.clone(),
                });
            }
        }
        self.runs = result;
        if self.runs.is_empty() {
            self.default = style;
        }
        self.normalize();
    }
    fn normalize(&mut self) {
        let mut normalized: Vec<StyleRun> = Vec::new();
        for run in self.runs.drain(..) {
            if run.range.is_empty() {
                continue;
            }
            if let Some(last) = normalized.last_mut()
                && last.range.end == run.range.start
                && last.style == run.style
            {
                last.range.end = run.range.end;
            } else {
                normalized.push(run);
            }
        }
        self.runs = normalized;
    }
}
