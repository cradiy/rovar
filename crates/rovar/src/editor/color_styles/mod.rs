use super::*;
use crate::i18n::t;
use crate::scene::color_styles::{ColorStyle, Palette};
use assets::Scope;
use gpui::Focusable;
use uic::components::dropdown::DropdownState;

mod gradient;
mod input;
#[cfg(test)]
mod tests;
mod view;

pub(super) struct Dialog {
    scope: Scope,
    id: String,
    existing: bool,
    pub(in crate::editor) gradient: Option<crate::scene::artboard::LinearGradient>,
    saved_gradient: Option<crate::scene::artboard::LinearGradient>,
    solid: gpui::Rgba,
    scrub: Option<gradient::Scrub>,
    stop_before: Option<(crate::scene::artboard::LinearGradient, usize)>,
    pub(super) active_stop: usize,
}

pub(super) struct State {
    pub palette: Palette,
    pub dialog: Option<Dialog>,
    scope: Scope,
    pub(super) menu: [Entity<DropdownState>; 2],
    name: Entity<TextInput>,
    value: Entity<TextInput>,
    angle: Entity<TextInput>,
    position: Entity<TextInput>,
    picker: Entity<ColorPickerState>,
    error: Option<String>,
    selected: Option<(Scope, String)>,
    pub(super) ramp_bounds: std::rc::Rc<std::cell::Cell<gpui::Bounds<Pixels>>>,
    scroll: gpui::ScrollHandle,
    baselines: [gpui::SharedString; 4],
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let name = cx.new(TextInput::new);
        let value = cx.new(TextInput::new);
        let angle = cx.new(TextInput::new);
        let position = cx.new(TextInput::new);
        let picker = cx.new(|cx| ColorPickerState::new(rgb(ACCENT), cx));
        let mut subscriptions: Vec<_> = [&name, &value, &angle, &position]
            .into_iter()
            .enumerate()
            .flat_map(|(index, input)| {
                let focus = input.focus_handle(cx);
                [
                    cx.observe(input, |_, _, cx| cx.notify()),
                    cx.subscribe_in(input, window, move |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Submit(_)) {
                            this.finish_style_input(index, cx);
                        }
                    }),
                    cx.on_focus(&focus, window, move |this, _, cx| {
                        this.colors.baselines[index] = this.style_inputs()[index].read(cx).value();
                    }),
                    cx.on_blur(&focus, window, move |this, _, cx| {
                        if this.colors.dialog.is_some() {
                            this.finish_style_input(index, cx);
                        }
                    }),
                ]
            })
            .collect();
        subscriptions.push(
            cx.subscribe(&picker, |this, _, event: &ColorPickerEvent, cx| {
                let (ColorPickerEvent::Preview(color) | ColorPickerEvent::Commit(color)) = *event;
                this.colors
                    .value
                    .update(cx, |input, cx| input.set_value(color_hex(color), cx));
                this.colors.error = None;
                this.update_gradient_color(color);
                cx.notify();
            }),
        );
        subscriptions.push(cx.subscribe(&value, |this, _, event: &InputEvent, cx| {
            if let InputEvent::Change(value) = event
                && value == &this.colors.value.read(cx).value()
                && let Some(color) = parse_color(value)
            {
                this.colors
                    .picker
                    .update(cx, |picker, cx| picker.set_value(color, cx));
                this.colors.error = None;
                this.update_gradient_color(color);
            }
        }));
        for (input, is_angle) in [(&angle, true), (&position, false)] {
            subscriptions.push(cx.subscribe(input, move |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change(value) = event
                    && value
                        == &this.style_inputs()[if is_angle { 2 } else { 3 }]
                            .read(cx)
                            .value()
                    && let Ok(value) = value.parse::<f32>()
                    && value.is_finite()
                    && let Some(dialog) = &mut this.colors.dialog
                    && let Some(gradient) = &mut dialog.gradient
                {
                    if is_angle && (0. ..=360.).contains(&value) {
                        gradient.angle = value;
                    } else if !is_angle {
                        gradient.set_position(dialog.active_stop, value / 100.);
                    }
                    cx.notify();
                }
            }));
        }
        Self {
            palette: Palette::new(),
            dialog: None,
            scope: Scope::Document,
            menu: std::array::from_fn(|_| cx.new(|cx| DropdownState::new(window, cx))),
            name,
            value,
            angle,
            position,
            picker,
            error: None,
            selected: None,
            ramp_bounds: Default::default(),
            scroll: gpui::ScrollHandle::new(),
            baselines: Default::default(),
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    fn color_palette(&self, scope: Scope, cx: &gpui::App) -> Palette {
        match scope {
            Scope::Document => self.colors.palette.clone(),
            Scope::Local => self
                .assets
                .library
                .as_ref()
                .map(|l| l.read(cx).colors.clone())
                .unwrap_or_default(),
        }
    }

    fn color_source(&self, stroke: bool, cx: &gpui::App) -> Option<(gpui::Rgba, Option<String>)> {
        if let Some(shape) = self.selected_shape() {
            if !self.layer_editable(shape.id) {
                return None;
            }
            return Some(if stroke {
                (shape.stroke.color, shape.stroke.color_style.clone())
            } else {
                (shape.color, shape.color_style.clone())
            });
        }
        if !stroke {
            if let Some(text) = self.selected_text() {
                if !self.layer_editable(text.id) {
                    return None;
                }
                let style = text.editor.read(cx).effective_style();
                return Some((style.color, style.color_style.clone()));
            }
            if let Some(board) = self.selected_board().filter(|b| self.layer_editable(b.id)) {
                return Some((board.color, board.color_style.clone()));
            }
        }
        None
    }

    fn apply_color_style(
        &mut self,
        scope: Scope,
        id: &str,
        stroke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(style) = self.color_palette(scope, cx).get(id).cloned() else {
            return;
        };
        if self.color_source(stroke, cx).is_none() {
            return;
        }
        let reference = (scope == Scope::Document).then(|| id.to_owned());
        if self.selected_text.is_some() {
            self.history.borrow_mut().break_group();
            self.change_text_style(StyleChange::ColorStyle(reference, style), cx);
        } else {
            self.seal_text_edits(cx);
            let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
            let mut page = self.snapshot_page(cx).0;
            if let Some(shape) = page
                .shapes
                .iter_mut()
                .find(|s| Some(s.id) == self.selected_shape)
            {
                if stroke {
                    style.apply(
                        &mut shape.stroke.color,
                        &mut shape.stroke.fill_mode,
                        &mut shape.stroke.gradient,
                    );
                    shape.stroke.enabled = true;
                    shape.stroke.color_style = reference;
                } else {
                    style.apply(&mut shape.color, &mut shape.fill_mode, &mut shape.gradient);
                    shape.fill_enabled = true;
                    shape.color_style = reference;
                }
            } else if let Some(board) = page.boards.iter_mut().find(|b| Some(b.id) == self.selected)
            {
                style.apply(&mut board.color, &mut board.fill_mode, &mut board.gradient);
                board.color_style = reference;
            }
            self.apply_component_page(page, window, cx);
            self.record_page_edit(before);
        }
        self.colors.menu[usize::from(stroke)].update(cx, |m, cx| m.close(window, cx));
        self.colors.selected = Some((scope, id.to_owned()));
        cx.notify();
    }

    fn detach_color_style(&mut self, stroke: bool, cx: &mut Context<Self>) {
        if self.color_source(stroke, cx).is_none() {
            return;
        }
        if self.selected_text.is_some() {
            self.history.borrow_mut().break_group();
            self.change_text_style(StyleChange::DetachColorStyle, cx);
        } else if self.selected_shape.is_some() {
            self.edit_shape(|s| {
                if stroke {
                    s.stroke.color_style = None
                } else {
                    s.color_style = None
                }
            });
        } else {
            self.edit_board(None, |b| b.color_style = None);
        }
        cx.notify();
    }

    fn set_document_color(
        &mut self,
        id: String,
        value: Option<ColorStyle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seal_text_edits(cx);
        self.sync_components(window, cx);
        let ids: Vec<_> = self
            .pages
            .entries
            .iter()
            .map(|p| p.page.id.clone())
            .collect();
        let before = self.page_edit(&ids, cx);
        self.park_page(cx);
        if let Some(value) = value {
            self.colors.palette.insert(id, value);
        } else {
            self.colors.palette.remove(&id);
        }
        for page in self.pages.entries.iter_mut().map(|p| &mut p.page).chain(
            self.components
                .definitions
                .values_mut()
                .map(|d| &mut d.page),
        ) {
            crate::scene::color_styles::resolve(page, &self.colors.palette);
            for binding in page.hierarchy.components.values_mut() {
                if let Ok(mut baseline) =
                    serde_json::from_value::<crate::document::Page>(binding.baseline.clone())
                {
                    crate::scene::color_styles::resolve(&mut baseline, &self.colors.palette);
                    binding.baseline = serde_json::to_value(baseline).unwrap();
                }
            }
        }
        self.apply_component_page(self.pages.current().page.clone(), window, cx);
        self.record_page_edit(before);
        self.components.revision = None;
        cx.notify();
    }

    fn open_color_dialog(
        &mut self,
        scope: Scope,
        id: Option<String>,
        stroke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = id
            .as_ref()
            .and_then(|id| self.color_palette(scope, cx).get(id).cloned());
        let style = existing.clone().unwrap_or_else(|| ColorStyle {
            name: t("color-style-default").into(),
            color: self.color_source(stroke, cx).map_or(rgb(ACCENT), |s| s.0),
            gradient: self.selected_gradient(stroke, cx),
        });
        for menu in &self.colors.menu {
            menu.update(cx, |m, cx| m.close(window, cx));
        }
        self.colors.dialog = Some(Dialog {
            scope,
            id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            existing: existing.is_some(),
            active_stop: style.gradient.as_ref().map_or(0, |g| g.stops()[0].id),
            gradient: style.gradient.clone(),
            saved_gradient: None,
            solid: style.color,
            scrub: None,
            stop_before: None,
        });
        self.colors.error = None;
        self.colors
            .picker
            .update(cx, |picker, cx| picker.set_value(style.color, cx));
        self.colors
            .name
            .update(cx, |i, cx| i.set_value(style.name, cx));
        self.colors
            .value
            .update(cx, |i, cx| i.set_value(color_hex(style.color), cx));
        self.sync_gradient_inputs(cx);
        self.colors.name.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_color_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.colors.dialog = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn save_color_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.colors.dialog.as_ref() else {
            return;
        };
        let (scope, id) = (dialog.scope, dialog.id.clone());
        let active_stop = dialog.active_stop;
        let mut gradient = dialog.gradient.clone();
        if gradient.is_some() && !self.valid_gradient_inputs(cx) {
            self.colors.error = Some(t("color-style-invalid-gradient").into());
            cx.notify();
            return;
        }
        let Some(color) = parse_color(&self.colors.value.read(cx).value()) else {
            self.colors.error = Some(t("color-style-invalid").into());
            cx.notify();
            return;
        };
        if let Some(gradient) = &mut gradient {
            gradient.angle = self.colors.angle.read(cx).value().parse::<f32>().unwrap();
            gradient.set_position(
                active_stop,
                self.colors
                    .position
                    .read(cx)
                    .value()
                    .parse::<f32>()
                    .unwrap()
                    / 100.,
            );
            if let Some(stop) = gradient.stop_mut(active_stop) {
                stop.color = color;
            }
        }
        let style = ColorStyle {
            name: self.colors.name.read(cx).value().trim().into(),
            color,
            gradient,
        };
        if style.validate().is_err() {
            self.colors.error = Some(t("color-style-invalid-name").into());
            cx.notify();
            return;
        }
        if scope == Scope::Document {
            self.set_document_color(id.clone(), Some(style), window, cx);
        } else if let Some(library) = self.assets.library.clone() {
            if let Err(error) = library.update(cx, |l, cx| l.set_color(id.clone(), Some(style), cx))
            {
                self.colors.error = Some(error.to_string());
                cx.notify();
                return;
            }
        } else {
            return;
        }
        let position = self
            .color_palette(scope, cx)
            .keys()
            .position(|key| key == &id)
            .unwrap_or(0);
        self.colors.scroll.scroll_to_item(position);
        self.colors.selected = Some((scope, id));
        self.assets.scope = scope;
        self.assets
            .search
            .update(cx, |input, cx| input.set_value("", cx));
        self.close_color_dialog(window, cx);
    }

    fn delete_color_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.colors.dialog.as_ref() else {
            return;
        };
        let (scope, id) = (dialog.scope, dialog.id.clone());
        if scope == Scope::Document {
            self.set_document_color(id, None, window, cx);
        } else if let Some(library) = self.assets.library.clone()
            && let Err(error) = library.update(cx, |l, cx| l.set_color(id, None, cx))
        {
            self.colors.error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.close_color_dialog(window, cx);
    }
}

fn color_hex(color: gpui::Rgba) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8,
        (color.a * 255.).round() as u8
    )
}

fn parse_color(value: &str) -> Option<gpui::Rgba> {
    let value = value.trim().trim_start_matches('#');
    if !matches!(value.len(), 6 | 8) {
        return None;
    }
    let alpha = value.len() == 8;
    let value = u32::from_str_radix(value, 16).ok()?;
    Some(if alpha { gpui::rgba(value) } else { rgb(value) })
}
