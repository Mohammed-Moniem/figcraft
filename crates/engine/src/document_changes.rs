//! Bounded invalidations for committed command, gesture, text, plugin and background edits.
//! This observes immutable document identities, not command names; reads and cancelled previews emit none.
use crate::Session;
use figcraft_doc::Document;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
#[derive(Default)]
pub(crate) struct Observed(pub(crate) HashMap<u64, (Arc<Document>, u64)>);
impl Session {
    pub fn observe_document_changes(&mut self) {
        if self.group_depth > 0 {
            return;
        }
        let mut updates = Vec::new();
        for ds in &mut self.docs {
            if ds.interaction.is_some() || ds.preview.is_some() {
                continue;
            }
            // Hover and text previews replace the displayed document temporarily. Observe the
            // authored file underneath so showing or cancelling a preview cannot look committed.
            let doc = ds.document_itself().clone();
            let previous = self.observed_documents.0.get(&ds.uid);
            let changed = previous.is_some_and(|(before, _)| !Arc::ptr_eq(before, &doc) && **before != *doc);
            if changed {
                let (before, revision) = previous.map(|(d, r)| (d.clone(), *r)).unwrap_or_else(|| (doc.clone(), ds.revision));
                if ds.revision == revision {
                    ds.revision = ds.revision.saturating_add(1)
                }
                let ids = doc.changed_ids(&before, 256);
                updates.push((doc.name.clone(),ds.path.clone(),json!({"document":ds.uid,"revision":ds.revision,"dirty":ds.is_dirty(),"changedIds":ids.as_ref().map(|ids|ids.iter().map(|id|id.0).collect::<Vec<_>>()).unwrap_or_default(),"reset":ids.is_none(),"scope":"committed-document"})));
            }
            self.observed_documents.0.insert(ds.uid, (doc, ds.revision));
        }
        self.observed_documents.0.retain(|uid, _| self.docs.iter().any(|ds| ds.uid == *uid));
        for (name, path, data) in updates {
            self.events.push("document.changed", &name, path.as_deref(), data);
        }
    }
}
