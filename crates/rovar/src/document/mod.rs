use crate::{
    scene::artboard::{Artboard, Rect},
    scene::layer::{Hierarchy, LayerState},
    scene::shape::Shape,
    scene::text::styles::StyledText,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) mod export;
pub(crate) mod library;
mod preview;
pub(crate) use preview::render as render_preview;
#[cfg(test)]
mod tests;

pub(crate) mod identity;
pub(crate) mod merge;
mod storage;
pub(crate) use storage::{cache_preview, load, read_id, save, save_as};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Text {
    #[serde(default, skip_serializing_if = "uuid::Uuid::is_nil")]
    pub uid: uuid::Uuid,
    pub id: usize,
    pub board: Option<usize>,
    pub rect: Rect,
    pub layer: LayerState,
    pub content: String,
    pub styles: StyledText,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AssetUse {
    pub object: usize,
    pub fill: bool,
    pub hash: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Page {
    pub id: String,
    pub name: String,
    pub boards: Vec<Artboard>,
    pub shapes: Vec<Shape>,
    pub texts: Vec<Text>,
    pub hierarchy: Hierarchy,
    pub next_id: usize,
    pub assets: Vec<AssetUse>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Document {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: crate::scene::color_styles::Palette,
    pub id: String,
    pub pages: Vec<Page>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: crate::scene::components::Definitions,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub component_sets: crate::scene::components::variants::Sets,
}

impl Document {
    pub fn single(page: Page) -> Self {
        Self {
            colors: Default::default(),
            id: uuid::Uuid::new_v4().to_string(),
            pages: vec![page],
            components: Default::default(),
            component_sets: Default::default(),
        }
    }
    pub fn decode(json: &[u8]) -> Result<Self> {
        let mut document: Self = serde_json::from_slice(json)?;
        document.upgrade_node_ids();
        document.validate()?;
        Ok(document)
    }
    pub fn validate(&self) -> Result<()> {
        crate::scene::components::variants::validate(&self.component_sets, &self.components)?;
        for (id, style) in &self.colors {
            ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid color style ID");
            style.validate()?;
        }
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid document ID"
        );
        ensure!(
            !self.pages.is_empty(),
            "A document must contain at least one page"
        );
        let mut ids = BTreeSet::new();
        for page in &self.pages {
            ensure!(ids.insert(&page.id), "Duplicate page ID");
            page.validate()?;
        }
        for (id, component) in &self.components {
            ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid component ID");
            ensure!(
                !component.name.trim().is_empty() && component.name.chars().count() <= 200,
                "Invalid component name"
            );
            component.page.validate()?;
            ensure!(
                component.page.hierarchy.components.is_empty(),
                "Nested component definitions are not supported"
            );
            ensure!(
                crate::scene::components::ids(&component.page).contains(&component.root)
                    && crate::scene::components::subtree(&component.page, component.root)
                        == crate::scene::components::ids(&component.page),
                "Invalid component root"
            );
        }
        let mut masters = BTreeSet::new();
        for page in &self.pages {
            for (root, binding) in &page.hierarchy.components {
                let definition = self
                    .components
                    .get(&binding.component)
                    .ok_or_else(|| anyhow::anyhow!("Missing component definition"))?;
                ensure!(
                    !binding.master || masters.insert(&binding.component),
                    "Duplicate main component"
                );
                ensure!(
                    binding.nodes.get(&definition.root) == Some(root),
                    "Invalid component root mapping"
                );
                let mut parent = crate::scene::components::parent(page, *root);
                while let Some(id) = parent {
                    ensure!(
                        !page.hierarchy.components.contains_key(&id),
                        "Nested components are not supported"
                    );
                    parent = crate::scene::components::parent(page, id);
                }
            }
        }
        Ok(())
    }
    pub fn first_page(&self) -> &Page {
        self.pages.first().expect("validated nonempty document")
    }
    pub fn assets(&self) -> impl Iterator<Item = &AssetUse> {
        self.pages
            .iter()
            .chain(self.components.values().map(|c| &c.page))
            .flat_map(|page| &page.assets)
    }
}

#[derive(Clone)]
pub(crate) struct AssetSource {
    pub hash: String,
    pub name: String,
    pub path: Arc<crate::media::Source>,
    pub size: [u32; 2],
}

pub(crate) struct Loaded {
    pub json: Vec<u8>,
    pub needs_upgrade: bool,
    pub assets: BTreeMap<String, Arc<crate::media::MediaAsset>>,
}

impl Loaded {
    pub fn into_document(self) -> Result<Document> {
        let mut document = Document::decode(&self.json)?;
        for page in document
            .pages
            .iter_mut()
            .chain(document.components.values_mut().map(|c| &mut c.page))
        {
            for use_ in &page.assets {
                let asset = self
                    .assets
                    .get(&use_.hash)
                    .ok_or_else(|| anyhow::anyhow!("Missing asset"))?
                    .clone();
                if use_.fill {
                    if let Some(board) = page.boards.iter_mut().find(|b| b.id == use_.object) {
                        board.image_fill.asset = Some(asset);
                    } else if let Some(shape) = page.shapes.iter_mut().find(|s| s.id == use_.object)
                    {
                        shape.image_fill.asset = Some(asset);
                    }
                } else if let Some(shape) = page.shapes.iter_mut().find(|s| s.id == use_.object) {
                    shape.media = Some(asset);
                }
            }
        }
        Ok(document)
    }
}

pub(crate) fn import(
    source: &Path,
    directory: &Path,
    text_system: &Arc<gpui::TextSystem>,
) -> Result<(Loaded, PathBuf)> {
    let mut loaded = load(source)?;
    let mut document = Document::decode(&loaded.json)?;
    document.id = uuid::Uuid::new_v4().to_string();
    loaded.json = serde_json::to_vec(&document)?;
    let assets: Vec<_> = loaded
        .assets
        .values()
        .map(|asset| AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [asset.width, asset.height],
        })
        .collect();
    rovar_storage::fs::create_dir_all(directory)?;
    let path = directory.join(format!("{}.rovar", document.id));
    save_as(&path, &loaded.json, &assets, text_system)?;
    Ok((load(&path)?, path))
}

impl Page {
    pub fn empty(name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            boards: Vec::new(),
            shapes: Vec::new(),
            texts: Vec::new(),
            hierarchy: Default::default(),
            next_id: 1,
            assets: Vec::new(),
        }
    }
    pub fn decode(json: &[u8]) -> Result<Self> {
        let mut document: Self = serde_json::from_slice(json)?;
        document.upgrade_node_ids();
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid document ID"
        );
        let mut identities = BTreeSet::new();
        for uid in self.node_ids().into_values() {
            ensure!(
                !uid.is_nil() && identities.insert(uid),
                "Duplicate or missing node identity"
            );
        }
        ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 200,
            "Invalid page name"
        );
        let mut ids = BTreeSet::new();
        for id in self
            .boards
            .iter()
            .map(|b| b.id)
            .chain(self.shapes.iter().map(|s| s.id))
            .chain(self.texts.iter().map(|t| t.id))
            .chain(self.hierarchy.groups.keys().copied())
        {
            ensure!(id > 0 && ids.insert(id), "Duplicate or invalid object ID");
        }
        ensure!(
            ids.last().is_none_or(|id| self.next_id > *id),
            "Invalid next object ID"
        );
        ensure!(
            self.hierarchy
                .names
                .keys()
                .chain(self.hierarchy.order.iter())
                .all(|id| ids.contains(id)),
            "Invalid layer reference"
        );
        crate::scene::auto_layout::validate(self, &ids)?;
        for (id, shadows) in &self.hierarchy.shadows {
            ensure!(
                ids.contains(id) && !self.hierarchy.groups.contains_key(id),
                "Invalid shadow object"
            );
            ensure!(
                shadows.len() <= crate::scene::effects::MAX_SHADOWS,
                "Too many shadows"
            );
            for shadow in shadows {
                shadow.validate()?;
            }
        }
        for (id, presets) in &self.hierarchy.exports {
            ensure!(ids.contains(id), "Invalid export object");
            for preset in presets {
                ensure!((1..=4).contains(&preset.scale), "Invalid export scale");
                ensure!(
                    preset.suffix.chars().count() <= 100
                        && !preset
                            .suffix
                            .chars()
                            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)),
                    "Invalid export suffix"
                );
            }
        }
        let boards: BTreeSet<_> = self.boards.iter().map(|b| b.id).collect();
        for board in self
            .shapes
            .iter()
            .map(|s| s.board)
            .chain(self.texts.iter().map(|t| t.board))
            .chain(self.hierarchy.groups.values().map(|g| g.board))
            .flatten()
        {
            ensure!(boards.contains(&board), "Missing frame");
        }
        for (child, parent) in &self.hierarchy.parents {
            ensure!(
                ids.contains(child) && self.hierarchy.groups.contains_key(parent),
                "Invalid group membership"
            );
            let mut visited = BTreeSet::from([*child]);
            let mut next = Some(parent);
            while let Some(parent) = next {
                ensure!(visited.insert(*parent), "Cyclic group membership");
                next = self.hierarchy.parents.get(parent);
            }
        }
        let mut parents: BTreeMap<_, _> = self
            .shapes
            .iter()
            .map(|s| (s.id, s.board))
            .chain(self.texts.iter().map(|t| (t.id, t.board)))
            .chain(self.hierarchy.groups.iter().map(|(id, g)| (*id, g.board)))
            .collect();
        parents.extend(
            self.hierarchy
                .parents
                .iter()
                .map(|(id, parent)| (*id, Some(*parent))),
        );
        for id in &ids {
            let mut seen = BTreeSet::new();
            let mut next = Some(*id);
            while let Some(id) = next {
                ensure!(seen.insert(id), "Cyclic document hierarchy");
                next = parents.get(&id).copied().flatten();
            }
        }
        for rect in self
            .boards
            .iter()
            .map(|b| b.rect)
            .chain(self.shapes.iter().map(|s| s.rect))
            .chain(self.texts.iter().map(|t| t.rect))
        {
            ensure!(
                [rect.x, rect.y, rect.width, rect.height]
                    .iter()
                    .all(|v| v.is_finite())
                    && rect.width >= 0.
                    && rect.height >= 0.,
                "Invalid object geometry"
            );
        }
        for text in &self.texts {
            let mut end = 0;
            for run in &text.styles.runs {
                ensure!(
                    run.range.start == end
                        && run.range.end >= end
                        && text.content.is_char_boundary(run.range.start)
                        && text.content.is_char_boundary(run.range.end),
                    "Invalid text style range"
                );
                end = run.range.end;
            }
            ensure!(end == text.content.len(), "Incomplete text styles");
        }
        for gradient in self
            .boards
            .iter()
            .map(|b| &b.gradient)
            .chain(
                self.shapes
                    .iter()
                    .flat_map(|s| [&s.gradient, &s.stroke.gradient]),
            )
            .chain(self.texts.iter().flat_map(|t| {
                std::iter::once(&t.styles.default.gradient)
                    .chain(t.styles.runs.iter().map(|r| &r.style.gradient))
            }))
        {
            gradient.validate()?;
        }
        let mut slots = BTreeSet::new();
        for placement in self.boards.iter().map(|b| &b.image_fill.placement).chain(
            self.shapes
                .iter()
                .flat_map(|s| [&s.image_fill.placement, &s.media_placement]),
        ) {
            placement.validate()?;
        }
        for asset in &self.assets {
            let shape = self.shapes.iter().find(|s| s.id == asset.object);
            ensure!(
                slots.insert((asset.object, asset.fill))
                    && (if asset.fill {
                        boards.contains(&asset.object) || shape.is_some()
                    } else {
                        shape.is_some_and(|s| s.kind.is_media())
                    })
                    && asset.hash.len() == 64
                    && asset.hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid asset reference"
            );
        }
        for shape in &self.shapes {
            ensure!(
                !shape.kind.is_media() || slots.contains(&(shape.id, false)),
                "Missing media asset"
            );
            ensure!(
                [
                    shape.layer.rotation,
                    shape.radius,
                    shape.inner_radius,
                    shape.stroke.width,
                    shape.image_fill.opacity
                ]
                .iter()
                .all(|v| v.is_finite()),
                "Invalid shape properties"
            );
            ensure!(
                shape
                    .points
                    .0
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite())
                    && shape
                        .nodes
                        .0
                        .iter()
                        .all(|n| [n.anchor, n.incoming, n.outgoing]
                            .iter()
                            .all(|p| p.x.is_finite() && p.y.is_finite())),
                "Invalid path coordinates"
            );
        }
        for (root, binding) in &self.hierarchy.components {
            ensure!(ids.contains(root), "Missing component instance");
            ensure!(
                uuid::Uuid::parse_str(&binding.component).is_ok(),
                "Invalid component reference"
            );
            let baseline: Page = serde_json::from_value(binding.baseline.clone())?;
            ensure!(
                baseline.hierarchy.components.is_empty(),
                "Recursive component baseline"
            );
            baseline.validate()?;
            ensure!(
                crate::scene::components::ids(&baseline)
                    .iter()
                    .all(|id| binding.nodes.contains_key(id)),
                "Incomplete component mapping"
            );
            let mapped: BTreeSet<_> = binding.nodes.values().copied().collect();
            ensure!(
                mapped.len() == binding.nodes.len()
                    && mapped.iter().all(|id| *id > 0 && *id < self.next_id),
                "Invalid component mapping"
            );
        }
        Ok(())
    }
}

pub(crate) mod rgba {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(color: &gpui::Rgba, s: S) -> Result<S::Ok, S::Error> {
        [color.r, color.g, color.b, color.a].serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gpui::Rgba, D::Error> {
        let [r, g, b, a] = <[f32; 4]>::deserialize(d)?;
        if [r, g, b, a]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(serde::de::Error::custom("Invalid color"));
        }
        Ok(gpui::Rgba { r, g, b, a })
    }
}

pub(crate) mod gradient_kind {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(kind: &gpui::GradientKind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match kind {
            gpui::GradientKind::Linear => "linear",
            gpui::GradientKind::Radial => "radial",
            gpui::GradientKind::Angular => "angular",
            gpui::GradientKind::Diamond => "diamond",
        })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gpui::GradientKind, D::Error> {
        match String::deserialize(d)?.as_str() {
            "linear" => Ok(gpui::GradientKind::Linear),
            "radial" => Ok(gpui::GradientKind::Radial),
            "angular" => Ok(gpui::GradientKind::Angular),
            "diamond" => Ok(gpui::GradientKind::Diamond),
            _ => Err(serde::de::Error::custom("Invalid gradient kind")),
        }
    }
}
