use super::*;
use crate::i18n::t;
use model::variants::ComponentSet;
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
mod view;

pub(in crate::editor) struct Rename {
    page: String,
    component: String,
    set: String,
    set_name: bool,
    input: Entity<TextInput>,
    error: bool,
    _subscriptions: Vec<Subscription>,
}

fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut value: String = base.chars().take(200).collect();
    let mut suffix = 2;
    while taken(&value) {
        let tail = format!(" {suffix}");
        value = base.chars().take(200 - tail.len()).collect::<String>() + &tail;
        suffix += 1;
    }
    value
}

impl Workspace {
    pub(in crate::editor) fn component_display_name(&self, id: &str) -> String {
        if let Some((_, set)) = self.variant_set(id) {
            format!("{} · {}", set.name, set.variants[id])
        } else {
            self.components.definitions[id].name.clone()
        }
    }

    fn selected_component(&self) -> Option<(usize, Binding)> {
        let ids = self.selection_ids();
        if ids.len() != 1 {
            return None;
        }
        let root = *ids.first()?;
        self.hierarchy
            .components
            .get(&root)
            .cloned()
            .map(|link| (root, link))
    }

    fn variant_set(&self, component: &str) -> Option<(&String, &ComponentSet)> {
        self.components
            .sets
            .iter()
            .find(|(_, set)| set.variants.contains_key(component))
    }

    fn selected_masters(&self) -> Option<Vec<String>> {
        if self.preview.is_some() || self.gesture.is_some() {
            return None;
        }
        let ids = self.selection_ids();
        if ids.len() < 2 {
            return None;
        }
        let mut components = Vec::new();
        let mut kind = None;
        for root in ids {
            if !self.layer_editable(root) {
                return None;
            }
            let binding = self.hierarchy.components.get(&root)?;
            if !binding.master || self.variant_set(&binding.component).is_some() {
                return None;
            }
            let definition = self.components.definitions.get(&binding.component)?;
            let next = model::variants::node_kind(&definition.page, definition.root);
            if kind.is_some_and(|kind| kind != next) {
                return None;
            }
            kind = Some(next);
            components.push(binding.component.clone());
        }
        Some(components)
    }

    fn combine_variants(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(components) = self.selected_masters() else {
            return;
        };
        self.suspend(window, cx);
        self.sync_components(window, cx);
        let before = self.page_edit(&[], cx);
        let mut set = ComponentSet {
            name: self.components.definitions[&components[0]].name.clone(),
            variants: BTreeMap::new(),
        };
        for id in components {
            let name = unique_name(&self.components.definitions[&id].name, |name| {
                set.variants.values().any(|v| v == name)
            });
            set.variants.insert(id, name);
        }
        self.components
            .sets
            .insert(uuid::Uuid::new_v4().to_string(), set);
        self.record_page_edit(before);
        cx.notify();
    }

    fn add_variant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((root, binding)) = self.selected_component() else {
            return;
        };
        if !binding.master || !self.layer_editable(root) || self.preview.is_some() {
            return;
        }
        self.suspend(window, cx);
        self.sync_components(window, cx);
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let mut definition = self.components.definitions[&binding.component].clone();
        definition.source = None;
        let set_id = self
            .variant_set(&binding.component)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| {
                let id = uuid::Uuid::new_v4().to_string();
                self.components.sets.insert(
                    id.clone(),
                    ComponentSet {
                        name: definition.name.clone(),
                        variants: BTreeMap::from([(binding.component.clone(), "Default".into())]),
                    },
                );
                id
            });
        let set = self.components.sets.get_mut(&set_id).unwrap();
        let name = unique_name(&set.variants[&binding.component], |name| {
            set.variants.values().any(|v| v == name)
        });
        let id = uuid::Uuid::new_v4().to_string();
        set.variants.insert(id.clone(), name);
        self.components.definitions.insert(id.clone(), definition);
        let rect = self.world_rect(root).unwrap();
        let suppressed = self.history.borrow().suppressed;
        self.history.borrow_mut().suppressed = true;
        self.insert_document_component(
            &id,
            true,
            Some(point(rect.x + rect.width + 40., rect.y)),
            window,
            cx,
        );
        self.history.borrow_mut().suppressed = suppressed;
        self.record_page_edit(before);
        self.components.revision = None;
        self.sidebar.resources = false;
        cx.notify();
    }

    fn switch_variant(
        &mut self,
        root: usize,
        page_id: &str,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pages.active != page_id || !self.layer_editable(root) || self.preview.is_some() {
            return;
        }
        let Some(binding) = self.hierarchy.components.get(&root) else {
            return;
        };
        if binding.component == target
            || self
                .variant_set(&binding.component)
                .is_none_or(|(_, set)| !set.variants.contains_key(target))
        {
            return;
        }
        if binding.master {
            self.edit_document_component(target, window, cx);
            return;
        }
        self.suspend(window, cx);
        self.sync_components(window, cx);
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let mut page = self.snapshot_page(cx).0;
        if let Err(error) = model::variants::switch(
            &mut page,
            root,
            target,
            &self.components.definitions,
            &self.components.sets,
        ) {
            self.assets.error = Some(error.to_string());
            return;
        }
        self.apply_component_page(page, window, cx);
        self.record_page_edit(before);
        self.components.revision = None;
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        cx.notify();
    }

    fn begin_variant_rename(
        &mut self,
        set_name: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((root, binding)) = self.selected_component() else {
            return;
        };
        if !binding.master || !self.layer_editable(root) || self.preview.is_some() {
            return;
        }
        let Some((id, set)) = self.variant_set(&binding.component) else {
            return;
        };
        let id = id.clone();
        let name = if set_name {
            &set.name
        } else {
            &set.variants[&binding.component]
        }
        .clone();
        self.suspend(window, cx);
        let input = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.set_value(name, cx);
            input
        });
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit(_)) {
                this.finish_variant_rename(true, window, cx);
            }
        });
        let blur = cx.on_blur(&input.focus_handle(cx), window, |this, window, cx| {
            this.finish_variant_rename(true, window, cx);
        });
        input.focus_handle(cx).focus(window, cx);
        self.components.rename = Some(Rename {
            page: self.pages.active.clone(),
            component: binding.component,
            set: id,
            set_name,
            input,
            error: false,
            _subscriptions: vec![submit, blur],
        });
        cx.notify();
    }

    pub(in crate::editor) fn finish_variant_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.components.rename.take() else {
            return;
        };
        let focused = rename.input.focus_handle(cx).is_focused(window);
        let valid_target = rename.page == self.pages.active
            && self.selected_component().is_some_and(|(root, b)| {
                b.master && b.component == rename.component && self.layer_editable(root)
            });
        if commit && valid_target {
            let name = rename.input.read(cx).value().trim().to_owned();
            if let Some(set) = self.components.sets.get(&rename.set) {
                let valid = !name.is_empty()
                    && name.chars().count() <= 200
                    && (rename.set_name
                        || !set
                            .variants
                            .iter()
                            .any(|(id, value)| id != &rename.component && value == &name));
                if !valid {
                    if focused {
                        self.components.rename = Some(Rename {
                            error: true,
                            ..rename
                        });
                    }
                    cx.notify();
                    return;
                }
                let old = if rename.set_name {
                    &set.name
                } else {
                    &set.variants[&rename.component]
                };
                if old != &name {
                    let before = self.page_edit(&[], cx);
                    let set = self.components.sets.get_mut(&rename.set).unwrap();
                    if rename.set_name {
                        set.name = name;
                    } else {
                        set.variants.insert(rename.component, name);
                    }
                    self.record_page_edit(before);
                }
            }
        }
        if focused {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
}
