use super::*;

impl Workspace {
    pub(super) fn agent_history(
        &mut self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<p::Reply> {
        let params: p::HistoryTarget = p::parse(request.params.clone())?;
        self.agent_guard(&self.pages.active, &params.base_revision)?;
        let redo = request.method == "document.redo";
        let changed = self.can_undo_redo(redo);
        let revision = self
            .document_revision()
            .checked_add(u64::from(changed))
            .ok_or_else(|| Error::new("LIMIT_REACHED", "Document revision exhausted"))?;
        let result = json!({"revision":revision.to_string(),"changed":changed});
        // Reserve the acknowledgement before touching history, including for no-ops.
        let (fingerprint, bytes) = self.agent_receipt(request, &result)?;
        if changed {
            self.undo_redo(redo, window, cx);
        }
        self.agent.receipt_bytes += bytes;
        self.agent
            .receipts
            .insert(request.request_id.clone(), (fingerprint, result.clone()));
        Ok(p::Reply::Ready(result))
    }
}
