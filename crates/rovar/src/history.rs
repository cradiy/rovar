use crate::{
    artboard::{Artboard, Rect},
    property::{Property, TextProperty},
    shape::Shape,
    text::Snapshot,
};
use std::{
    cell::RefCell,
    ops::Range,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) type SharedHistory = Rc<RefCell<History>>;

#[derive(Clone)]
pub(crate) struct PageEdit {
    pub components: crate::components::Definitions,
    pub pages: std::collections::BTreeMap<String, Option<SavedPage>>,
    pub order: Vec<String>,
    pub active: String,
}

#[derive(Clone)]
pub(crate) struct SavedPage {
    pub page: crate::document::Page,
    pub view: [f32; 3],
    pub selection: std::collections::BTreeSet<usize>,
    pub folded: std::collections::HashSet<usize>,
}
impl SavedPage {
    pub fn new(page: crate::document::Page) -> Self {
        Self {
            page,
            view: [0., 0., 1.],
            selection: Default::default(),
            folded: Default::default(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SavedText {
    pub id: usize,
    pub layer: crate::layer::LayerState,
    pub board: Option<usize>,
    pub rect: Rect,
    pub text: Snapshot,
}

/// Each change stores the state to restore. Replaying captures its inverse.
/// No entities are retained, so deleted objects do not retain UI subscriptions.
#[derive(Clone)]
pub(crate) enum Change {
    Pages {
        value: PageEdit,
    },
    NodeSelection {
        value: Option<(usize, usize)>,
    },
    Hierarchy {
        value: crate::layer::Hierarchy,
        selection: std::collections::BTreeSet<usize>,
    },
    Layer {
        id: usize,
        value: crate::layer::LayerState,
    },
    Board {
        id: usize,
        index: usize,
        value: Option<Artboard>,
    },
    TextBox {
        id: usize,
        index: usize,
        value: Option<SavedText>,
    },
    Text {
        id: usize,
        value: Snapshot,
    },
    Shape {
        id: usize,
        index: usize,
        value: Option<Shape>,
    },
    TextRect {
        id: usize,
        board: Option<usize>,
        value: Rect,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Group {
    SelectionProperty(Vec<usize>, Property),
    Typing(usize),
    Style(usize, TextProperty, Range<usize>),
    Property {
        id: usize,
        property: Property,
        stop: usize,
        stroke: bool,
    },
    Color(usize, usize),
}

#[derive(Default)]
pub(crate) struct History {
    pub suppressed: bool,
    revision: u64,
    page: String,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    group: Option<(Group, Instant)>,
    scope: Option<(Group, bool)>,
    preview: Option<Vec<Change>>,
}

struct Entry {
    page: String,
    changes: Vec<Change>,
}

impl History {
    pub fn record_for(&mut self, page: String, changes: Vec<Change>) {
        self.break_group();
        let preview = self.preview.take();
        let scope = self.scope.take();
        let current = std::mem::replace(&mut self.page, page);
        self.record(changes, None);
        self.page = current;
        self.preview = preview;
        self.scope = scope;
    }
    pub fn set_page(&mut self, page: String) {
        self.break_group();
        self.page = page;
    }
    pub fn replay_page(&self, redo: bool) -> Option<&str> {
        (if redo { &self.redo } else { &self.undo })
            .last()
            .map(|entry| entry.page.as_str())
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn mark_changed(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("document revision exhausted");
    }
    pub fn break_group(&mut self) {
        self.group = None;
        self.scope = None;
    }
    // A picker drag has an explicit Commit boundary; numeric typing uses a timeout.
    pub fn set_scope(&mut self, group: Group, continuous: bool) {
        self.scope = Some((group, continuous));
    }
    pub fn clear_scope(&mut self) {
        self.scope = None;
    }
    // A numeric drag previews one property on one object. Keep its first
    // inverse until commit, leaving undo/redo untouched during the drag.
    pub fn begin_preview(&mut self) {
        self.break_group();
        self.preview = Some(Vec::new());
    }
    pub fn end_preview(&mut self, commit: bool) -> Vec<Change> {
        let changes = self.preview.take().unwrap_or_default();
        self.break_group();
        if commit && !changes.is_empty() {
            self.record(changes, None);
            Vec::new()
        } else {
            changes
        }
    }
    pub fn can_merge(&self, group: Option<&Group>) -> bool {
        if let Some(changes) = &self.preview {
            return !changes.is_empty();
        }
        let key = self.scope.as_ref().map(|s| &s.0).or(group);
        self.group.as_ref().is_some_and(|(last, time)| {
            Some(last) == key
                && (self.scope.as_ref().is_some_and(|s| s.1)
                    || time.elapsed() < Duration::from_secs(1))
        })
    }
    pub fn record(&mut self, changes: Vec<Change>, group: Option<Group>) {
        if self.suppressed {
            return;
        }
        // Merged typing/style edits may intentionally omit the inverse snapshot.
        self.mark_changed();
        if let Some(before) = &mut self.preview {
            if before.is_empty() {
                *before = changes;
            }
            return;
        }
        if !changes.is_empty() && !self.can_merge(group.as_ref()) {
            self.undo.push(Entry {
                page: self.page.clone(),
                changes,
            });
            // Keep history bounded without retaining every document forever.
            if self.undo.len() > 256 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.group = self
            .scope
            .as_ref()
            .map(|s| s.0.clone())
            .or(group)
            .map(|g| (g, Instant::now()));
    }
    pub fn take(&mut self, redo: bool) -> Option<Vec<Change>> {
        self.break_group();
        if redo {
            self.redo.pop().map(|entry| entry.changes)
        } else {
            self.undo.pop().map(|entry| entry.changes)
        }
    }
    pub fn finish_replay(&mut self, inverse: Vec<Change>, redo: bool) {
        self.mark_changed();
        if redo {
            self.undo.push(Entry {
                page: self.page.clone(),
                changes: inverse,
            });
        } else {
            self.redo.push(Entry {
                page: self.page.clone(),
                changes: inverse,
            });
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
