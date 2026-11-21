use crate::library::VariableMap;
use figcraft_doc::{Document, Node, NodeId, Variables};
use std::collections::{BTreeMap, BTreeSet};

/// Import the local definitions carried by a design clipboard. Imported definitions stay editable,
/// and matching definitions from repeated pastes can be reused.
pub(super) fn import(dest: &mut Document, source: &Document, layers: &[(NodeId, Vec<Node>)]) -> VariableMap {
    let nodes = layers.iter().flat_map(|(_, nodes)| nodes);
    let needed = crate::library::alias_closure(&source.variables, nodes.clone().flat_map(|n| n.bound.values().cloned()));
    let mut mode_sets: BTreeSet<_> = nodes.flat_map(|n| n.var_modes.keys().cloned()).collect();
    let vars = &source.variables;
    let definitions: Vec<_> = vars.variables.iter().filter(|v| needed.contains(&v.id)).collect();
    mode_sets.extend(definitions.iter().map(|v| v.collection.clone()));
    let collections: Vec<_> = vars.collections.iter().filter(|c| mode_sets.contains(&c.id) && !c.modes.is_empty()).cloned().collect();
    let known: BTreeSet<_> = collections.iter().map(|c| c.id.as_str()).collect();
    // If a referenced definition is absent its dependants cannot be imported faithfully either.
    // Detach their bindings while retaining the concrete fills, text, and geometry in the canvas.
    let mut valid: BTreeSet<_> =
        definitions.iter().filter(|v| known.contains(v.collection.as_str()) && !v.values.is_empty()).map(|v| v.id.as_str()).collect();
    let mut dependants: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut invalid = Vec::new();
    for v in &definitions {
        for dependency in v.values.values().flat_map(|value| value.references()) {
            dependants.entry(dependency.as_str()).or_default().push(&v.id);
            if !valid.contains(dependency.as_str()) {
                invalid.push(v.id.as_str());
            }
        }
    }
    while let Some(id) = invalid.pop() {
        if valid.remove(id)
            && let Some(children) = dependants.get(id)
        {
            invalid.extend(children.iter().copied());
        }
    }
    let variables: Vec<_> = definitions.into_iter().filter(|v| valid.contains(v.id.as_str())).cloned().collect();
    let portable = Variables { collections, variables };
    if portable.collections.is_empty() {
        return VariableMap::default();
    }

    let target_vars: BTreeMap<_, _> = dest.variables.variables.iter().map(|v| (v.id.as_str(), v)).collect();
    let target_sets: BTreeMap<_, _> = dest.variables.collections.iter().map(|c| (c.id.as_str(), c)).collect();
    // A GUID has meaning only within its source file. If any definition differs, move this set of
    // definitions together so aliases and modes still point within the copied set.
    let mut suffix = 0u32;
    let (mapping, mut incoming) = loop {
        let local = |id: &str| {
            if suffix == 0 { id.to_owned() } else { format!("paste{suffix}/{id}") }
        };
        let mapping = VariableMap {
            variables: portable.variables.iter().map(|v| (v.id.clone(), local(&v.id))).collect(),
            collections: portable.collections.iter().map(|c| (c.id.clone(), local(&c.id))).collect(),
        };
        let mut incoming = portable.clone();
        for c in &mut incoming.collections {
            c.id = local(&c.id);
        }
        for v in &mut incoming.variables {
            v.id = local(&v.id);
            v.collection = local(&v.collection);
            for reference in v.values.values_mut().flat_map(|value| value.references_mut()) {
                *reference = local(reference);
            }
        }
        let conflicts = incoming.collections.iter().any(|c| target_sets.get(c.id.as_str()).is_some_and(|old| *old != c))
            || incoming.variables.iter().any(|v| target_vars.get(v.id.as_str()).is_some_and(|old| *old != v));
        if !conflicts {
            break (mapping, incoming);
        }
        suffix += 1;
    };
    incoming.collections.retain(|c| !target_sets.contains_key(c.id.as_str()));
    incoming.variables.retain(|v| !target_vars.contains_key(v.id.as_str()));
    dest.variables.collections.extend(incoming.collections);
    dest.variables.variables.extend(incoming.variables);
    mapping
}

pub(super) fn remap(node: &mut Node, source: &Variables, mapping: &VariableMap) {
    node.bound.retain(|_, variable| {
        let Some(local) = mapping.variables.get(variable) else {
            return false;
        };
        variable.clone_from(local);
        true
    });
    node.var_modes = std::mem::take(&mut node.var_modes)
        .into_iter()
        .filter_map(|(id, mode)| {
            source.collection_by_id(&id)?.mode_by_id(&mode)?;
            Some((mapping.collections.get(&id)?.clone(), mode))
        })
        .collect();
}
