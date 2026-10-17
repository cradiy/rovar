use crate::scene::artboard::Rect;
use std::ops::Range;

struct Branch {
    bounds: Rect,
    entries: Range<usize>,
    children: Option<(usize, usize)>,
}

#[derive(Default)]
pub(super) struct SpatialIndex {
    entries: Vec<(usize, Rect)>,
    branches: Vec<Branch>,
}

impl SpatialIndex {
    pub fn new(entries: Vec<(usize, Rect)>) -> Self {
        let mut tree = Self {
            entries,
            branches: Vec::new(),
        };
        if !tree.entries.is_empty() {
            tree.build(0..tree.entries.len());
        }
        tree
    }

    fn build(&mut self, range: Range<usize>) -> usize {
        let mut bounds = self.entries[range.start].1;
        for (_, rect) in &self.entries[range.clone()] {
            let right = (bounds.x + bounds.width).max(rect.x + rect.width);
            let bottom = (bounds.y + bounds.height).max(rect.y + rect.height);
            bounds.x = bounds.x.min(rect.x);
            bounds.y = bounds.y.min(rect.y);
            bounds.width = right - bounds.x;
            bounds.height = bottom - bounds.y;
        }
        let branch = self.branches.len();
        self.branches.push(Branch {
            bounds,
            entries: range.clone(),
            children: None,
        });
        if range.len() > 16 {
            let middle = range.start + range.len() / 2;
            let center = |r: Rect| {
                if bounds.width >= bounds.height {
                    r.x + r.width / 2.
                } else {
                    r.y + r.height / 2.
                }
            };
            self.entries[range.clone()].select_nth_unstable_by(middle - range.start, |a, b| {
                center(a.1).total_cmp(&center(b.1))
            });
            let left = self.build(range.start..middle);
            let right = self.build(middle..range.end);
            self.branches[branch].children = Some((left, right));
        }
        branch
    }

    pub fn query(&self, viewport: Rect, out: &mut Vec<usize>) {
        if !self.branches.is_empty() {
            self.visit(0, viewport, out);
        }
    }

    fn visit(&self, id: usize, viewport: Rect, out: &mut Vec<usize>) {
        let branch = &self.branches[id];
        let intersects = |r: Rect| {
            r.x <= viewport.x + viewport.width
                && r.y <= viewport.y + viewport.height
                && r.x + r.width >= viewport.x
                && r.y + r.height >= viewport.y
        };
        if !intersects(branch.bounds) {
            return;
        }
        if let Some((left, right)) = branch.children {
            self.visit(left, viewport, out);
            self.visit(right, viewport, out);
        } else {
            out.extend(
                self.entries[branch.entries.clone()]
                    .iter()
                    .filter(|(_, rect)| intersects(*rect))
                    .map(|(position, _)| *position),
            );
        }
    }
}
