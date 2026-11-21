//! The component definitions needed to reconstruct the visible instances. A document's
//! library store is a separate tree and can also contain assets unused by this design.

use std::collections::BTreeSet;

use super::budget::Budget;
use super::{Document, MAX_DEPTH, MAX_NODES, NodeId, NodeKind, PropDef};

#[derive(Default)]
pub(super) struct Library {
    pub nodes: BTreeSet<NodeId>,
    pub mains: BTreeSet<NodeId>,
    pub roots: Vec<NodeId>,
    /// Edited slots in the trees reached from the exported pages, paired with their enclosing instance.
    pub slots: Vec<(NodeId, NodeId)>,
}

impl Library {
    pub fn used_by(doc: &Document, budget: &mut Budget) -> Self {
        let mut library = Self::default();
        let store = doc.library_store();
        let mut seen = BTreeSet::new();
        let mut requested = BTreeSet::new();
        let mut variant_sets = BTreeSet::new();
        if !budget.items(doc.pages.len(), 32, 1) {
            return library;
        }
        let mut pending: Vec<_> = doc.pages.iter().map(|id| (*id, false, None)).collect();
        while let Some((id, hidden, owner)) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(n) = doc.node(id) else { continue };
            if seen.len() > MAX_NODES || !budget.items(n.children.len().saturating_add(1), 32, 2) {
                budget.fail("component reference inventory exceeds the export limit");
                break;
            }
            if hidden {
                library.nodes.insert(id);
            }
            if let Some(inst) = owner
                && n.overrides.contains("slot")
                && n.source.is_some()
            {
                if !budget.reserve(32, 1) {
                    break;
                }
                library.slots.push((id, inst));
            }
            let inside = if matches!(n.kind, NodeKind::Instance { .. }) { Some(id) } else { owner };
            pending.extend(n.children.iter().map(|id| (*id, hidden, inside)));
            let references = match &n.kind {
                NodeKind::Component { props, .. } => props.len(),
                NodeKind::Instance { props, .. } => props.len().saturating_add(n.bound.len()).saturating_add(1),
                _ => 0,
            };
            if !budget.items(references, 64, 2) {
                break;
            }
            let mut targets = Vec::new();
            match &n.kind {
                NodeKind::Instance { component, props } => {
                    targets.push(*component);
                    if n.bound.keys().any(|field| field.starts_with(crate::rest::PROP_BINDING))
                        && let Some(set) = doc.node(*component).and_then(|main| main.parent).and_then(|id| doc.node(id))
                        && matches!(set.kind, NodeKind::ComponentSet { .. })
                        && variant_sets.insert(set.id)
                    {
                        if !budget.items(set.children.len(), 64, 2) {
                            break;
                        }
                        targets.extend(&set.children);
                    }
                    if let Some(NodeKind::Component { props: definitions, .. }) = doc.node(*component).map(|n| &n.kind) {
                        for (name, value) in props {
                            if matches!(definitions.get(name), Some(PropDef::InstanceSwap { .. }))
                                && let Some(target) = value.as_u64()
                            {
                                targets.push(NodeId(target));
                            }
                        }
                    }
                }
                NodeKind::Component { props, .. } => {
                    if owner.is_none() {
                        library.mains.insert(id);
                    }
                    targets.extend(props.values().filter_map(|prop| match prop {
                        PropDef::InstanceSwap { default, .. } => Some(*default),
                        _ => None,
                    }));
                }
                _ => {}
            }
            let Some(store) = store else { continue };
            for target in targets {
                if !requested.insert(target) || !doc.node(target).is_some_and(|n| matches!(n.kind, NodeKind::Component { .. })) {
                    continue;
                }
                let mut branch = Vec::new();
                let mut ancestor = Some(target);
                while let Some(id) = ancestor {
                    if id == store {
                        library.nodes.extend(branch);
                        pending.push((target, true, None));
                        break;
                    }
                    if branch.len() > MAX_DEPTH || !budget.reserve(32, 1) {
                        budget.fail("component reference depth exceeds the export limit");
                        return library;
                    }
                    let Some(parent) = doc.node(id) else { break };
                    if matches!(parent.kind, NodeKind::Instance { .. }) {
                        break;
                    }
                    branch.push(id);
                    ancestor = parent.parent;
                }
            }
        }
        if let Some(root) = store.and_then(|s| doc.node(s)) {
            if !budget.items(root.children.len(), 16, 1) {
                return library;
            }
            library.roots.extend(root.children.iter().filter(|id| library.nodes.contains(id)));
        }
        library
    }
}
