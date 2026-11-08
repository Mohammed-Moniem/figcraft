use super::{Document, FigApp, Id, Lead, NodeId, Ui, json, row, tr, widgets};

#[derive(Clone)]
struct PhaseField {
    owner: Option<Id>,
    sources: Vec<Option<f64>>,
    frame: u64,
    epoch: u64,
    focused: bool,
}

impl PhaseField {
    fn id(&self) -> Option<Id> {
        self.owner.map(|owner| owner.with(self.epoch))
    }
}

fn values(app: &FigApp, ids: &[NodeId]) -> Vec<Option<f64>> {
    app.session
        .doc()
        .map_or_else(|_| Vec::new(), |doc| ids.iter().map(|id| doc.node(*id).and_then(|n| n.stroke.dash_phase).filter(|p| p.is_finite())).collect())
}

fn retire(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|d| {
        d.remove::<String>(id.with("editing"));
        d.remove::<String>(id.with("kept"));
        d.remove::<bool>(id.with("selectall"));
        d.remove::<egui::text_edit::TextEditState>(id.with("te"));
    });
    ctx.memory_mut(|m| m.surrender_focus(id.with("te")));
}

fn field_id(app: &FigApp, ui: &Ui, ids: &[NodeId], active: bool) -> Option<Id> {
    let ctx = ui.ctx();
    let viewport = ctx.viewport_id();
    let frame = ctx.cumulative_frame_nr();
    let (focused, lost_focus) = ctx.input(|i| (i.focused, !i.focused || i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)))));
    let uid = app.session.ds().map_or(0, |ds| ds.uid);
    let owner = active.then(|| Id::unique(("stroke_phase", viewport, uid, ids)));
    let sources = if active { values(app, ids) } else { Vec::new() };
    let key = Id::unique(("stroke_phase_owner", viewport));
    let (id, old) = ctx.data_mut(|d| {
        let previous = d.get_temp::<PhaseField>(key);
        let changed = previous.as_ref().is_some_and(|p| {
            p.owner != owner
                || p.sources != sources
                || (p.frame != frame && p.frame.checked_add(1) != Some(frame))
                || (p.frame != frame && p.focused && lost_focus)
        });
        let epoch = previous.as_ref().map_or(0, |p| p.epoch).saturating_add(u64::from(changed));
        let old = changed.then(|| previous.as_ref().and_then(PhaseField::id)).flatten();
        let next = PhaseField { owner, sources, frame, epoch, focused };
        let id = next.id();
        d.insert_temp(key, next);
        (id, old)
    });
    if let Some(old) = old {
        retire(ctx, old);
    }
    id
}

fn adopt(app: &FigApp, ui: &Ui, ids: &[NodeId], id: Id) {
    let sources = values(app, ids);
    let key = Id::unique(("stroke_phase_owner", ui.ctx().viewport_id()));
    ui.ctx().data_mut(|d| {
        if let Some(mut state) = d.get_temp::<PhaseField>(key)
            && state.id() == Some(id)
        {
            state.sources = sources;
            d.insert_temp(key, state);
        }
    });
}

pub(super) fn settings(app: &mut FigApp, ui: &mut Ui, doc: &Document, ids: &[NodeId]) {
    let dashed = !ids.is_empty() && ids.iter().all(|id| doc.node(*id).is_some_and(|n| !n.stroke.dashes.is_empty()));
    if !dashed {
        field_id(app, ui, ids, false);
        return;
    }
    let before = values(app, ids);
    let manual = !before.is_empty() && before.iter().all(Option::is_some);
    row(ui, 32.0, |ui| {
        if widgets::checkbox(ui, manual, tr("Manual dash phase")) {
            let targets: Vec<NodeId> = ids.iter().copied().zip(&before).filter_map(|(id, phase)| (manual || phase.is_none()).then_some(id)).collect();
            app.set_props(&targets, json!({"dashPhase": if manual { None } else { Some(0.0) }}));
        }
    });
    let phase_values = values(app, ids);
    let active = !phase_values.is_empty() && phase_values.iter().all(Option::is_some);
    let Some(id) = field_id(app, ui, ids, active) else { return };
    let value = phase_values.first().copied().flatten().filter(|p| phase_values.iter().all(|v| *v == Some(*p)));
    widgets::sublabels(ui, tr("Phase"), None);
    row(ui, 32.0, |ui| {
        let edit = widgets::number_field_mixed_exact(ui, id, Lead::Named(tr("Phase")), value, "px", ui.available_width(), 1.0);
        app.field_of(ids, "dashPhase", edit, |n| n.stroke.dash_phase.unwrap_or(0.0), |v| json!(v));
    });
    // A field's accepted edits keep its identity for arrow/wheel/scrub continuation. A later
    // external source change still differs from this adopted snapshot and retires its draft.
    adopt(app, ui, ids, id);
}

#[cfg(test)]
#[path = "stroke_phase_tests.rs"]
mod tests;
