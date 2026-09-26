use super::*;
use crate::document::{AssetSource, Document};
use std::collections::BTreeSet;

pub(in crate::workspace) struct SavedComponent {
    pub json: Vec<u8>,
    pub sources: Vec<AssetSource>,
    pub size: [f32; 2],
}

impl Workspace {
    pub(super) fn component_snapshot(&self, cx: &gpui::App) -> anyhow::Result<SavedComponent> {
        let roots = self.selection_ids();
        let included = self.descendants(&roots);
        let bounds = roots
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
            .ok_or_else(|| anyhow::anyhow!("Select a component first"))?;
        let (json, mut sources) = self.snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)?;
        let mut document = Document::decode(&json)?;
        document.boards.retain(|item| included.contains(&item.id));
        document.shapes.retain(|item| included.contains(&item.id));
        document.texts.retain(|item| included.contains(&item.id));
        let boards: BTreeSet<_> = document.boards.iter().map(|b| b.id).collect();
        for board in &mut document.boards {
            board.rect.x -= bounds.x;
            board.rect.y -= bounds.y;
        }
        for shape in &mut document.shapes {
            if shape.board.is_none_or(|id| !boards.contains(&id)) {
                shape.rect = self.world_rect(shape.id).unwrap();
                shape.rect.x -= bounds.x;
                shape.rect.y -= bounds.y;
                shape.board = None;
            }
        }
        for text in &mut document.texts {
            if text.board.is_none_or(|id| !boards.contains(&id)) {
                text.rect = self.world_rect(text.id).unwrap();
                text.rect.x -= bounds.x;
                text.rect.y -= bounds.y;
                text.board = None;
            }
        }
        document
            .hierarchy
            .groups
            .retain(|id, _| included.contains(id));
        for group in document.hierarchy.groups.values_mut() {
            group.board = group.board.filter(|id| boards.contains(id));
        }
        document
            .hierarchy
            .parents
            .retain(|id, parent| included.contains(id) && included.contains(parent));
        document
            .hierarchy
            .names
            .retain(|id, _| included.contains(id));
        document.hierarchy.order.retain(|id| included.contains(id));
        if roots.len() > 1 {
            let id = document.next_id;
            document.next_id += 1;
            document.hierarchy.groups.insert(
                id,
                crate::layer::LayerGroup {
                    name: t("asset-default-name").into(),
                    board: None,
                    layer: Default::default(),
                },
            );
            for root in roots {
                document.hierarchy.parents.insert(root, id);
            }
            document.hierarchy.order.push(id);
        }
        document
            .assets
            .retain(|asset| included.contains(&asset.object));
        let hashes: BTreeSet<_> = document.assets.iter().map(|a| &a.hash).collect();
        sources.retain(|a| hashes.contains(&a.hash));
        document.validate()?;
        Ok(SavedComponent {
            json: serde_json::to_vec(&document)?,
            sources,
            size: [bounds.width, bounds.height],
        })
    }
}
