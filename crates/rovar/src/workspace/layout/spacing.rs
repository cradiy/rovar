use super::measurement::interval;
use super::*;
#[cfg(test)]
mod tests;
mod view;

#[derive(Clone)]
struct Plan {
    axis: usize,
    items: Vec<(usize, Rect)>,
    cross: f32,
}
impl Plan {
    fn gap(&self, index: usize) -> f32 {
        interval(self.items[index + 1].1, self.axis).0 - interval(self.items[index].1, self.axis).1
    }
    fn ids(&self) -> BTreeSet<usize> {
        self.items.iter().map(|(id, _)| *id).collect()
    }
}

pub(in crate::workspace) struct State {
    input: Entity<TextInput>,
    edit: Option<(Plan, usize)>,
    drag: Option<(Plan, Vec<Change>)>,
    invalid: bool,
    _subscriptions: Vec<Subscription>,
}
impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let input = cx.new(TextInput::new);
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit(_)) && this.finish_spacing_input(true, cx) {
                this.focus.focus(window, cx);
            }
        });
        let blur = cx.on_blur(&input.focus_handle(cx), window, |this, _, cx| {
            if !this.finish_spacing_input(true, cx) {
                this.finish_spacing_input(false, cx);
            }
        });
        Self {
            input,
            edit: None,
            drag: None,
            invalid: false,
            _subscriptions: vec![submit, blur],
        }
    }
    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }
}

fn label(value: f32) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}").trim_end_matches('0').to_owned()
    }
}

impl Workspace {
    fn spacing_plan(&self) -> Option<Plan> {
        let ids = self.selection_ids();
        let parent = self.layer_parent(*ids.first()?);
        if ids.len() < 2
            || ids.iter().any(|id| {
                !self.layer_editable(*id)
                    || self.layer_parent(*id) != parent
                    || self.is_layout_flow_item(*id)
            })
        {
            return None;
        }
        let rects = ids
            .into_iter()
            .map(|id| self.world_bounds(id).map(|r| (id, r)))
            .collect::<Option<Vec<_>>>()?;
        for axis in 0..2 {
            let low = rects
                .iter()
                .map(|(_, r)| interval(*r, 1 - axis).0)
                .fold(f32::NEG_INFINITY, f32::max);
            let high = rects
                .iter()
                .map(|(_, r)| interval(*r, 1 - axis).1)
                .fold(f32::INFINITY, f32::min);
            if low >= high {
                continue;
            }
            let mut items = rects.clone();
            items.sort_by(|a, b| {
                interval(a.1, axis)
                    .0
                    .total_cmp(&interval(b.1, axis).0)
                    .then(a.0.cmp(&b.0))
            });
            if items
                .windows(2)
                .any(|pair| interval(pair[0].1, axis).1 > interval(pair[1].1, axis).0 + 0.001)
            {
                continue;
            }
            return Some(Plan {
                axis,
                items,
                cross: (low + high) / 2.,
            });
        }
        None
    }

    fn apply_spacing(&mut self, plan: &Plan, gap: f32) {
        let mut next = interval(plan.items[0].1, plan.axis).0;
        for &(id, original) in &plan.items {
            if let Some(current) = self.world_bounds(id) {
                let delta = next - interval(current, plan.axis).0;
                self.translate_root(
                    id,
                    if plan.axis == 0 {
                        point(delta, 0.)
                    } else {
                        point(0., delta)
                    },
                );
            }
            let (start, end) = interval(original, plan.axis);
            next += end - start + gap;
        }
    }

    fn begin_spacing(
        &mut self,
        index: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let Some(plan) = self.spacing_plan() else {
            return;
        };
        if index + 1 >= plan.items.len() {
            return;
        }
        self.finish_spacing_input(false, cx);
        self.seal_text_edits(cx);
        let kind = GestureKind::Spacing {
            axis: plan.axis,
            original: plan.gap(index),
        };
        let before = self.before_geometry();
        self.spacing.drag = Some((plan, before));
        self.begin(kind, event.position, event.button, window, cx);
    }

    pub(in crate::workspace) fn move_spacing(
        &mut self,
        axis: usize,
        original: f32,
        delta: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some((plan, _)) = &self.spacing.drag else {
            return;
        };
        let plan = plan.clone();
        let distance = if axis == 0 { delta.x } else { delta.y };
        if distance.abs() < 3. {
            // Returning to the starting point restores unequal gaps too.
            let before = self.spacing.drag.take().unwrap();
            self.restore_batch(&before.1, cx);
            self.spacing.drag = Some(before);
        } else {
            self.apply_spacing(
                &plan,
                (original + distance / self.view.zoom)
                    .round()
                    .clamp(0., 100_000.),
            );
        }
        self.sync_fields(cx);
    }

    pub(in crate::workspace) fn finish_spacing(&mut self, commit: bool, cx: &mut Context<Self>) {
        if let Some((_, before)) = self.spacing.drag.take() {
            if !commit {
                self.restore_batch(&before, cx);
            } else if self.batch_changed(&before, cx) {
                self.history.borrow_mut().record(before, None);
            }
            self.sync_fields(cx);
        }
    }

    fn edit_spacing(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.gesture.is_some() {
            return;
        }
        let Some(plan) = self.spacing_plan() else {
            return;
        };
        if index + 1 >= plan.items.len() {
            return;
        }
        let value = plan.gap(index).to_string();
        self.spacing
            .input
            .update(cx, |input, cx| input.set_value(value, cx));
        self.spacing.edit = Some((plan, index));
        self.spacing.invalid = false;
        self.spacing.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn finish_spacing_input(
        &mut self,
        commit: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((plan, index)) = self.spacing.edit.clone() else {
            return true;
        };
        if !commit
            || self.spacing.input.read(cx).value().as_ref() == plan.gap(index).to_string()
            || self
                .spacing_plan()
                .is_none_or(|current| current.ids() != plan.ids() || current.axis != plan.axis)
        {
            self.spacing.edit = None;
            self.spacing.invalid = false;
            cx.notify();
            return true;
        }
        let value = self
            .spacing
            .input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0. ..=100_000.).contains(v));
        let Some(value) = value else {
            self.spacing.invalid = true;
            cx.notify();
            return false;
        };
        self.spacing.edit = None;
        self.spacing.invalid = false;
        self.seal_text_edits(cx);
        let before = self.before_geometry();
        self.apply_spacing(&plan, value);
        if self.batch_changed(&before, cx) {
            self.history.borrow_mut().record(before, None);
        }
        self.sync_fields(cx);
        cx.notify();
        true
    }
}
