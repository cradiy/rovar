use super::*;
use std::collections::BTreeMap;
use web_time::Instant;

pub(in crate::app) const BAR_HEIGHT: f32 = 46.;
pub(super) const TAB_TOP: f32 = 7.;
pub(super) const TAB_HEIGHT: f32 = 39.;
pub(super) const TAB_LEFT: f32 = 92.;
pub(super) const RIGHT_RESERVED: f32 = 236.;
pub(super) const TAB_GAP: f32 = 1.;

// Drag geometry and the exact critically damped spring follow gpui-chrome-tab-demo.
// MIT, copyright (c) 2026 cradiy. See LICENSE.
pub(crate) fn tab_width(viewport: f32, count: usize) -> f32 {
    let available = (viewport - TAB_LEFT - RIGHT_RESERVED).max(118.);
    ((available - TAB_GAP * count.saturating_sub(1) as f32) / count.max(1) as f32).clamp(118., 224.)
}
pub(crate) fn slot_left(index: usize, width: f32) -> f32 {
    index as f32 * (width + TAB_GAP)
}
pub(crate) fn insertion_index(x: f32, width: f32, count: usize) -> usize {
    (((x + (width + TAB_GAP) * 0.5) / (width + TAB_GAP)).floor() as isize)
        .clamp(0, count.saturating_sub(1) as isize) as usize
}
pub(crate) fn over_strip(x: f32, y: f32, viewport: f32, native: bool, snapped: bool) -> bool {
    let threshold = BAR_HEIGHT
        + if native {
            if snapped { 3. } else { -3. }
        } else {
            0.
        };
    y >= 0. && y < threshold && x >= 0. && x <= viewport
}
pub(crate) fn locked_y(pointer: f32, offset: f32) -> f32 {
    TAB_TOP - (pointer - offset)
}

#[derive(Clone, Copy)]
pub(crate) struct SpringSlot {
    pub current: f32,
    velocity: f32,
    pub target: f32,
}
impl SpringSlot {
    pub fn settled(target: f32) -> Self {
        Self {
            current: target,
            velocity: 0.,
            target,
        }
    }
    pub fn step(&mut self, dt: f32) -> bool {
        let displacement = self.current - self.target;
        let spring = self.velocity + 64. * displacement;
        let decay = (-64. * dt).exp();
        self.current = self.target + (displacement + spring * dt) * decay;
        self.velocity = (self.velocity - 64. * spring * dt) * decay;
        if (self.target - self.current).abs() < 0.18 && self.velocity.abs() < 20. {
            self.current = self.target;
            self.velocity = 0.;
            false
        } else {
            true
        }
    }
}

#[derive(Clone)]
pub(crate) struct DragTab {
    pub token: usize,
    pub title: String,
    pub source: gpui::WeakEntity<Studio>,
    pub window: gpui::WindowHandle<Studio>,
    pub width: f32,
    pub thumbnail: Option<PathBuf>,
    pub preview: Rc<RefCell<Option<Entity<DragPreview>>>>,
    pub transaction: Rc<RefCell<DragTransaction>>,
}
pub(crate) struct DragTransaction {
    pub origin_index: usize,
    pub origin_selected: Option<usize>,
    pub cursor_offset_x: f32,
    pub native: bool,
    pub owner: Option<(u64, usize)>,
    pub detached: Option<Tab>,
}
impl DragTab {
    pub(in crate::app) fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.transaction, &other.transaction)
    }
}

pub(crate) struct TabStrip {
    pub hovered: Option<usize>,
    pub hover_card: Option<usize>,
    pub hover_task: Task<()>,
    pub slots: BTreeMap<usize, SpringSlot>,
    pub drag: Option<DragTab>,
    pub snap_index: Option<usize>,
    pub snap_left: Option<f32>,
    last_frame: Instant,
}
impl Default for TabStrip {
    fn default() -> Self {
        Self {
            hovered: None,
            hover_card: None,
            hover_task: Task::ready(()),
            slots: Default::default(),
            drag: None,
            snap_index: None,
            snap_left: None,
            last_frame: Instant::now(),
        }
    }
}
impl Studio {
    pub(super) fn tab_left(&self) -> f32 {
        TAB_LEFT + self.chrome.left_inset()
    }

    pub(super) fn tab_right_reserved(&self) -> f32 {
        RIGHT_RESERVED + self.chrome.controls_width()
    }

    pub(in crate::app) fn tab_width(&self, viewport: f32, count: usize) -> f32 {
        tab_width(
            viewport - self.chrome.left_inset() - self.chrome.controls_width(),
            count,
        )
    }

    pub(in crate::app) fn animate_tabs(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.strip.last_frame)
            .as_secs_f32()
            .min(1. / 20.);
        self.strip.last_frame = now;
        let width = self.tab_width(
            f32::from(window.viewport_size().width),
            self.tabs.len() + usize::from(self.strip.snap_index.is_some()),
        );
        let mut moving = false;
        for (index, tab) in self.tabs.iter().enumerate() {
            let visual =
                index + usize::from(self.strip.snap_index.is_some_and(|snap| index >= snap));
            let target = slot_left(visual, width);
            let slot = self
                .strip
                .slots
                .entry(tab.token)
                .or_insert_with(|| SpringSlot::settled(target));
            slot.target = target;
            moving |= slot.step(dt);
        }
        self.strip
            .slots
            .retain(|token, _| self.tabs.iter().any(|tab| tab.token == *token));
        if moving {
            window.request_animation_frame();
        }
    }
}

pub(crate) struct DragPreview {
    pub title: String,
    pub thumbnail: Option<PathBuf>,
    pub width: f32,
    pub detached: bool,
    pub hidden: bool,
    pub cursor_offset_y: f32,
    pub y_offset: f32,
}
impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.hidden {
            return div().size(px(1.)).opacity(0.);
        }
        if !self.detached {
            return div().w(px(self.width)).h(px(TAB_HEIGHT)).relative().child(
                tab_face(&self.title, cx)
                    .absolute()
                    .top(px(self.y_offset))
                    .w_full()
                    .h(px(TAB_HEIGHT)),
            );
        }
        div()
            .w(px(336.))
            .h(px(196.))
            .overflow_hidden()
            .rounded(px(12.))
            .border_1()
            .border_color(gpui::rgba(0xffffff2b))
            .bg(rgb(0x17171f))
            .shadow_xl()
            .flex()
            .flex_col()
            .text_color(rgb(TEXT))
            .font_family(crate::ui::font::family(cx))
            .child(
                div()
                    .h(px(34.))
                    .px(px(12.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(icon(LucideIcons::PenTool, 14.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(self.title.clone()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x121419))
                    .when_some(self.thumbnail.clone(), |el, path| {
                        el.child(
                            gpui::img(crate::platform::preview_image(path))
                                .w(px(266.))
                                .h(px(160.))
                                .object_fit(gpui::ObjectFit::Contain),
                        )
                    })
                    .when(self.thumbnail.is_none(), |el| {
                        el.child(
                            div()
                                .text_color(rgb(ACCENT))
                                .child(icon(LucideIcons::Frame, 36.)),
                        )
                    }),
            )
    }
}
pub(in crate::app) fn tab_face(title: &str, cx: &gpui::App) -> gpui::Div {
    div()
        .rounded_tl(px(8.))
        .rounded_tr(px(8.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .bg(rgb(0x292531))
        .text_color(rgb(TEXT))
        .text_size(px(12.))
        .font_family(crate::ui::font::family(cx))
        .child(icon(LucideIcons::PenTool, 13.))
        .child(div().flex_1().min_w_0().truncate().child(title.to_owned()))
        .child(icon(LucideIcons::X, 12.))
}
