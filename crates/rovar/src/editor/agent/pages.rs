use super::*;
use crate::document::Page;
use crate::editor::pages::PageState;

fn lookup(pages: &[Page], id: &str, refs: &BTreeMap<String, String>) -> Result<usize> {
    let id = refs.get(id).map_or(id, String::as_str);
    pages
        .iter()
        .position(|page| page.id == id)
        .ok_or_else(|| Error::new("PAGE_NOT_FOUND", format!("Unknown page: {id}")))
}

fn insert(
    pages: &mut Vec<Page>,
    page: Page,
    before: Option<&str>,
    refs: &BTreeMap<String, String>,
) -> Result<()> {
    let index = before
        .map(|id| lookup(pages, id, refs))
        .transpose()?
        .unwrap_or(pages.len());
    pages.insert(index, page);
    Ok(())
}

impl Workspace {
    pub(super) fn agent_page(&self, id: &str, cx: &gpui::App) -> Page {
        if id == self.pages.active {
            self.snapshot_page(cx).0
        } else {
            self.pages
                .entries
                .iter()
                .find(|state| state.page.id == id)
                .expect("validated page")
                .page
                .clone()
        }
    }

    pub(super) fn agent_pages(&self, cx: &gpui::App) -> Vec<Page> {
        self.pages
            .entries
            .iter()
            .map(|state| self.agent_page(&state.page.id, cx))
            .collect()
    }

    pub(super) fn agent_page_list(&self, cx: &gpui::App) -> Value {
        json!({"revision":self.document_revision().to_string(),"active_page_id":self.pages.active,
            "pages":self.agent_pages(cx).iter().map(|page| json!({"id":page.id,"name":page.name,
                "node_count":page.node_ids().len(),"frame_count":page.boards.len()})).collect::<Vec<_>>()})
    }

    pub(super) fn agent_receipt(
        &self,
        request: &Request,
        result: &Value,
    ) -> Result<([u8; 32], usize)> {
        let fingerprint = fingerprint(request)?;
        let bytes = serde_json::to_vec(&(fingerprint, result))
            .map_err(|e| Error::invalid(e.to_string()))?
            .len();
        if self.agent.receipts.len() >= p::MAX_RECEIPTS
            || self.agent.receipt_bytes.saturating_add(bytes) > p::MAX_RECEIPT_BYTES
        {
            return Err(Error::new(
                "LIMIT_REACHED",
                "Session receipt capacity reached; reopen the document for a new session",
            ));
        }
        Ok((fingerprint, bytes))
    }

    pub(super) fn agent_activate_page(
        &mut self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<p::Reply> {
        let params: p::PageTarget = p::parse(request.params.clone())?;
        self.agent_guard(&params.page_id, &params.revision)?;
        let result = json!({"revision":params.revision,"active_page_id":params.page_id,
            "changed":params.page_id != self.pages.active});
        let (fingerprint, bytes) = self.agent_receipt(request, &result)?;
        if params.page_id != self.pages.active {
            self.park_page(cx);
            self.activate_page(&params.page_id, window, cx);
        }
        self.agent.receipt_bytes += bytes;
        self.agent
            .receipts
            .insert(request.request_id.clone(), (fingerprint, result.clone()));
        Ok(p::Reply::Ready(result))
    }

    pub(super) fn agent_apply_pages(
        &mut self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<p::Reply> {
        let params: p::ApplyPages = p::parse(request.params.clone())?;
        self.agent_guard(&self.pages.active, &params.base_revision)?;
        if params.operations.is_empty() || params.operations.len() > p::MAX_OPERATIONS {
            return Err(Error::invalid("operations must contain 1 to 128 entries"));
        }
        let original = self.agent_pages(cx);
        let mut pages = original.clone();
        let mut refs = BTreeMap::new();
        for (index, operation) in params.operations.into_iter().enumerate() {
            let result = (|| {
                match &operation {
                    p::PageOperation::Create {
                        reference, name, ..
                    }
                    | p::PageOperation::Duplicate {
                        reference, name, ..
                    } => {
                        model::validate_reference(reference, &refs)?;
                        validate_name(name)?;
                    }
                    p::PageOperation::Rename { name, .. } => validate_name(name)?,
                    _ => {}
                }
                match operation {
                    p::PageOperation::Create {
                        reference,
                        name,
                        before,
                    } => {
                        let page = Page::empty(name);
                        refs.insert(reference, page.id.clone());
                        insert(&mut pages, page, before.as_deref(), &refs)?;
                    }
                    p::PageOperation::Duplicate {
                        id,
                        reference,
                        name,
                        before,
                    } => {
                        let mut page = pages[lookup(&pages, &id, &refs)?].clone();
                        page.id = uuid::Uuid::new_v4().to_string();
                        page.name = name;
                        page.renew_node_ids();
                        for binding in page.hierarchy.components.values_mut() {
                            binding.master = false;
                        }
                        refs.insert(reference, page.id.clone());
                        insert(&mut pages, page, before.as_deref(), &refs)?;
                    }
                    p::PageOperation::Rename { id, name } => {
                        let index = lookup(&pages, &id, &refs)?;
                        pages[index].name = name;
                    }
                    p::PageOperation::Delete { id } => {
                        let index = lookup(&pages, &id, &refs)?;
                        pages.remove(index);
                    }
                    p::PageOperation::Reorder { id, before } => {
                        let index = lookup(&pages, &id, &refs)?;
                        if before
                            .as_deref()
                            .map(|id| lookup(&pages, id, &refs))
                            .transpose()?
                            == Some(index)
                        {
                            return Ok(());
                        }
                        let page = pages.remove(index);
                        insert(&mut pages, page, before.as_deref(), &refs)?;
                    }
                }
                Ok(())
            })();
            result.map_err(|mut error: Error| {
                error.operation = Some(index);
                error
            })?;
        }
        if pages.is_empty() {
            return Err(Error::invalid("A document must retain at least one page"));
        }
        let document = crate::document::Document {
            id: request
                .document_id
                .clone()
                .ok_or_else(|| Error::invalid("document_id is required"))?,
            colors: self.colors.palette.clone(),
            components: self.components.definitions.clone(),
            component_sets: self.components.sets.clone(),
            pages,
        };
        document
            .validate()
            .map_err(|e| Error::invalid(e.to_string()))?;
        let pages = document.pages;
        let changed = pages != original;
        let active = if pages.iter().any(|page| page.id == self.pages.active) {
            self.pages.active.clone()
        } else {
            let index = original
                .iter()
                .position(|page| page.id == self.pages.active)
                .unwrap();
            original[index + 1..]
                .iter()
                .chain(original[..index].iter().rev())
                .find(|old| pages.iter().any(|page| page.id == old.id))
                .unwrap_or(&pages[0])
                .id
                .clone()
        };
        refs.retain(|_, id| pages.iter().any(|page| page.id == *id));
        let revision = self
            .document_revision()
            .checked_add(u64::from(changed))
            .ok_or_else(|| Error::new("LIMIT_REACHED", "Document revision exhausted"))?;
        let result = json!({"revision":revision.to_string(),"active_page_id":active,"changed":changed,"created":refs,
            "pages":pages.iter().map(|page| json!({"id":page.id,"name":page.name})).collect::<Vec<_>>()});
        let (fingerprint, bytes) = self.agent_receipt(request, &result)?;
        if changed {
            let ids = original
                .iter()
                .chain(&pages)
                .filter(|page| {
                    original.iter().find(|old| old.id == page.id)
                        != pages.iter().find(|new| new.id == page.id)
                })
                .map(|page| page.id.clone())
                .collect::<Vec<_>>();
            let before = self.page_edit(&ids, cx);
            self.park_page(cx);
            let mut states = std::mem::take(&mut self.pages.entries);
            self.pages.entries = pages
                .into_iter()
                .map(|page| {
                    if let Some(index) = states.iter().position(|state| state.page.id == page.id) {
                        let mut state = states.remove(index);
                        state.page = page;
                        state
                    } else {
                        PageState::new(page)
                    }
                })
                .collect();
            if active != self.pages.active {
                self.restore_page(&active, window, cx);
            }
            self.record_page_edit(before);
            cx.notify();
        }
        self.agent.receipt_bytes += bytes;
        self.agent
            .receipts
            .insert(request.request_id.clone(), (fingerprint, result.clone()));
        Ok(p::Reply::Ready(result))
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 200 {
        return Err(Error::invalid("Page name must contain 1 to 200 characters"));
    }
    Ok(())
}
