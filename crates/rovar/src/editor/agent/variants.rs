use super::*;
use crate::{
    document::Page,
    scene::components::{
        self,
        variants::{self as native, ComponentSet},
    },
};

pub(super) fn describe(
    definitions: &components::Definitions,
    sets: &native::Sets,
    pages: &[Page],
) -> Value {
    let mut roots: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for page in pages {
        let ids = page.node_ids();
        for (id, binding) in &page.hierarchy.components {
            roots
                .entry(binding.component.clone())
                .or_default()
                .push(json!({"page_id":page.id,"id":ids[id],"master":binding.master}));
        }
    }
    json!({"components":definitions.iter().map(|(id,definition)| {
        let set=sets.iter().find(|(_,set)|set.variants.contains_key(id));
        json!({"id":id,"name":definition.name,"set_id":set.map(|(id,_)|id),
            "variant_name":set.map(|(_,set)|&set.variants[id]),"roots":roots.get(id).cloned().unwrap_or_default()})
    }).collect::<Vec<_>>(),"sets":sets.iter().map(|(id,set)|json!({"id":id,"name":set.name,
        "variants":set.variants.iter().map(|(component,name)|json!({"component":component,"name":name})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()})
}

pub(super) struct Edit<'a> {
    pub definitions: &'a mut components::Definitions,
    pub sets: &'a mut native::Sets,
    pub components: &'a mut BTreeMap<String, String>,
    pub set_refs: &'a mut BTreeMap<String, String>,
    pub pages: &'a [Page],
}

impl Edit<'_> {
    fn component(&self, id: &str) -> Result<String> {
        let id = self.components.get(id).map_or(id, String::as_str);
        if !self.definitions.contains_key(id) {
            return Err(Error::new(
                "COMPONENT_NOT_FOUND",
                "Unknown component definition",
            ));
        }
        Ok(id.into())
    }
    fn set(&self, id: &str) -> Result<String> {
        let id = self.set_refs.get(id).map_or(id, String::as_str);
        if !self.sets.contains_key(id) {
            return Err(Error::new(
                "VARIANT_SET_NOT_FOUND",
                "Unknown component variant set",
            ));
        }
        Ok(id.into())
    }
    fn member_set(&self, component: &str) -> Result<String> {
        self.sets
            .iter()
            .find(|(_, set)| set.variants.contains_key(component))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                Error::new(
                    "VARIANT_SET_NOT_FOUND",
                    "Component does not belong to a variant set",
                )
            })
    }
    fn editable(&self, page: &Page, component: &str) -> Result<()> {
        for page in
            std::iter::once(page).chain(self.pages.iter().filter(|other| other.id != page.id))
        {
            for (&root, binding) in &page.hierarchy.components {
                if binding.master && binding.component == component {
                    model::editable(page, root)?;
                }
            }
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        page: &mut Page,
        op: p::Operation,
        refs: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        components::synchronize(std::slice::from_mut(page), self.definitions)
            .map_err(|e| Error::invalid(e.to_string()))?;
        match op {
            p::Operation::CreateComponent {
                id,
                reference,
                name,
            } => {
                let id = model::lookup(page, &id, refs)?;
                let component = structure::create_component(page, self.definitions, id, name)?;
                self.components.insert(reference, component);
            }
            p::Operation::InstantiateComponent {
                component,
                reference,
                parent,
                position,
            } => {
                let component = self.component(&component)?;
                let parent = parent
                    .as_deref()
                    .map(|id| model::lookup(page, id, refs))
                    .transpose()?;
                let root = structure::instantiate(
                    page,
                    &self.definitions[&component],
                    component.clone(),
                    parent,
                    position,
                )?;
                refs.insert(reference, page.node_ids()[&root].to_string());
            }
            p::Operation::CreateVariantSet {
                reference,
                name,
                variants,
            } => {
                if variants.is_empty() || variants.len() > p::MAX_OPERATIONS {
                    return Err(Error::invalid("A variant set requires 1 to 128 members"));
                }
                let mut set = ComponentSet {
                    name,
                    variants: BTreeMap::new(),
                };
                for member in variants {
                    let component = self.component(&member.component)?;
                    self.editable(page, &component)?;
                    if set.variants.insert(component, member.name).is_some() {
                        return Err(Error::invalid("Duplicate variant component"));
                    }
                }
                let id = uuid::Uuid::new_v4().to_string();
                self.sets.insert(id.clone(), set);
                self.set_refs.insert(reference, id);
            }
            p::Operation::AddVariant {
                source,
                reference,
                component_ref,
                name,
                parent,
                position,
            } => {
                let source = self.component(&source)?;
                self.editable(page, &source)?;
                let set = self.member_set(&source)?;
                let mut definition = self.definitions[&source].clone();
                definition.source = None;
                let id = uuid::Uuid::new_v4().to_string();
                let parent = parent
                    .as_deref()
                    .map(|id| model::lookup(page, id, refs))
                    .transpose()?;
                let root = structure::instantiate(page, &definition, id.clone(), parent, position)?;
                page.hierarchy.components.get_mut(&root).unwrap().master = true;
                self.definitions.insert(id.clone(), definition);
                self.sets
                    .get_mut(&set)
                    .unwrap()
                    .variants
                    .insert(id.clone(), name);
                self.components.insert(component_ref, id);
                refs.insert(reference, page.node_ids()[&root].to_string());
            }
            p::Operation::RenameVariantSet { id, name } => {
                let id = self.set(&id)?;
                for component in self.sets[&id].variants.keys() {
                    self.editable(page, component)?;
                }
                self.sets.get_mut(&id).unwrap().name = name;
            }
            p::Operation::RenameVariant { component, name } => {
                let component = self.component(&component)?;
                self.editable(page, &component)?;
                let set = self.member_set(&component)?;
                self.sets
                    .get_mut(&set)
                    .unwrap()
                    .variants
                    .insert(component, name);
            }
            p::Operation::SwitchVariant { id, component } => {
                let root = model::lookup(page, &id, refs)?;
                model::editable(page, root)?;
                let component = self.component(&component)?;
                native::switch(page, root, &component, self.definitions, self.sets)
                    .map_err(|e| Error::invalid(e.to_string()))?;
            }
            p::Operation::ResetComponent { id } => {
                let root = model::lookup(page, &id, refs)?;
                model::editable(page, root)?;
                components::reset_instance(page, root, self.definitions)
                    .map_err(|e| Error::invalid(e.to_string()))?;
            }
            _ => return Err(Error::invalid("Expected a component operation")),
        }
        native::validate(self.sets, self.definitions).map_err(|e| Error::invalid(e.to_string()))
    }
}

pub(super) fn resolve_action_refs(op: &mut p::Operation, refs: &BTreeMap<String, String>) {
    if let p::Operation::SetInteraction {
        action, hover_exit, ..
    } = op
    {
        if let p::interaction::Action::ChangeVariant { target } = action
            && let Some(id) = refs.get(target)
        {
            *target = id.clone();
        }
        if let Some(p::interaction::HoverExit::ChangeVariant { target }) = hover_exit
            && let Some(id) = refs.get(target)
        {
            *target = id.clone();
        }
    }
}
