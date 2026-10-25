use super::*;
use crate::agent::protocol::{self as p, Error, Request, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod geometry;
mod history;
mod import;
mod interaction;
mod model;
mod pages;
mod structure;
#[cfg(test)]
mod tests;
mod view;

pub(super) struct State {
    session: String,
    receipts: BTreeMap<String, ([u8; 32], Value)>,
    receipt_bytes: usize,
}

fn fingerprint(request: &Request) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut value =
        serde_json::to_value(request).map_err(|error| Error::invalid(error.to_string()))?;
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|error| Error::invalid(error.to_string()))?;
    Ok(Sha256::digest(bytes).into())
}
impl Default for State {
    fn default() -> Self {
        Self {
            session: uuid::Uuid::new_v4().to_string(),
            receipts: BTreeMap::new(),
            receipt_bytes: 0,
        }
    }
}

impl Workspace {
    pub(crate) fn agent_session_id(&self) -> &str {
        &self.agent.session
    }

    pub(crate) fn agent_scope(&self, document_id: &str) -> Value {
        json!({"session_id":self.agent.session,"document_id":document_id,
            "page_id":self.pages.active,"revision":self.document_revision().to_string(),
            "history":{"can_undo":self.can_undo_redo(false),"can_redo":self.can_undo_redo(true)}})
    }

    pub(crate) fn agent_session(
        &self,
        document_id: &str,
        title: &str,
        window: &Window,
        cx: &gpui::App,
    ) -> Value {
        let (page, _) = self.snapshot_page(cx);
        let identities = page.node_ids();
        json!({"session_id":self.agent.session,"document_id":document_id,"title":title,
            "page_id":self.pages.active,"revision":self.document_revision().to_string(),
            "pages":self.pages.entries.iter().map(|p| json!({"id":p.page.id,"name":p.page.name})).collect::<Vec<_>>(),
            "components":self.components.definitions.iter().map(|(id,d)|json!({"id":id,"name":d.name})).collect::<Vec<_>>(),
            "selection":self.selection_ids().iter().filter_map(|id| identities.get(id)).collect::<Vec<_>>(),
            "view":self.agent_view(),
            "history":{"can_undo":self.can_undo_redo(false),"can_redo":self.can_undo_redo(true)},
            "start_frame_id":page.hierarchy.start.and_then(|id| identities.get(&id)),
            "busy":self.agent_busy(window,cx),"remaining_apply_receipts":p::MAX_RECEIPTS-self.agent.receipts.len(),
            "remaining_receipt_bytes":p::MAX_RECEIPT_BYTES-self.agent.receipt_bytes})
    }

    pub(crate) fn agent_busy(&self, window: &Window, cx: &gpui::App) -> bool {
        !self.sync_ready(cx)
            || self.history.borrow().is_previewing()
            || self.preview_read_only()
            || self.presentation.player.is_some()
            || self.draft.is_some()
            || self.gesture.is_some()
            || self.box_draft.is_some()
            || self.layer_drag.is_some()
            || self.selection_resize.is_some()
            || self.path_before.is_some()
            || self.pages.renaming.is_some()
            || self.sidebar.renaming.is_some()
            || self.corner_editor.editing()
            || self.spacing.editing()
            || self.texts.iter().any(|t| t.editor.read(cx).editing)
            || self
                .inspector
                .fields
                .iter()
                .any(|f| f.read(cx).focus_handle(cx).is_focused(window))
            || self
                .rename_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
    }

    pub(crate) fn agent_request(
        &mut self,
        request: &Request,
        imports: Option<&crate::agent::imports::Prepared>,
        active_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<p::Reply> {
        if request.session_id.as_deref() != Some(self.agent.session.as_str()) {
            return Err(Error::new(
                "SESSION_CLOSED",
                "Session changed; call session.describe",
            ));
        }
        if let Some((original, result)) = self.agent.receipts.get(&request.request_id) {
            if *original != fingerprint(request)? {
                return Err(Error::new(
                    "REQUEST_ID_REUSED",
                    "Use a new request_id for a different request",
                ));
            }
            return Ok(p::Reply::Ready(result.clone()));
        }
        if self.agent_busy(window, cx) {
            return Err(Error::new(
                "BUSY",
                "Finish the current editor interaction before requesting a snapshot or edit",
            ));
        }
        if !active_tab
            && matches!(
                request.method.as_str(),
                "pages.activate"
                    | "selection.set"
                    | "canvas.focus"
                    | "canvas.fit"
                    | "document.undo"
                    | "document.redo"
            )
        {
            return Err(Error::new(
                "TAB_NOT_ACTIVE",
                "Call tabs.activate before changing the view",
            ));
        }
        match request.method.as_str() {
            "pages.list" => {
                let _: p::Empty = p::parse(request.params.clone())?;
                Ok(p::Reply::Ready(self.agent_page_list(cx)))
            }
            "pages.apply" => self.agent_apply_pages(request, window, cx),
            "pages.activate" => self.agent_activate_page(request, window, cx),
            "nodes.get" | "nodes.find" => {
                let params: p::Read = p::parse(request.params.clone())?;
                self.agent_guard(&params.page_id, &params.revision)?;
                if params.limit == 0
                    || params.limit > p::MAX_NODES
                    || params.ids.len() > p::MAX_NODES
                {
                    return Err(Error::invalid(
                        "Read limit and ID count must not exceed 200",
                    ));
                }
                if request.method == "nodes.get"
                    && (params.name.is_some() || params.node_type.is_some())
                {
                    return Err(Error::invalid("Use nodes.find for name/type filters"));
                }
                let page = self.agent_page(&params.page_id, cx);
                let identities = page.node_ids();
                let mut ids = identities.keys().copied().collect::<Vec<_>>();
                if !params.ids.is_empty() {
                    ids = params
                        .ids
                        .iter()
                        .map(|id| model::lookup(&page, id, &BTreeMap::new()))
                        .collect::<Result<_>>()?;
                }
                let ranks: BTreeMap<_, _> = page
                    .hierarchy
                    .order
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (*id, i))
                    .collect();
                ids.sort_by_key(|id| ranks.get(id).map_or((1, *id), |i| (0, *i)));
                let mut nodes = Vec::new();
                let mut total = 0;
                let name = params.name.as_ref().map(|name| name.to_lowercase());
                for node in ids
                    .into_iter()
                    .map(|id| {
                        let mut node = model::node(&page, id, &identities);
                        node["available_variants"] = json!(
                            interaction::variants(&page, id, &self.components.sets)
                                .into_iter()
                                .map(|(id, name)| json!({"id":id,"name":name}))
                                .collect::<Vec<_>>()
                        );
                        node
                    })
                    .filter(|node| {
                        name.as_ref().is_none_or(|name| {
                            node["properties"]["name"]
                                .as_str()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(name)
                        }) && params
                            .node_type
                            .as_ref()
                            .is_none_or(|kind| node["type"].as_str() == Some(kind))
                    })
                {
                    if total >= params.offset && nodes.len() < params.limit {
                        nodes.push(node);
                    }
                    total += 1;
                }
                let end = params.offset.saturating_add(params.limit).min(total);
                Ok(p::Reply::Ready(
                    json!({"revision":self.document_revision().to_string(),"page_id":page.id,
                    "nodes":nodes,
                    "total":total,"next_offset":(end<total).then_some(end)}),
                ))
            }
            "document.apply" => self.agent_apply(request, imports, window, cx),
            "document.undo" | "document.redo" => self.agent_history(request, window, cx),
            "canvas.capture" => self.agent_capture(p::parse(request.params.clone())?, window, cx),
            "selection.set" | "canvas.focus" | "canvas.fit" => self
                .agent_view_request(&request.method, p::parse(request.params.clone())?, cx)
                .map(p::Reply::Ready),
            _ => Err(Error::new(
                "UNSUPPORTED",
                "Unknown method; call session.describe",
            )),
        }
    }

    fn agent_guard(&self, page: &str, revision: &str) -> Result<()> {
        if !self.pages.entries.iter().any(|entry| entry.page.id == page) {
            return Err(Error::new(
                "PAGE_NOT_FOUND",
                "Page no longer exists; call pages.list",
            ));
        }
        if revision != self.document_revision().to_string() {
            return Err(Error::new(
                "REVISION_CONFLICT",
                format!(
                    "Current revision is {}; reread before editing",
                    self.document_revision()
                ),
            ));
        }
        Ok(())
    }

    fn agent_apply(
        &mut self,
        request: &Request,
        imports: Option<&crate::agent::imports::Prepared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<p::Reply> {
        let params: p::Apply = p::parse(request.params.clone())?;
        self.agent_guard(&params.page_id, &params.base_revision)?;
        if params.operations.is_empty() || params.operations.len() > p::MAX_OPERATIONS {
            return Err(Error::invalid("operations must contain 1 to 128 entries"));
        }
        if self.agent.receipts.len() >= p::MAX_RECEIPTS {
            return Err(Error::new(
                "LIMIT_REACHED",
                "Session receipt capacity reached; reopen the document for a new session",
            ));
        }
        if imports.is_none() {
            let sources: Vec<_> = params
                .operations
                .iter()
                .enumerate()
                .filter_map(|(index, operation)| {
                    if let p::Operation::Import { source, .. }
                    | p::Operation::ReplaceSource { source, .. } = operation
                    {
                        Some((index, source.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            if !sources.is_empty() {
                return Ok(p::Reply::PrepareImports(sources));
            }
        }
        let original = self.agent_page(&params.page_id, cx);
        let mut page = original.clone();
        let mut refs = BTreeMap::new();
        let mut component_refs = BTreeMap::new();
        let mut definitions = self.components.definitions.clone();
        for (index, op) in params.operations.into_iter().enumerate() {
            let reference = match &op {
                p::Operation::Create { reference, .. }
                | p::Operation::Import { reference, .. }
                | p::Operation::Duplicate { reference, .. }
                | p::Operation::CreateComponent { reference, .. }
                | p::Operation::InstantiateComponent { reference, .. } => Some(reference),
                _ => None,
            };
            if let Some(reference) = reference {
                model::validate_reference(reference, &refs)
                    .and_then(|()| model::validate_reference(reference, &component_refs))
                    .map_err(|mut e| {
                        e.operation = Some(index);
                        e
                    })?;
            }
            let props = match &op {
                p::Operation::Create { properties, .. }
                | p::Operation::Update { properties, .. } => Some(properties),
                _ => None,
            };
            let family = match &op {
                p::Operation::SetInteraction {
                    action:
                        p::interaction::Action::SetProperty {
                            property:
                                crate::scene::presentation::property::Property::FontFamily(family),
                            ..
                        },
                    ..
                } => Some(family),
                _ => props.and_then(|p| p.font_family.as_ref()),
            };
            if let Some(family) = family
                && family != crate::ui::font::SYSTEM
                && !cx
                    .text_system()
                    .all_font_names()
                    .iter()
                    .any(|f| f == family)
            {
                let mut error = Error::new(
                    "FONT_NOT_FOUND",
                    format!("Font family is unavailable: {family}"),
                );
                error.operation = Some(index);
                return Err(error);
            }
            let result = if matches!(
                op,
                p::Operation::CreateComponent { .. } | p::Operation::InstantiateComponent { .. }
            ) {
                (|| {
                    // An instance created later in a batch sees earlier master edits.
                    crate::scene::components::synchronize(
                        std::slice::from_mut(&mut page),
                        &mut definitions,
                    )
                    .map_err(|e| Error::invalid(e.to_string()))?;
                    match op {
                        p::Operation::CreateComponent {
                            id,
                            reference,
                            name,
                        } => {
                            model::validate_reference(&reference, &component_refs)?;
                            let id = model::lookup(&page, &id, &refs)?;
                            let component =
                                structure::create_component(&mut page, &mut definitions, id, name)?;
                            component_refs.insert(reference, component);
                        }
                        p::Operation::InstantiateComponent {
                            component,
                            reference,
                            parent,
                            position,
                        } => {
                            model::validate_reference(&reference, &refs)?;
                            let component =
                                component_refs.get(&component).cloned().unwrap_or(component);
                            let definition = definitions.get(&component).ok_or_else(|| {
                                Error::new("COMPONENT_NOT_FOUND", "Unknown component definition")
                            })?;
                            let parent = parent
                                .as_deref()
                                .map(|id| model::lookup(&page, id, &refs))
                                .transpose()?;
                            let root = structure::instantiate(
                                &mut page, definition, component, parent, position,
                            )?;
                            refs.insert(reference, page.node_ids()[&root].to_string());
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                })()
            } else if matches!(
                op,
                p::Operation::Import { .. } | p::Operation::ReplaceSource { .. }
            ) {
                let asset = imports
                    .and_then(|assets| assets.get(&index))
                    .ok_or_else(|| Error::new("INTERNAL", "Missing prepared asset"))?;
                if let p::Operation::ReplaceSource { id, source } = op {
                    import::replace(&mut page, &id, source, asset.clone(), &refs)
                } else {
                    import::insert(&mut page, op, asset.clone(), &mut refs)
                }
            } else {
                interaction::apply(&mut page, op, &mut refs, &self.components.sets)
            };
            result.map_err(|mut e| {
                e.operation = Some(index);
                e
            })?;
        }
        page.validate().map_err(|e| Error::invalid(e.to_string()))?;
        structure::validate(&page)?;
        crate::scene::auto_layout::resolve(&mut page, window.text_system())
            .map_err(|e| Error::invalid(e.to_string()))?;
        page.validate().map_err(|e| Error::invalid(e.to_string()))?;
        model::protect(&original, &page)?;
        // Validate the edited page in its complete component/document context as well.
        let before_pages = self.agent_pages(cx);
        let mut document = crate::document::Document {
            id: request
                .document_id
                .clone()
                .ok_or_else(|| Error::invalid("document_id is required"))?,
            colors: self.colors.palette.clone(),
            components: definitions,
            component_sets: self.components.sets.clone(),
            pages: before_pages
                .iter()
                .map(|p| {
                    if p.id == page.id {
                        page.clone()
                    } else {
                        p.clone()
                    }
                })
                .collect(),
        };
        crate::scene::components::synchronize(&mut document.pages, &mut document.components)
            .map_err(|e| Error::invalid(e.to_string()))?;
        for (before, after) in before_pages.iter().zip(&mut document.pages) {
            if before == after {
                continue;
            }
            crate::scene::auto_layout::resolve(after, window.text_system())
                .map_err(|e| Error::invalid(e.to_string()))?;
            model::protect(before, after)?;
            structure::validate(after)?;
        }
        document
            .validate()
            .map_err(|e| Error::invalid(e.to_string()))?;
        let changed_pages = document
            .pages
            .iter()
            .zip(&before_pages)
            .filter(|(after, before)| after != before)
            .map(|(after, _)| after.id.clone())
            .collect::<Vec<_>>();
        page = document
            .pages
            .iter()
            .find(|p| p.id == params.page_id)
            .unwrap()
            .clone();
        let changed =
            !changed_pages.is_empty() || document.components != self.components.definitions;
        let before_ids = original.node_ids();
        let after_ids = page.node_ids();
        refs.retain(|_, uid| after_ids.values().any(|value| value.to_string() == *uid));
        let updated = after_ids
            .keys()
            .filter_map(|id| {
                let value = model::node(&page, *id, &after_ids);
                (!before_ids.contains_key(id) || value != model::node(&original, *id, &before_ids))
                    .then_some(value)
            })
            .collect::<Vec<_>>();
        let revision = self
            .document_revision()
            .checked_add(u64::from(changed))
            .ok_or_else(|| Error::new("LIMIT_REACHED", "Document revision exhausted"))?;
        let result = json!({"revision":revision.to_string(),"page_id":page.id,
            "changed":changed,"created":refs,"created_components":component_refs,"changed_pages":changed_pages,"nodes":updated,
            "start_frame_id":page.hierarchy.start.and_then(|id| after_ids.get(&id)),
            "deleted":before_ids.iter().filter_map(|(id,uid)|(!after_ids.contains_key(id)).then_some(uid)).collect::<Vec<_>>()});
        let (fingerprint, receipt_bytes) = self.agent_receipt(request, &result)?;
        if changed {
            // All fallible work precedes this point. Reuse the ordinary page undo entry.
            let before = self.page_edit(&changed_pages, cx);
            let selection = self.selection_ids();
            self.history.borrow_mut().break_group();
            self.shape_paths.borrow_mut().clear();
            *self.boolean_cache.borrow_mut() = Default::default();
            self.components.revision = None;
            self.components.definitions = document.components;
            for updated in document.pages {
                if let Some(state) = self
                    .pages
                    .entries
                    .iter_mut()
                    .find(|s| s.page.id == updated.id)
                {
                    let remaining = crate::scene::components::ids(&updated);
                    state.selection.retain(|id| remaining.contains(id));
                    state.folded.retain(|id| remaining.contains(id));
                    state.page = updated;
                }
            }
            if changed_pages.contains(&self.pages.active) {
                let active = self.pages.current().page.clone();
                let remaining = crate::scene::components::ids(&active);
                self.load_page(active, window, cx);
                self.set_selection(selection.intersection(&remaining).copied().collect(), cx);
            }
            self.record_page_edit(before);
            cx.notify();
        }
        self.agent.receipt_bytes += receipt_bytes;
        self.agent
            .receipts
            .insert(request.request_id.clone(), (fingerprint, result.clone()));
        Ok(p::Reply::Ready(result))
    }

    fn agent_capture(
        &self,
        params: p::Capture,
        window: &Window,
        cx: &gpui::App,
    ) -> Result<p::Reply> {
        self.agent_guard(&params.page_id, &params.revision)?;
        if !(1..=4).contains(&params.scale) {
            return Err(Error::invalid("scale must be 1 through 4"));
        }
        let page = self.agent_page(&params.page_id, cx);
        let id = model::lookup(&page, &params.node_id, &BTreeMap::new())?;
        let subtree = crate::scene::components::subtree(&page, id);
        if page
            .shapes
            .iter()
            .any(|n| subtree.contains(&n.id) && n.kind.is_playable())
        {
            return Err(Error::new(
                "UNSUPPORTED",
                "Media capture is not supported by the export renderer",
            ));
        }
        let mut jobs = super::export::Snapshot::new(page)
            .jobs([id].into(), window)
            .map_err(|e| Error::new("CAPTURE_FAILED", e.to_string()))?;
        let job = jobs.remove(0);
        let width = (job.bounds.width * params.scale as f32).ceil();
        let height = (job.bounds.height * params.scale as f32).ceil();
        if width * height > 4_000_000. {
            return Err(Error::new(
                "LIMIT_REACHED",
                "Capture exceeds 4 million pixels; use a smaller node or scale",
            ));
        }
        let metadata = json!({"revision":params.revision,"page_id":params.page_id,"node_id":params.node_id,
            "canvas_rect":job.bounds,"width":width as u32,"height":height as u32,
            "renderer":"export","mime_type":"image/png"});
        Ok(p::Reply::Capture {
            job: Box::new(job),
            scale: params.scale,
            metadata,
        })
    }
}
