use super::*;

pub(crate) type Sets = BTreeMap<String, ComponentSet>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ComponentSet {
    pub name: String,
    pub variants: BTreeMap<String, String>,
}

pub(crate) fn validate(sets: &Sets, definitions: &Definitions) -> anyhow::Result<()> {
    let mut members = BTreeSet::new();
    let valid_name = |name: &str| !name.trim().is_empty() && name.chars().count() <= 200;
    for (id, set) in sets {
        anyhow::ensure!(
            uuid::Uuid::parse_str(id).is_ok() && valid_name(&set.name),
            "Invalid component set"
        );
        anyhow::ensure!(!set.variants.is_empty(), "Empty component set");
        let mut names = BTreeSet::new();
        let mut root_kind = None;
        for (id, name) in &set.variants {
            let definition = definitions
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("Missing variant definition"))?;
            anyhow::ensure!(members.insert(id), "Component belongs to multiple sets");
            let kind = node_kind(&definition.page, definition.root);
            anyhow::ensure!(
                root_kind.is_none_or(|previous| previous == kind),
                "Variant roots must have the same type"
            );
            root_kind = Some(kind);
            anyhow::ensure!(valid_name(name), "Invalid variant name");
            anyhow::ensure!(names.insert(name), "Duplicate variant name");
        }
    }
    Ok(())
}

pub(crate) fn node_kind(page: &Page, id: usize) -> u8 {
    if page.boards.iter().any(|n| n.id == id) {
        0
    } else if page.shapes.iter().any(|n| n.id == id) {
        1
    } else if page.texts.iter().any(|n| n.id == id) {
        2
    } else {
        3
    }
}

// A semantic path pairs equally named siblings by occurrence and node type.
fn paths(page: &Page, root: usize) -> BTreeMap<Vec<(u8, String, usize)>, usize> {
    fn visit(
        page: &Page,
        id: usize,
        path: Vec<(u8, String, usize)>,
        out: &mut BTreeMap<Vec<(u8, String, usize)>, usize>,
    ) {
        out.insert(path.clone(), id);
        let mut children: Vec<_> = ids(page)
            .into_iter()
            .filter(|child| parent(page, *child) == Some(id))
            .collect();
        children.sort_by_key(|id| {
            (
                page.hierarchy
                    .order
                    .iter()
                    .position(|n| n == id)
                    .unwrap_or(usize::MAX),
                *id,
            )
        });
        let mut occurrences = BTreeMap::new();
        for child in children {
            let name = page
                .hierarchy
                .names
                .get(&child)
                .cloned()
                .or_else(|| {
                    page.boards
                        .iter()
                        .find(|n| n.id == child)
                        .map(|n| n.name.clone())
                })
                .or_else(|| {
                    page.shapes
                        .iter()
                        .find(|n| n.id == child)
                        .map(|n| n.name.clone())
                })
                .or_else(|| page.hierarchy.groups.get(&child).map(|n| n.name.clone()))
                .unwrap_or_default();
            let kind = node_kind(page, child);
            let count = occurrences.entry((kind, name.clone())).or_insert(0);
            let mut next = path.clone();
            next.push((kind, name, *count));
            *count += 1;
            visit(page, child, next, out);
        }
    }
    let mut result = BTreeMap::new();
    visit(page, root, Vec::new(), &mut result);
    result
}

pub(crate) fn switch(
    page: &mut Page,
    root: usize,
    target: &str,
    definitions: &Definitions,
    sets: &Sets,
) -> anyhow::Result<()> {
    let link = page
        .hierarchy
        .components
        .get(&root)
        .ok_or_else(|| anyhow::anyhow!("Missing instance"))?;
    anyhow::ensure!(!link.master, "Main components cannot switch variants");
    anyhow::ensure!(
        sets.values()
            .any(|set| set.variants.contains_key(&link.component)
                && set.variants.contains_key(target)),
        "Variants must belong to the same set"
    );
    if link.component == target {
        return Ok(());
    }
    let definition = &definitions[target];
    let old: Page = serde_json::from_value(link.baseline.clone())?;
    let old_root = *link
        .nodes
        .iter()
        .find(|(_, id)| **id == root)
        .ok_or_else(|| anyhow::anyhow!("Missing root mapping"))?
        .0;
    let next_paths = paths(&definition.page, definition.root);
    let old_uids = old.node_ids();
    let next_uids: BTreeMap<_, _> = definition
        .page
        .node_ids()
        .into_iter()
        .map(|(id, uid)| (uid, id))
        .collect();
    let mut matched = BTreeSet::new();
    let mut fresh = ids(&definition.page).last().copied().unwrap_or(0) + 1;
    let mut mapping = BTreeMap::new();
    for (path, id) in paths(&old, old_root) {
        let candidate = if id == old_root {
            Some(definition.root)
        } else {
            next_uids
                .get(&old_uids[&id])
                .or_else(|| next_paths.get(&path))
                .copied()
        };
        let target_id = candidate
            .filter(|target| {
                node_kind(&old, id) == node_kind(&definition.page, *target)
                    && matched.insert(*target)
            })
            .unwrap_or_else(|| {
                let id = fresh;
                fresh += 1;
                id
            });
        mapping.insert(id, target_id);
    }
    let nodes = mapping
        .iter()
        .map(|(old, new)| (*new, link.nodes[old]))
        .collect();
    let baseline = place(&old, &mapping, [0., 0.], None);
    let binding = page.hierarchy.components.get_mut(&root).unwrap();
    binding.component = target.into();
    binding.nodes = nodes;
    binding.baseline = serde_json::to_value(baseline)?;
    // Synchronize only the changed instance. Other masters are already current.
    let mut only = BTreeMap::new();
    only.insert(target.into(), definition.clone());
    let masters: Vec<_> = page
        .hierarchy
        .components
        .iter()
        .filter(|(_, b)| b.master)
        .map(|(id, b)| (*id, b.clone()))
        .collect();
    for (id, _) in &masters {
        page.hierarchy.components.remove(id);
    }
    let result = synchronize(std::slice::from_mut(page), &mut only);
    page.hierarchy.components.extend(masters);
    result?;
    page.validate()
}
