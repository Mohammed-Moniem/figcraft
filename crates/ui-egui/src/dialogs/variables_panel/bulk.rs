//! Table selection and shared publishing/scope controls. No per-variable text/value drafts.
use std::collections::BTreeSet;

use egui::{Align2, Context, CornerRadius, Id, Pos2, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{Variable, Variables, scopes_for};
use serde_json::{Value, json};

use super::{Editor, View};
use crate::FigApp;
use crate::i18n::tr;
use crate::theme::{Tokens, regular};
use crate::{a11y, icons, widgets};

#[derive(Clone, Default)]
pub(super) struct Selection {
    ids: BTreeSet<String>,
    anchor: Option<String>,
    order: Vec<String>,
    scope: Option<(Option<String>, String)>,
    editor: Option<SharedEditor>,
}

#[derive(Clone)]
struct SharedEditor {
    owner: Option<u64>,
    collection: String,
    ids: Vec<String>,
    scopes: bool,
    focus: bool,
}

impl Selection {
    pub(super) fn multiple(&self) -> bool {
        self.ids.len() > 1
    }

    pub(super) fn open(&self) -> bool {
        self.editor.is_some()
    }

    pub(super) fn dismiss(&mut self) -> bool {
        self.editor.take().is_some()
    }

    pub(super) fn clear(&mut self, primary: &mut Option<String>) {
        self.ids.clear();
        self.anchor = None;
        *primary = None;
    }

    pub(super) fn single(&mut self, id: &str, primary: &mut Option<String>) {
        self.ids = BTreeSet::from([id.to_owned()]);
        self.anchor = Some(id.to_owned());
        *primary = Some(id.to_owned());
    }

    pub(super) fn contains(&self, id: &str, primary: Option<&str>) -> bool {
        self.ids.contains(id) || primary == Some(id)
    }

    pub(super) fn sync(&mut self, view: &View<'_>, collection: &str, list: &[&Variable], primary: &mut Option<String>) {
        let scope = (view.id().map(str::to_owned), collection.to_owned());
        if self.scope.as_ref().is_some_and(|s| *s != scope) {
            self.clear(primary);
            self.editor = None;
        }
        self.scope = Some(scope);
        self.order = list.iter().map(|v| v.id.clone()).collect();
        let visible: BTreeSet<&str> = self.order.iter().map(String::as_str).collect();
        self.ids.retain(|id| visible.contains(id.as_str()));
        if primary.as_deref().is_some_and(|id| !visible.contains(id)) {
            *primary = None;
        }
        if self.ids.is_empty()
            && let Some(id) = primary.as_ref()
        {
            self.ids.insert(id.clone());
        }
        if self.anchor.as_deref().is_some_and(|id| !visible.contains(id)) {
            self.anchor = None;
        }
    }

    pub(super) fn click(&mut self, id: &str, primary: &mut Option<String>, modifiers: egui::Modifiers, secondary: bool) {
        if secondary {
            if !self.ids.contains(id) {
                self.single(id, primary);
            }
        } else if modifiers.shift {
            let range = self
                .anchor
                .as_deref()
                .and_then(|anchor| Some((self.order.iter().position(|x| x == anchor)?, self.order.iter().position(|x| x == id)?)));
            if let Some((a, b)) = range {
                if !modifiers.command {
                    self.ids.clear();
                }
                self.ids.extend(self.order[a.min(b)..=a.max(b)].iter().cloned());
                *primary = Some(id.to_owned());
            } else {
                self.single(id, primary);
            }
        } else if modifiers.command {
            if !self.ids.remove(id) {
                self.ids.insert(id.to_owned());
            }
            self.anchor = Some(id.to_owned());
            *primary = if self.ids.contains(id) { Some(id.to_owned()) } else { self.ids.first().cloned() };
        } else {
            self.single(id, primary);
        }
    }

    pub(super) fn can_edit(&self, vars: &Variables, collection: &str) -> bool {
        self.multiple() && self.ids.iter().all(|id| vars.get(id).is_some_and(|v| v.extends.is_none() && v.collection == collection))
    }

    pub(super) fn begin(&mut self, vars: &Variables, owner: Option<u64>, collection: &str) {
        if self.can_edit(vars, collection) {
            self.editor =
                Some(SharedEditor { owner, collection: collection.to_owned(), ids: self.ids.iter().cloned().collect(), scopes: false, focus: true });
        }
    }
}

/// False, true, and mixed. A mixed click always turns the property on for every target.
fn state(vars: &[&Variable], on: impl Fn(&Variable) -> bool) -> Option<bool> {
    let first = vars.first().map(|v| on(v))?;
    vars.iter().all(|v| on(v) == first).then_some(first)
}

fn checkbox(ui: &mut Ui, on: Option<bool>, label: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    a11y::checkbox(&response, label, on == Some(true));
    if on.is_none() {
        ui.ctx().accesskit_node_builder(response.id, |node| node.set_toggled(egui::accesskit::Toggled::Mixed));
    }
    let rtl = widgets::rtl(ui);
    let b = widgets::flip_rect(rtl, r, egui::Rect::from_min_size(pos2(r.left(), r.center().y - 7.0), vec2(14.0, 14.0)));
    if on != Some(false) {
        ui.painter().rect_filled(b, CornerRadius::same(3), t.accent);
        if on.is_none() {
            ui.painter().line_segment([b.left_center() + vec2(3.0, 0.0), b.right_center() - vec2(3.0, 0.0)], Stroke::new(1.5, t.accent_on));
        } else {
            icons::paint(ui.painter(), b.shrink(1.5), icons::Icon::Check, t.accent_on);
        }
    } else {
        ui.painter().rect_stroke(b, CornerRadius::same(3), Stroke::new(1.0, t.border_strong), StrokeKind::Inside);
    }
    let (x, align) = if rtl { (b.left() - 8.0, Align2::RIGHT_CENTER) } else { (b.right() + 8.0, Align2::LEFT_CENTER) };
    ui.painter().text(Pos2::new(x, r.center().y), align, label, regular(11.0), t.text);
    response.clicked()
}

fn update(app: &mut FigApp, vars: &[&Variable], field: &str, values: impl Fn(&Variable) -> Option<Value>) {
    let commands: Vec<Value> = vars
        .iter()
        .filter_map(|v| {
            let value = values(v)?;
            Some(json!({"command":"variable.update", "params":{"variable":v.id, field:value}}))
        })
        .collect();
    if !commands.is_empty() {
        app.run("command.batch", json!({"label":tr("Edit variables"), "commands":commands}));
    }
}

fn scope_controls(app: &mut FigApp, ui: &mut Ui, vars: &[&Variable]) {
    let common: Vec<_> = vars
        .first()
        .into_iter()
        .flat_map(|v| scopes_for(v.kind))
        .filter(|(id, _)| vars.iter().all(|v| scopes_for(v.kind).iter().any(|(other, _)| other == id)))
        .collect();
    if common.is_empty() {
        widgets::label(ui, tr("No shared scope properties"), Tokens::get(ui.ctx()).text_secondary);
        return;
    }
    let all = state(vars, |v| scopes_for(v.kind).iter().all(|(id, _)| v.in_scope(id)));
    if checkbox(ui, all, tr("All supported properties")) {
        let on = all != Some(true);
        update(app, vars, "scopes", |v| {
            (scopes_for(v.kind).iter().any(|(id, _)| v.in_scope(id) != on)).then(|| if on { Value::Null } else { json!([]) })
        });
    }
    for (id, label) in common {
        let checked = state(vars, |v| v.in_scope(id));
        ui.push_id(id, |ui| {
            if checkbox(ui, checked, tr(label)) {
                let on = checked != Some(true);
                update(app, vars, "scopes", |v| {
                    (v.in_scope(id) != on).then(|| {
                        json!(
                            scopes_for(v.kind)
                                .iter()
                                .filter_map(|(scope, _)| { (if scope == id { on } else { v.in_scope(scope) }).then_some(*scope) })
                                .collect::<Vec<_>>()
                        )
                    })
                });
            }
        });
    }
}

pub(super) fn show(app: &mut FigApp, ctx: &Context, vars: &Variables, st: &mut Editor) {
    let Some(mut edit) = st.bulk.editor.take() else { return };
    // A captured ID is actionable only in its original local collection and document. In
    // particular, another document can legitimately contain the same variable IDs.
    if edit.owner != st.owner || st.library.is_some() || st.collection.as_deref() != Some(edit.collection.as_str()) {
        return;
    }
    let targets: Option<Vec<_>> = edit.ids.iter().map(|id| vars.get(id).filter(|v| v.extends.is_none() && v.collection == edit.collection)).collect();
    let Some(targets) = targets.filter(|v| v.len() > 1) else { return };
    let mut open = true;
    egui::Window::new(tr("Edit variables"))
        .id(Id::unique(("variables_bulk_details", st.owner, st.draft_epoch, &edit.ids)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(300.0)
        .max_height((ctx.content_rect().height() - 32.0).max(80.0))
        .vscroll(true)
        .frame(egui::Frame::window(&ctx.global_style()).fill(Tokens::get(ctx).panel).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.set_width(280.0);
            ui.horizontal(|ui| {
                let details = widgets::button(ui, tr("Details"), !edit.scopes, true);
                // Enter the new editor at a harmless tab. Do not leave keyboard users walking
                // every canvas/table control, or activate a mutating checkbox with the opener key.
                if edit.focus {
                    details.request_focus();
                    edit.focus = false;
                }
                if details.clicked() {
                    edit.scopes = false;
                }
                if widgets::button(ui, tr("Scope"), edit.scopes, true).clicked() {
                    edit.scopes = true;
                }
            });
            ui.add_space(10.0);
            if edit.scopes {
                scope_controls(app, ui, &targets);
            } else {
                let hidden = state(&targets, |v| v.hidden_from_publishing);
                if checkbox(ui, hidden, tr("Hide from publishing")) {
                    let on = hidden != Some(true);
                    update(app, &targets, "hiddenFromPublishing", |v| (v.hidden_from_publishing != on).then_some(json!(on)));
                }
            }
        });
    if open {
        st.bulk.editor = Some(edit);
    }
}
