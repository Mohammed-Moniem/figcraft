use std::collections::{BTreeMap, BTreeSet};

use super::{Exporter, MAX_DEPTH, NodeId, NodeKind};

pub(super) struct Frame {
    pub slot: NodeId,
    pub guid: u32,
    pub def: u32,
}

#[derive(Default)]
pub(super) struct Slots {
    pub frames: Vec<Frame>,
    pub by_instance: BTreeMap<NodeId, Vec<usize>>,
    pub private_layers: BTreeSet<NodeId>,
}

impl Exporter<'_> {
    /// The normal instance walk assigns no GUIDs below linked instances. An edited slot's
    /// content is independent, so write it as a separate tree under a private native frame.
    pub(super) fn assign_slots(&mut self, limit: usize) {
        let doc = self.doc;
        for (slot, instance) in std::mem::take(&mut self.library.slots) {
            let Some(n) = doc.node(slot) else { continue };
            let Some(NodeKind::Instance { component, .. }) = doc.node(instance).map(|i| &i.kind) else { continue };
            if self.ids.contains_key(&slot) || !self.library.mains.contains(component) || !self.ids.contains_key(component) {
                continue;
            }
            // A slot nested in an instance can point at that instance's expanded copy, which
            // in turn points at the actual slot property on the main.
            let mut source = n.source.and_then(|id| doc.node(id));
            let mut def = None;
            for _ in 0..=MAX_DEPTH {
                let Some(s) = source else { break };
                if !self.budget.reserve(16, 1) {
                    return;
                }
                if let Some(name) = s.prop_refs.get("slot") {
                    def = self.def_for(*component, name, "SLOT");
                    break;
                }
                source = s.source.and_then(|id| doc.node(id));
            }
            let Some(def) = def else {
                self.warn("slot content without a matching slot property was left out");
                continue;
            };
            if !self.budget.items(n.children.len(), 32, 1) || !self.budget.reserve(128, 3) {
                return;
            }
            let mut pending: Vec<_> = n.children.iter().rev().map(|id| (*id, 1usize)).collect();
            while let Some((id, depth)) = pending.pop() {
                let Some(child) = doc.node(id) else { continue };
                if depth > MAX_DEPTH || self.ids.contains_key(&id) || !self.slots.private_layers.insert(id) {
                    self.budget.fail("duplicate or over-depth private slot subtree");
                    return;
                }
                if self.ids.len() >= limit || !self.budget.reserve(96, 2) {
                    self.budget.fail("private slot layers exceed the export limit");
                    return;
                }
                let Ok(local) = u32::try_from(self.ids.len() + 1) else {
                    self.budget.fail("private slot layer identifier overflow");
                    return;
                };
                self.ids.insert(id, local);
                // A placed instance is itself private content even when its source is its main.
                // Its expanded children still come from that main on import.
                if matches!(&child.kind, NodeKind::Instance { component, .. } if self.library.mains.contains(component)) {
                    continue;
                }
                if !self.budget.items(child.children.len(), 32, 1) {
                    return;
                }
                pending.extend(child.children.iter().rev().map(|id| (*id, depth + 1)));
            }
            self.slots.by_instance.entry(instance).or_default().push(self.slots.frames.len());
            self.slots.frames.push(Frame { slot, def, guid: 0 });
        }
        // Real layer ids are contiguous. Synthetic wrappers follow them, before the asset page.
        let Some(last) = self.ids.len().checked_add(self.slots.frames.len()).filter(|&n| n <= limit && u32::try_from(n).is_ok()) else {
            self.budget.fail("private slot frames exceed the export limit");
            return;
        };
        let first = last - self.slots.frames.len();
        for (offset, frame) in self.slots.frames.iter_mut().enumerate() {
            frame.guid = (first + offset + 1) as u32;
        }
    }

    pub(super) fn asset_canvas(&self) -> Option<u32> {
        u32::try_from(self.ids.len().checked_add(self.slots.frames.len())?.checked_add(1)?).ok()
    }
}
