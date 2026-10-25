use super::*;
use std::collections::BTreeSet;

impl Workspace {
    pub(super) fn agent_view(&self) -> Value {
        json!({"zoom":self.view.zoom,"pan":{"x":self.view.pan.x,"y":self.view.pan.y}})
    }

    pub(super) fn agent_view_request(
        &mut self,
        method: &str,
        params: p::Targets,
        cx: &mut Context<Self>,
    ) -> Result<Value> {
        self.agent_guard(&params.page_id, &params.revision)?;
        if params.page_id != self.pages.active {
            return Err(Error::new(
                "PAGE_NOT_ACTIVE",
                "Call pages.activate before changing the view",
            ));
        }
        if params.ids.len() > p::MAX_NODES {
            return Err(Error::invalid("ID count must not exceed 200"));
        }
        let (page, _) = self.snapshot_page(cx);
        let ids = params
            .ids
            .iter()
            .map(|id| model::lookup(&page, id, &BTreeMap::new()))
            .collect::<Result<BTreeSet<_>>>()?;
        if method == "selection.set" {
            if ids.iter().any(|id| !self.layer_editable(*id)) {
                return Err(Error::new(
                    "NOT_SELECTABLE",
                    "Hidden or locked nodes and their descendants cannot be selected",
                ));
            }
            self.set_selection(ids, cx);
        } else {
            if method == "canvas.focus" && ids.is_empty() {
                return Err(Error::invalid("canvas.focus requires at least one ID"));
            }
            let visible: BTreeSet<_> = self.canvas_layer_order().into_iter().collect();
            if !ids.is_subset(&visible) {
                return Err(Error::new(
                    "NOT_VISIBLE",
                    "Target or its ancestor is hidden",
                ));
            }
            let targets = if ids.is_empty() { &visible } else { &ids };
            if ids.iter().any(|id| self.world_bounds(*id).is_none()) {
                return Err(Error::invalid("Target has no canvas bounds"));
            }
            let rect = targets
                .iter()
                .filter_map(|id| self.world_bounds(*id))
                .reduce(|a, b| {
                    let x = a.x.min(b.x);
                    let y = a.y.min(b.y);
                    Rect {
                        x,
                        y,
                        width: (a.x + a.width).max(b.x + b.width) - x,
                        height: (a.y + a.height).max(b.y + b.height) - y,
                    }
                })
                .ok_or_else(|| Error::invalid("No visible content to frame"))?;
            let size = self.bounds.get().size;
            let (left, right) = self.canvas_insets();
            if f32::from(size.width) <= left + right || f32::from(size.height) <= 0. {
                return Err(Error::new("BUSY", "Canvas layout is not ready"));
            }
            self.frame_canvas_rect(rect, method == "canvas.fit", cx);
        }
        let identities = page.node_ids();
        Ok(
            json!({"revision":self.document_revision().to_string(),"page_id":page.id,
            "selection":self.selection_ids().iter().filter_map(|id| identities.get(id)).collect::<Vec<_>>(),
            "view":self.agent_view()}),
        )
    }
}
