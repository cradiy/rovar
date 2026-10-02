use super::*;
use std::{rc::Rc, sync::Arc};

#[derive(Clone)]
pub(crate) struct TextFragment {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub style: TextStyle,
}

struct Fragment {
    line: ShapedLine,
    style: TextStyle,
    origin: Point<Pixels>,
}
struct Row {
    range: Range<usize>,
    fragments: Vec<Fragment>,
    origin: Point<Pixels>,
    height: Pixels,
    carets: Vec<(usize, Pixels)>,
}
#[derive(Clone)]
pub(super) struct TextLayout {
    rows: Rc<Vec<Row>>,
    pub bounds: Bounds<Pixels>,
}

fn shape(
    text: &str,
    style: &TextStyle,
    zoom: f32,
    text_system: &gpui::WindowTextSystem,
) -> ShapedLine {
    let mut font = gpui::font(crate::platform::render_font_family(&style.family));
    font.weight = FontWeight(style.weight);
    if style.spacing != 0. {
        font.features = FontFeatures(Arc::new(vec![("liga".into(), 0), ("clig".into(), 0)]));
    }
    let run = TextRun {
        len: text.len(),
        font,
        color: if style.fill_mode == FillMode::Linear {
            gpui::white()
        } else {
            style.color.into()
        },
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut line =
        text_system.shape_line(text.to_owned().into(), px(style.size * zoom), &[run], None);
    if style.spacing != 0. && !text.is_empty() {
        let boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        let spacing = px(style.spacing * zoom);
        let mut runs = line.runs.clone();
        for run in &mut runs {
            for glyph in &mut run.glyphs {
                let index = boundaries
                    .partition_point(|i| *i <= glyph.index)
                    .saturating_sub(1);
                glyph.position.x += spacing * index as f32;
            }
        }
        *line = Arc::new(LineLayout {
            font_size: line.font_size,
            width: line.width() + spacing * boundaries.len().saturating_sub(1) as f32,
            ascent: line.ascent,
            descent: line.descent,
            runs,
            len: line.len(),
        });
    }
    line
}

// Match GPUI's first glyph whose byte index is >= the requested index, but
// advance a single cursor for monotonically increasing grapheme boundaries.
fn positions_for_indices<'a>(
    line: &'a ShapedLine,
    indices: impl Iterator<Item = usize> + 'a,
) -> impl Iterator<Item = (usize, Pixels)> + 'a {
    let mut glyphs = line.runs.iter().flat_map(|run| &run.glyphs).peekable();
    indices.map(move |index| {
        while glyphs.peek().is_some_and(|glyph| glyph.index < index) {
            glyphs.next();
        }
        (
            index,
            glyphs.peek().map_or(line.width(), |glyph| glyph.position.x),
        )
    })
}

impl TextLayout {
    #[cfg(test)]
    pub fn shares_rows(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.rows, &other.rows)
    }

    pub fn new(
        content: &str,
        styles: &StyledText,
        zoom: f32,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) -> Self {
        Self::with_text_system(content, styles, zoom, bounds, window.text_system())
    }

    fn with_text_system(
        content: &str,
        styles: &StyledText,
        zoom: f32,
        bounds: Bounds<Pixels>,
        text_system: &gpui::WindowTextSystem,
    ) -> Self {
        let viewport = bounds;
        let bounds = Bounds::new(Point::default(), bounds.size);
        let mut rows = Vec::new();
        let mut paragraph_start = 0;
        let mut top = bounds.top();
        for paragraph in content.split('\n') {
            let paragraph_end = paragraph_start + paragraph.len();
            let paragraph_style = styles.at(paragraph_start);
            let mut metrics = Vec::new();
            for run in styles.runs_in(paragraph_start..paragraph_end) {
                let start = run.range.start.max(paragraph_start);
                let end = run.range.end.min(paragraph_end);
                if start >= end {
                    continue;
                }
                let text = &content[start..end];
                let shaped = shape(text, &run.style, zoom, text_system);
                let boundaries = text
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .chain([text.len()]);
                let mut positions = positions_for_indices(&shaped, boundaries);
                let (mut offset, mut x) = positions.next().unwrap();
                for (next, next_x) in positions {
                    let spacing = px(run.style.spacing * zoom);
                    let advance = next_x - x + if next == text.len() { spacing } else { px(0.) };
                    metrics.push((start + offset, start + next, advance, spacing));
                    (offset, x) = (next, next_x);
                }
            }
            let breaks: Vec<_> = unicode_linebreak::linebreaks(paragraph)
                .map(|(i, _)| paragraph_start + i)
                .collect();
            let mut cursor = 0;
            loop {
                let start = metrics.get(cursor).map_or(paragraph_start, |m| m.0);
                let mut next = cursor;
                let mut width = px(0.);
                let mut preferred = None;
                while next < metrics.len() {
                    let (_, end, advance, spacing) = metrics[next];
                    if width + advance - spacing > bounds.size.width && next > cursor {
                        break;
                    }
                    width += advance;
                    next += 1;
                    if breaks.binary_search(&end).is_ok() {
                        preferred = Some(next);
                    }
                    if width - spacing > bounds.size.width {
                        break;
                    }
                }
                if next < metrics.len()
                    && let Some(preferred) = preferred
                {
                    next = preferred;
                }
                let end = if next > cursor {
                    metrics[next - 1].1
                } else {
                    start
                };
                let mut fragments = Vec::new();
                let mut x = px(0.);
                let mut height = px(0.);
                let mut ascent = px(0.);
                let mut descent = px(0.);
                let mut band_descent = px(0.);
                let mut carets = Vec::new();
                for run in styles.runs_in(start..end) {
                    let a = run.range.start.max(start);
                    let b = run.range.end.min(end);
                    if a >= b {
                        continue;
                    }
                    let line = shape(&content[a..b], &run.style, zoom, text_system);
                    let indices = content[a..b].grapheme_indices(true).map(|(i, _)| i);
                    carets.extend(
                        positions_for_indices(&line, indices)
                            .map(|(i, position)| (a + i, x + position)),
                    );
                    height = height.max(px(run.style.size * paragraph_style.line_height * zoom));
                    ascent = ascent.max(line.ascent);
                    descent = descent.max(line.descent);
                    band_descent = band_descent.max(line.descent.abs());
                    let advance = line.width()
                        + if b < end {
                            px(run.style.spacing * zoom)
                        } else {
                            px(0.)
                        };
                    fragments.push(Fragment {
                        line,
                        style: run.style.clone(),
                        origin: point(x, px(0.)),
                    });
                    x += advance;
                }
                if fragments.is_empty() {
                    height = px(paragraph_style.size * paragraph_style.line_height * zoom);
                }
                carets.push((end, x));
                let slack = (bounds.size.width - x).max(px(0.));
                let left = bounds.left()
                    + match paragraph_style.align {
                        TextAlign::Center => slack / 2.,
                        TextAlign::Right => slack,
                        _ => px(0.),
                    };
                // Align within this row's font metric band, excluding its leading.
                // A fragment on another row or outside the selection stays independent.
                // Some text backends report descent below the baseline as negative.
                // Preserve the existing baseline and use unsigned extents for alignment.
                let band_height = ascent + band_descent;
                let band_top = top + (height - ascent - descent) / 2.;
                for fragment in &mut fragments {
                    let fragment_height = fragment.line.ascent + fragment.line.descent.abs();
                    let offset = match fragment.style.vertical_align {
                        VerticalAlign::Baseline => ascent - fragment.line.ascent,
                        VerticalAlign::Top => px(0.),
                        VerticalAlign::Center => (band_height - fragment_height) / 2.,
                        VerticalAlign::Bottom => band_height - fragment_height,
                    };
                    fragment.origin += point(left, band_top + offset);
                }
                for (_, x) in &mut carets {
                    *x += left;
                }
                rows.push(Row {
                    range: start..end,
                    fragments,
                    origin: point(left, top),
                    height,
                    carets,
                });
                top += height;
                if next >= metrics.len() {
                    break;
                }
                cursor = next;
            }
            paragraph_start = paragraph_end + 1;
        }
        Self {
            rows: Rc::new(rows),
            bounds: viewport,
        }
    }
    fn row_index(&self, index: usize) -> usize {
        self.rows
            .partition_point(|r| r.range.start <= index)
            .saturating_sub(1)
    }
    fn x(row: &Row, index: usize) -> Pixels {
        row.carets[row
            .carets
            .partition_point(|(i, _)| *i < index)
            .min(row.carets.len() - 1)]
        .1
    }
    pub fn caret_bounds(&self, index: usize) -> Bounds<Pixels> {
        let row = &self.rows[self.row_index(index)];
        Bounds::new(
            self.bounds.origin + point(Self::x(row, index), row.origin.y),
            size(px(1.), row.height),
        )
    }
    fn painted_caret_bounds(&self, index: usize) -> Bounds<Pixels> {
        let mut caret = self.caret_bounds(index);
        // Keep the caret clear of the selection border without moving glyphs or
        // changing the logical positions used for navigation and IME placement.
        caret.size.width = px(2.).min(self.bounds.size.width);
        let inset = px(2.).min((self.bounds.size.width - caret.size.width) / 2.);
        caret.origin.x = caret.origin.x.clamp(
            self.bounds.left() + inset,
            self.bounds.right() - inset - caret.size.width,
        );
        caret
    }
    pub fn index(&self, position: Point<Pixels>) -> usize {
        let position = position - self.bounds.origin;
        let row = &self.rows[self
            .rows
            .partition_point(|r| r.origin.y <= position.y)
            .saturating_sub(1)];
        row.carets
            .iter()
            .min_by(|a, b| {
                f32::from(a.1 - position.x)
                    .abs()
                    .total_cmp(&f32::from(b.1 - position.x).abs())
            })
            .map_or(row.range.start, |c| c.0)
    }
    pub fn navigate(&self, index: usize, key: &str) -> usize {
        let row_index = self.row_index(index);
        let row = &self.rows[row_index];
        match key {
            "home" => row.range.start,
            "end" => row.range.end,
            _ => {
                let next = if key == "up" {
                    row_index.saturating_sub(1)
                } else {
                    (row_index + 1).min(self.rows.len() - 1)
                };
                self.index(
                    self.bounds.origin
                        + point(
                            Self::x(row, index),
                            self.rows[next].origin.y + self.rows[next].height / 2.,
                        ),
                )
            }
        }
    }
    pub fn paint(
        &self,
        selection: Range<usize>,
        cursor: usize,
        marked: Option<Range<usize>>,
        focused: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Rows are ordered vertically; clipped overflow needs no paint traversal.
        for row in self
            .rows
            .iter()
            .take_while(|r| r.origin.y < self.bounds.size.height)
        {
            if selection.start < row.range.end && selection.end > row.range.start {
                let left = Self::x(row, selection.start);
                let right = Self::x(row, selection.end);
                window.paint_quad(fill(
                    Bounds::new(
                        self.bounds.origin + point(left, row.origin.y),
                        size((right - left).max(px(2.)), row.height),
                    ),
                    rgba(0x8560d955),
                ));
            }
            for fragment in &row.fragments {
                let mut paint = |window: &mut Window| {
                    fragment.line.paint(
                        self.bounds.origin + fragment.origin,
                        fragment.line.ascent + fragment.line.descent,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                };
                let result = if fragment.style.fill_mode == FillMode::Linear {
                    window.with_masked_fill(
                        self.bounds,
                        fragment.style.gradient.background(),
                        paint,
                    )
                } else {
                    paint(window)
                };
                if let Err(error) = result {
                    eprintln!("Could not render text: {error}");
                }
            }
            if let Some(marked) = &marked
                && marked.start < row.range.end
                && marked.end > row.range.start
            {
                let left = Self::x(row, marked.start);
                let right = Self::x(row, marked.end);
                window.paint_quad(fill(
                    Bounds::new(
                        self.bounds.origin + point(left, row.origin.y + row.height - px(2.)),
                        size(right - left, px(1.)),
                    ),
                    rgb(0x8560d9),
                ));
            }
        }
        if focused && selection.is_empty() {
            window.paint_quad(fill(self.painted_caret_bounds(cursor), rgb(0x8560d9)));
        }
    }
}

pub(crate) fn export_fragments(
    content: &str,
    styles: &StyledText,
    rect: Rect,
    text_system: &gpui::WindowTextSystem,
) -> Vec<TextFragment> {
    let layout = TextLayout::with_text_system(
        content,
        styles,
        1.,
        Bounds::new(Point::default(), size(px(rect.width), px(rect.height))),
        text_system,
    );
    layout
        .rows
        .iter()
        .take_while(|row| row.origin.y < px(rect.height))
        .flat_map(|row| row.fragments.iter())
        .map(|fragment| TextFragment {
            text: fragment.line.text.to_string(),
            x: fragment.origin.x.into(),
            baseline: (fragment.origin.y + fragment.line.ascent).into(),
            style: fragment.style.clone(),
        })
        .collect()
}

pub(crate) fn measure_content(
    content: &str,
    styles: &StyledText,
    width: f32,
    system: &gpui::WindowTextSystem,
) -> [f32; 2] {
    let layout = TextLayout::with_text_system(
        content,
        styles,
        1.,
        Bounds::new(
            Point::default(),
            size(px(width.max(1.)), px(crate::scene::artboard::MAX_SIZE)),
        ),
        system,
    );
    let width = layout
        .rows
        .iter()
        .filter_map(|r| r.carets.last().map(|(_, x)| f32::from(*x - r.origin.x)))
        .fold(1., f32::max);
    let height = layout
        .rows
        .iter()
        .map(|r| f32::from(r.height))
        .sum::<f32>()
        .max(1.);
    [width, height]
}

#[cfg(test)]
mod tests {
    use super::TextLayout;
    use crate::scene::text::{StyleChange, StyledText, VerticalAlign};
    use gpui::{Bounds, Empty, TestAppContext, point, px, size};

    #[gpui::test]
    fn caret_stays_inside_selection_edges_without_changing_text_layout(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(500.), px(400.)), |_, _| Empty);
        window
            .update(cx, |_, window, _| {
                for zoom in [0.25, 1., 4.] {
                    for content in ["", "你好\n\na", "你好你好你好你好"] {
                        let mut styles = StyledText::default();
                        styles.replace(0..0, content.len());
                        for alignment in [gpui::TextAlign::Left, gpui::TextAlign::Right] {
                            styles.apply(0..content.len(), &StyleChange::Align(alignment), true);
                            let bounds = Bounds::new(
                                point(px(40.5), px(30.25)),
                                size(px(80. * zoom), px(400. * zoom)),
                            );
                            let layout = TextLayout::new(content, &styles, zoom, bounds, window);
                            for row in layout.rows.iter() {
                                for index in [row.range.start, row.range.end] {
                                    let logical = layout.caret_bounds(index);
                                    let painted = layout.painted_caret_bounds(index);
                                    assert!(painted.left() > bounds.left() + px(1.));
                                    assert!(painted.right() < bounds.right() - px(1.));
                                    assert_eq!(painted.size.width, px(2.));
                                    assert_eq!(painted.top(), logical.top());
                                    assert_eq!(painted.size.height, logical.size.height);
                                }
                            }
                            let first = layout.caret_bounds(0);
                            if alignment == gpui::TextAlign::Left {
                                assert_eq!(first.left(), bounds.left());
                            }
                            assert_eq!(layout.index(first.origin), 0);
                        }
                    }
                }
                // A very narrow box still clips the caret to its own width.
                let bounds = Bounds::new(point(px(40.), px(30.)), size(px(1.), px(100.)));
                let layout = TextLayout::new("", &StyledText::default(), 1., bounds, window);
                let caret = layout.painted_caret_bounds(0);
                assert_eq!(caret.left(), bounds.left());
                assert_eq!(caret.right(), bounds.right());
            })
            .unwrap();
    }

    #[gpui::test]
    fn mixed_size_vertical_alignment_preserves_wrapping_hit_testing_and_zoom(
        cx: &mut TestAppContext,
    ) {
        let window = cx.open_window(size(px(500.), px(400.)), |_, _| Empty);
        window
            .update(cx, |_, window, _| {
                let content = "AB\nCD";
                let mut styles = StyledText::default();
                styles.replace(0..0, content.len());
                styles.apply(0..1, &StyleChange::Size(72.), false);
                let bounds = Bounds::new(point(px(20.), px(30.)), size(px(400.), px(200.)));
                let baseline = TextLayout::new(content, &styles, 1., bounds, window);
                let first = &baseline.rows[0].fragments;
                assert_eq!(first.len(), 2);
                assert!(
                    (f32::from(
                        first[0].origin.y + first[0].line.ascent
                            - first[1].origin.y
                            - first[1].line.ascent
                    ))
                    .abs()
                        < 0.001
                );
                let narrow_bounds = Bounds::new(bounds.origin, size(px(1.), bounds.size.height));
                let baseline_wrapped = TextLayout::new(content, &styles, 1., narrow_bounds, window);
                for alignment in [
                    VerticalAlign::Top,
                    VerticalAlign::Center,
                    VerticalAlign::Bottom,
                ] {
                    styles.apply(1..2, &StyleChange::VerticalAlign(alignment), false);
                    let layout = TextLayout::new(content, &styles, 1., bounds, window);
                    let large = &layout.rows[0].fragments[0];
                    let small = &layout.rows[0].fragments[1];
                    let fraction = match alignment {
                        VerticalAlign::Top => 0.,
                        VerticalAlign::Center => 0.5,
                        _ => 1.,
                    };
                    let large_edge =
                        large.origin.y + (large.line.ascent + large.line.descent.abs()) * fraction;
                    let small_edge =
                        small.origin.y + (small.line.ascent + small.line.descent.abs()) * fraction;
                    assert!((f32::from(large_edge - small_edge)).abs() < 0.001);
                    assert_eq!(large.origin, first[0].origin);
                    assert_eq!(
                        layout.rows[1].fragments[0].origin,
                        baseline.rows[1].fragments[0].origin
                    );
                    for index in [0, 1, 2, 3, 4, 5] {
                        assert_eq!(layout.caret_bounds(index), baseline.caret_bounds(index));
                        assert_eq!(layout.index(layout.caret_bounds(index).origin), index);
                    }
                    let half = TextLayout::new(
                        content,
                        &styles,
                        0.5,
                        Bounds::new(
                            bounds.origin,
                            size(bounds.size.width * 0.5, bounds.size.height * 0.5),
                        ),
                        window,
                    );
                    assert!(
                        (f32::from(half.rows[0].fragments[1].origin.y * 2. - small.origin.y)).abs()
                            < 0.001
                    );
                    // Each soft-wrapped row computes its own alignment band.
                    let wrapped = TextLayout::new(
                        content,
                        &styles,
                        1.,
                        Bounds::new(bounds.origin, size(px(1.), bounds.size.height)),
                        window,
                    );
                    assert_eq!(wrapped.rows.len(), 4);
                    assert_eq!(wrapped.rows[1].range, 1..2);
                    assert_eq!(
                        wrapped.rows[1].fragments[0].origin,
                        baseline_wrapped.rows[1].fragments[0].origin
                    );
                }
            })
            .unwrap();
    }
}
