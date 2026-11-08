use super::load::file_stem;
use super::{DocState, EngineError, Result, Session, Value, json, load_path};
/// An external replacement that could not be loaded. A new stamp or a successful refresh clears
/// it; the desktop schedules `retry_at` even when the file's stamp stays unchanged.
#[derive(Clone, Debug)]
pub struct RefreshFailure {
    /// The failed destination; a changed Save as path starts a separate failure.
    pub path: String,
    /// The replacement that failed, distinct from the document's saved baseline.
    pub stamp: crate::file_access::Stamp,
    /// What to report once for this file and replacement, while logging each failed attempt.
    pub message: String,
    /// Monotonic deadline for the next automatic attempt; explicit refresh bypasses it.
    pub retry_at: web_time::Instant,
}

/// External saves are a new saved baseline, never an edit or the end of a preview. Prepare all
/// replacements before touching tabs, preserve local work, and bound automatic failure retries.
pub(super) fn refresh(s: &mut Session, p: &Value) -> Result<Value> {
    let automatic = p.get("automatic").and_then(Value::as_bool) == Some(true);
    let now = web_time::Instant::now();
    let retry_at = now.checked_add(std::time::Duration::from_secs(30)).unwrap_or(now);
    let mut refreshed = Vec::new();
    let mut errors = Vec::new();
    let mut replacements = Vec::new();
    let mut failures = Vec::new();
    for i in 0..s.docs.len() {
        let Some(ds) = s.docs.get(i) else { continue };
        let (Some(path), Some(was)) = (&ds.path, &ds.on_disk) else {
            failures.push((i, None));
            continue;
        };
        let Ok(stamp) = crate::library::stat(path) else {
            failures.push((i, None));
            continue;
        };
        if stamp == *was {
            failures.push((i, None));
            continue;
        }
        let previous = ds.refresh_failure.as_ref().filter(|f| f.path == *path && f.stamp == stamp);
        if ds.branch_of.is_some()
            || ds.interaction.is_some()
            || ds.text_edit.is_some()
            || ds.text_preview.is_some()
            || ds.preview.is_some()
            || s.live_of(ds.uid).doc_uid.is_some()
            || s.docs.iter().any(|b| b.branch_of.as_ref().is_some_and(|(uid, _)| *uid == ds.uid))
            // Undo/normalisation can copy the saved document without changing its contents.
            || (ds.is_dirty() && (ds.versions_changed || ds.doc != ds.saved_doc))
        {
            // Keep the warning's identity while busy, but never leave an expired deadline that
            // would make the desktop spin. Finishing the edit also triggers another check.
            let held = previous.cloned().map(|mut failure| {
                if failure.retry_at <= now {
                    failure.retry_at = retry_at;
                }
                failure
            });
            failures.push((i, held));
            continue;
        }
        if automatic && previous.is_some_and(|f| f.retry_at > now) {
            continue;
        }
        let loaded = match load_path(path, None).and_then(|(loaded, rest)| {
            if !rest && loaded.issues.is_empty() && loaded.images_past_budget == 0 {
                Ok(loaded)
            } else {
                // An incomplete or damaged replacement must never become the saved baseline.
                Err(EngineError::Other(format!("{} couldn't be refreshed: the replacement file is incomplete or damaged", file_stem(path))))
            }
        }) {
            Ok(loaded) => loaded,
            Err(e) => {
                let message = e.to_string();
                errors.push(message.clone());
                failures.push((i, Some(RefreshFailure { path: path.clone(), stamp, message, retry_at })));
                continue;
            }
        };
        // A writer still replacing this file: retry on the next notification.
        if crate::library::stat(path).ok().as_ref() != Some(&stamp) {
            failures.push((i, None));
            continue;
        }
        let mut replacement = DocState::from_native(loaded.doc, Some(path.clone())).with_versions(loaded.versions).with_branches(loaded.branches);
        replacement.uid = ds.uid;
        replacement.revision = ds.revision.wrapping_add(1);
        replacement.view = ds.view.clone();
        replacement.view_from_prefs = ds.view_from_prefs;
        if replacement.doc.pages.contains(&ds.page) {
            replacement.page = ds.page;
        }
        replacement.selection = ds.selection.clone();
        replacement.clean_selection();
        replacement.on_disk = Some(stamp);
        refreshed.push(ds.uid);
        replacements.push((i, replacement));
    }
    // Parsing and settlement of every replacement happen before changing any tab, so the panic
    // guard can leave the session untouched even when a later file fails unexpectedly.
    for (i, failure) in failures {
        if let Some(slot) = s.docs.get_mut(i) {
            slot.refresh_failure = failure;
        }
    }
    for (i, replacement) in replacements {
        if let Some(slot) = s.docs.get_mut(i) {
            *slot = replacement;
        }
    }
    Ok(json!({"refreshed": refreshed, "errors": errors}))
}
