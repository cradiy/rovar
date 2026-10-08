//! Three-way property merging after aligning process-local handles by node UUID.
use super::*;
use serde_json::{Map, Value};

/// A conflict (including an invalid combined graph) leaves all inputs untouched.
pub(crate) fn merge(base: &[u8], local: &[u8], remote: &[u8]) -> Result<Vec<u8>> {
    let (local_json, remote_json) = (local, remote);
    let mut documents = [decode(base)?, decode(local)?, decode(remote)?];
    ensure!(
        documents.iter().all(|d| d.id == documents[0].id),
        "Different document identities"
    );
    let pages: BTreeSet<_> = documents
        .iter()
        .flat_map(|d| d.pages.iter().map(|p| p.id.clone()))
        .collect();
    for id in pages {
        // Preserve local handles, including its allocation watermark. A remote
        // insertion must never steal the handle of a selected/local object.
        let mut handles = BTreeMap::new();
        let mut next = documents
            .iter()
            .flat_map(|d| &d.pages)
            .filter(|p| p.id == id)
            .map(|p| p.next_id)
            .max()
            .unwrap_or(1);
        if let Some(page) = documents[1].pages.iter().find(|p| p.id == id) {
            handles.extend(page.node_ids().into_iter().map(|(id, uid)| (uid, id)));
        }
        let identities: BTreeSet<_> = documents
            .iter()
            .flat_map(|d| &d.pages)
            .filter(|p| p.id == id)
            .flat_map(|p| p.node_ids().into_values())
            .collect();
        for uid in identities {
            if let std::collections::btree_map::Entry::Vacant(entry) = handles.entry(uid) {
                entry.insert(next);
                next = next.checked_add(1).context("Node handle overflow")?;
            }
        }
        for page in documents
            .iter_mut()
            .flat_map(|d| &mut d.pages)
            .filter(|p| p.id == id)
        {
            align(page, &handles)?;
            page.next_id = next;
        }
    }
    let [base, local, remote] = documents.map(canonical);
    let (base, local, remote) = (base?, local?, remote?);
    if local == remote || local == base {
        return Ok(remote_json.to_vec());
    }
    if remote == base {
        return Ok(local_json.to_vec());
    }
    let merged =
        value(Some(&base), Some(&local), Some(&remote), "document")?.context("Deleted document")?;
    let json = materialize(merged)?;
    // Independent valid edits can combine into dangling parents, cycles or
    // invalid style references. Never install such a graph automatically.
    let document = Document::decode(&serde_json::to_vec(&json)?)?;
    Ok(serde_json::to_vec(&document)?)
}

fn decode(bytes: &[u8]) -> Result<Document> {
    let mut value: Value = serde_json::from_slice(bytes)?;
    restore_watermarks(&mut value)?;
    Document::decode(&serde_json::to_vec(&value)?)
}

fn restore_watermarks(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("boards")
                && fields.contains_key("shapes")
                && fields.contains_key("texts")
            {
                let mut maximum = 0;
                for field in ["boards", "shapes", "texts"] {
                    for node in fields[field].as_array().context("Invalid node list")? {
                        maximum = maximum.max(node["id"].as_u64().context("Invalid node handle")?);
                    }
                }
                if let Some(groups) = fields["hierarchy"]["groups"].as_object() {
                    for id in groups.keys() {
                        maximum = maximum.max(id.parse()?);
                    }
                }
                let next = maximum.checked_add(1).context("Node handle overflow")?;
                let next = next.max(fields.get("next_id").and_then(Value::as_u64).unwrap_or(1));
                fields.insert("next_id".into(), next.into());
            }
            for field in fields.values_mut() {
                restore_watermarks(field)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                restore_watermarks(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn align(page: &mut Page, handles: &BTreeMap<uuid::Uuid, usize>) -> Result<()> {
    let mapping: BTreeMap<_, _> = page
        .node_ids()
        .into_iter()
        .map(|(id, uid)| (id, handles[&uid]))
        .collect();
    let id = |old: usize| {
        mapping
            .get(&old)
            .copied()
            .context("Unresolved node reference")
    };
    // Materialize the implicit tail before handles change, preserving stacking.
    let ordered: BTreeSet<_> = page.hierarchy.order.iter().copied().collect();
    for old in mapping.keys() {
        if !ordered.contains(old) {
            page.hierarchy.order.push(*old);
        }
    }
    for node in &mut page.boards {
        node.id = id(node.id)?;
    }
    for node in &mut page.shapes {
        node.id = id(node.id)?;
        node.board = node.board.map(id).transpose()?;
    }
    for node in &mut page.texts {
        node.id = id(node.id)?;
        node.board = node.board.map(id).transpose()?;
    }
    for group in page.hierarchy.groups.values_mut() {
        group.board = group.board.map(id).transpose()?;
        group.mask = group.mask.map(id).transpose()?;
    }
    for parent in page.hierarchy.parents.values_mut() {
        *parent = id(*parent)?;
    }
    for binding in page.hierarchy.components.values_mut() {
        // Historical mappings may include removed nodes. Until these can be
        // aligned by template identity, keep that case in manual resolution.
        for handle in binding.nodes.values_mut() {
            *handle = id(*handle)?;
        }
    }
    let h = &mut page.hierarchy;
    h.start = h.start.map(id).transpose()?;
    for action in h.interactions.values_mut() {
        *action = action
            .remap(|target| mapping.get(&target).copied())
            .context("Unresolved navigation target")?;
    }
    remap(&mut h.interactions, &mapping)?;
    remap(&mut h.groups, &mapping)?;
    remap(&mut h.parents, &mapping)?;
    remap(&mut h.names, &mapping)?;
    remap(&mut h.layouts, &mapping)?;
    remap(&mut h.sizing, &mapping)?;
    remap(&mut h.exports, &mapping)?;
    remap(&mut h.effects, &mapping)?;
    remap(&mut h.components, &mapping)?;
    for handle in &mut h.order {
        *handle = id(*handle)?;
    }
    for asset in &mut page.assets {
        asset.object = id(asset.object)?;
    }
    Ok(())
}

fn remap<T>(items: &mut BTreeMap<usize, T>, mapping: &BTreeMap<usize, usize>) -> Result<()> {
    *items = std::mem::take(items)
        .into_iter()
        .map(|(id, item)| {
            Ok((
                *mapping.get(&id).context("Unresolved node reference")?,
                item,
            ))
        })
        .collect::<Result<_>>()?;
    Ok(())
}

fn canonical(document: Document) -> Result<Value> {
    let mut value = serde_json::to_value(document)?;
    let pages = value["pages"]
        .take()
        .as_array()
        .context("Invalid pages")?
        .clone();
    value["page_order"] = Value::Array(pages.iter().map(|p| p["id"].clone()).collect());
    let mut keyed = Map::new();
    for mut page in pages {
        for field in ["boards", "shapes", "texts"] {
            let mut nodes = Map::new();
            for mut node in page[field]
                .take()
                .as_array()
                .context("Invalid nodes")?
                .clone()
            {
                if field == "texts" {
                    // Text and its style runs form one edit, not independently
                    // mergeable properties with unrelated character offsets.
                    node["text"] =
                        Value::Array(vec![node["content"].take(), node["styles"].take()]);
                    node.as_object_mut().unwrap().remove("content");
                    node.as_object_mut().unwrap().remove("styles");
                }
                nodes.insert(node["id"].to_string(), node);
            }
            page[field] = nodes.into();
        }
        let mut assets = Map::new();
        for asset in page["assets"].take().as_array().context("Invalid assets")? {
            assets.insert(
                format!("{}:{}", asset["object"], asset["fill"]),
                asset.clone(),
            );
        }
        page["assets"] = assets.into();
        for field in [
            "exports",
            "layouts",
            "sizing",
            "components",
            "effects",
            "interactions",
        ] {
            if page["hierarchy"].get(field).is_none() {
                page["hierarchy"][field] = Value::Object(Map::new());
            }
        }
        // Bindings and definitions describe a template graph. Merge them as
        // units so overrides cannot be attached to a different template state.
        for binding in page["hierarchy"]["components"]
            .as_object_mut()
            .unwrap()
            .values_mut()
        {
            *binding = Value::Array(vec![binding.take()]);
        }
        keyed.insert(page["id"].as_str().context("Invalid page ID")?.into(), page);
    }
    value["pages"] = keyed.into();
    value["components"] = Value::Array(vec![
        value
            .get("components")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
    ]);
    if value.get("colors").is_none() {
        value["colors"] = Value::Object(Map::new());
    }
    if value.get("component_sets").is_none() {
        value["component_sets"] = Value::Object(Map::new());
    }
    Ok(value)
}

fn materialize(mut value: Value) -> Result<Value> {
    let mut pages = value["pages"].take();
    let mut ordered = Vec::new();
    for id in value["page_order"]
        .take()
        .as_array()
        .context("Invalid page order")?
    {
        let mut page = pages
            .as_object_mut()
            .unwrap()
            .remove(id.as_str().context("Invalid page ID")?)
            .context("Missing ordered page")?;
        for field in ["boards", "shapes", "texts"] {
            let mut nodes: Vec<_> = page[field]
                .take()
                .as_object()
                .context("Invalid nodes")?
                .values()
                .cloned()
                .collect();
            nodes.sort_by_key(|node| node["id"].as_u64());
            if field == "texts" {
                for node in &mut nodes {
                    let text = node
                        .as_object_mut()
                        .unwrap()
                        .remove("text")
                        .context("Missing text")?;
                    node["content"] = text[0].clone();
                    node["styles"] = text[1].clone();
                }
            }
            page[field] = nodes.into();
        }
        page["assets"] = page["assets"]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .into();
        for binding in page["hierarchy"]["components"]
            .as_object_mut()
            .unwrap()
            .values_mut()
        {
            *binding = binding[0].take();
        }
        ordered.push(page);
    }
    ensure!(pages.as_object().unwrap().is_empty(), "Unordered pages");
    value["pages"] = ordered.into();
    value.as_object_mut().unwrap().remove("page_order");
    value["components"] = value["components"][0].take();
    restore_watermarks(&mut value)?;
    Ok(value)
}

fn value(
    base: Option<&Value>,
    local: Option<&Value>,
    remote: Option<&Value>,
    path: &str,
) -> Result<Option<Value>> {
    if local == remote || remote == base {
        return Ok(local.cloned());
    }
    if local == base {
        return Ok(remote.cloned());
    }
    if let (Some(Value::Object(base)), Some(Value::Object(local)), Some(Value::Object(remote))) =
        (base, local, remote)
    {
        let keys: BTreeSet<_> = base
            .keys()
            .chain(local.keys())
            .chain(remote.keys())
            .collect();
        let mut result = Map::new();
        for key in keys {
            if let Some(merged) = value(
                base.get(key),
                local.get(key),
                remote.get(key),
                &format!("{path}/{key}"),
            )? {
                result.insert(key.clone(), merged);
            }
        }
        return Ok(Some(result.into()));
    }
    if (path.ends_with("/order") || path == "document/page_order")
        && let (Some(Value::Array(base)), Some(Value::Array(local)), Some(Value::Array(remote))) =
            (base, local, remote)
    {
        return Ok(Some(order(base, local, remote)?.into()));
    }
    anyhow::bail!("Concurrent edits at {path}")
}

/// Combine independent insertions/deletions using both branches' ordering
/// constraints. Incompatible reorders form a cycle and remain a conflict.
fn order(base: &[Value], local: &[Value], remote: &[Value]) -> Result<Vec<Value>> {
    let base_ids: BTreeSet<_> = base.iter().map(Value::to_string).collect();
    let local_ids: BTreeSet<_> = local.iter().map(Value::to_string).collect();
    let remote_ids: BTreeSet<_> = remote.iter().map(Value::to_string).collect();
    let retained: Vec<_> = local
        .iter()
        .chain(remote)
        .filter(|id| {
            let id = id.to_string();
            !base_ids.contains(&id) || (local_ids.contains(&id) && remote_ids.contains(&id))
        })
        .cloned()
        .collect();
    let mut nodes: BTreeMap<_, _> = retained.into_iter().map(|v| (v.to_string(), v)).collect();
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for branch in [local, remote] {
        let branch: Vec<_> = branch
            .iter()
            .map(Value::to_string)
            .filter(|id| nodes.contains_key(id))
            .collect();
        edges.extend(
            branch
                .windows(2)
                .map(|pair| (pair[0].clone(), pair[1].clone())),
        );
    }
    let mut outgoing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut degrees: BTreeMap<_, usize> = nodes.keys().map(|id| (id.clone(), 0)).collect();
    for (from, to) in edges {
        *degrees.get_mut(&to).unwrap() += 1;
        outgoing.entry(from).or_default().push(to);
    }
    let mut ready: BTreeSet<_> = degrees
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut result = Vec::new();
    while let Some(next) = ready.pop_first() {
        result.push(nodes.remove(&next).unwrap());
        for to in outgoing.remove(&next).unwrap_or_default() {
            let degree = degrees.get_mut(&to).unwrap();
            *degree -= 1;
            if *degree == 0 {
                ready.insert(to);
            }
        }
    }
    ensure!(nodes.is_empty(), "Conflicting layer order");
    Ok(result)
}

#[cfg(test)]
mod tests;
