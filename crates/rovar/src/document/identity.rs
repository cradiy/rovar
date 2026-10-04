use super::{Document, Page};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// Deterministic UUIDv8 for migration and component-generated nodes. The domain
/// distinguishes identities derived from old numeric IDs from instance children.
pub(crate) fn derived(domain: &[u8], namespace: &[u8], value: &[u8]) -> Uuid {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((namespace.len() as u64).to_le_bytes());
    hash.update(namespace);
    hash.update(value);
    let digest = hash.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

impl Document {
    pub(super) fn upgrade_node_ids(&mut self) {
        for page in self
            .pages
            .iter_mut()
            .chain(self.components.values_mut().map(|d| &mut d.page))
        {
            page.upgrade_node_ids();
        }
    }
}

impl Page {
    pub(crate) fn node_ids(&self) -> BTreeMap<usize, Uuid> {
        self.boards
            .iter()
            .map(|n| (n.id, n.uid))
            .chain(self.shapes.iter().map(|n| (n.id, n.uid)))
            .chain(self.texts.iter().map(|n| (n.id, n.uid)))
            .chain(self.hierarchy.groups.iter().map(|(id, n)| (*id, n.uid)))
            .collect()
    }

    fn map_node_ids(&mut self, mut map: impl FnMut(usize, Uuid) -> Uuid) {
        for node in &mut self.boards {
            node.uid = map(node.id, node.uid);
        }
        for node in &mut self.shapes {
            node.uid = map(node.id, node.uid);
        }
        for node in &mut self.texts {
            node.uid = map(node.id, node.uid);
        }
        for (id, node) in &mut self.hierarchy.groups {
            node.uid = map(*id, node.uid);
        }
    }

    /// Old files must acquire the same identities independently on every device.
    /// New objects receive UUIDv4 at creation, never from their numeric handle.
    pub(crate) fn upgrade_node_ids(&mut self) {
        let page = self.id.clone();
        self.map_node_ids(|id, uid| {
            if uid.is_nil() {
                derived(
                    b"rovar/legacy-node/v1",
                    page.as_bytes(),
                    &(id as u64).to_le_bytes(),
                )
            } else {
                uid
            }
        });
        for binding in self.hierarchy.components.values_mut() {
            upgrade_json_node_ids(&mut binding.baseline);
        }
    }

    pub(crate) fn renew_node_ids(&mut self) {
        self.map_node_ids(|_, _| Uuid::new_v4());
    }

    pub(crate) fn instance_node_ids(&mut self, root: Uuid) {
        self.map_node_ids(|_, uid| {
            derived(b"rovar/instance-node/v1", root.as_bytes(), uid.as_bytes())
        });
    }
}

/// Normalize saved sync baselines without requiring allocation watermarks,
/// which are intentionally omitted from their semantic representation.
pub(crate) fn upgrade_json_node_ids(value: &mut serde_json::Value) {
    let Some(page) = value.as_object_mut() else {
        return;
    };
    if !["boards", "shapes", "texts", "hierarchy"]
        .iter()
        .all(|key| page.contains_key(*key))
    {
        return;
    }
    let Some(namespace) = page.get("id").and_then(|v| v.as_str()).map(str::to_owned) else {
        return;
    };
    let upgrade = |node: &mut serde_json::Value, id: u64| {
        if let Some(node) = node.as_object_mut()
            && node
                .get("uid")
                .is_none_or(|uid| uid.as_str() == Some("00000000-0000-0000-0000-000000000000"))
        {
            node.insert(
                "uid".into(),
                serde_json::json!(derived(
                    b"rovar/legacy-node/v1",
                    namespace.as_bytes(),
                    &id.to_le_bytes()
                )),
            );
        }
    };
    for field in ["boards", "shapes", "texts"] {
        if let Some(nodes) = page.get_mut(field).and_then(|v| v.as_array_mut()) {
            for node in nodes {
                if let Some(id) = node.get("id").and_then(|v| v.as_u64()) {
                    upgrade(node, id);
                }
            }
        }
    }
    if let Some(groups) = page
        .get_mut("hierarchy")
        .and_then(|h| h.get_mut("groups"))
        .and_then(|v| v.as_object_mut())
    {
        for (id, node) in groups {
            if let Ok(id) = id.parse::<u64>() {
                upgrade(node, id);
            }
        }
    }
}
