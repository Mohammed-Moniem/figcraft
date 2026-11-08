//! Optional UI accent preference; never writes document paints.
use crate::{
    FigApp, a11y,
    i18n::{tr, tr_noop},
    widgets,
};
use egui::{Context, Id, TextEdit, Ui};
use figcraft_engine::Prefs;
use serde_json::json;

#[derive(Clone)]
struct Draft {
    source: String,
    text: String,
    invalid: bool,
}

fn draft_id(ctx: &Context) -> Id {
    Id::unique(("pref-accent-draft", ctx.viewport_id()))
}

/// A hidden Preferences window owns no unfinished edit. Keep other viewports independent.
pub(super) fn cancel(ctx: &Context) {
    let id = draft_id(ctx);
    ctx.data_mut(|d| d.remove::<Draft>(id));
    ctx.memory_mut(|m| m.surrender_focus(id.with("hex")));
}

fn choose(app: &mut FigApp, color: &str) {
    app.run("pref.accentColor", json!({"color":color}));
}

pub(super) fn controls(app: &mut FigApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(tr("Accent color"));
        for (label, color) in
            [(tr_noop("Green"), "#02DA96"), (tr_noop("Blue"), "#0D99FF"), (tr_noop("Purple"), "#9747FF"), (tr_noop("Orange"), "#E67E22")]
        {
            if ui.selectable_label(app.session.prefs.accent == color, tr(label)).clicked() {
                choose(app, color);
            }
        }
    });
    custom(app, ui);
}

fn custom(app: &mut FigApp, ui: &mut Ui) {
    let state_id = draft_id(ui.ctx());
    let field_id = state_id.with("hex");
    let mut draft =
        ui.data(|d| d.get_temp::<Draft>(state_id)).unwrap_or_else(|| Draft { source: String::new(), text: String::new(), invalid: false });
    ui.horizontal(|ui| {
        ui.label(tr("Custom accent hex"));
        let mut rgb = app.session.prefs.accent_rgb();
        let picker = ui.color_edit_button_srgb(&mut rgb).on_hover_text(tr("Choose a custom accent color"));
        a11y::color(&picker, tr("Choose a custom accent color"), &app.session.prefs.accent);
        if picker.changed() {
            choose(app, &format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        }
        let source = &app.session.prefs.accent;
        if draft.source != *source {
            draft = Draft { source: source.clone(), text: source.clone(), invalid: false };
            // A changed preference replaces the draft instead of letting an old edit later
            // overwrite it. The text editor's own cursor may stay, but its keyboard owner ends.
            ui.memory_mut(|m| m.surrender_focus(field_id));
        }
        let response = ui.add(TextEdit::singleline(&mut draft.text).id(field_id).desired_width(90.0).char_limit(32));
        a11y::text(&response, tr("Custom accent hex"), &draft.text);
        // Dialog keys handle Escape before drawing. Hidden-window cleanup discards that edit.
        if response.lost_focus() && draft.text != draft.source {
            if Prefs::parse_accent(&draft.text).is_some() {
                choose(app, &draft.text);
                draft.source = app.session.prefs.accent.clone();
                draft.text = draft.source.clone();
                draft.invalid = false;
            } else {
                draft.invalid = true;
            }
        }
        let reset = widgets::button(ui, tr("Reset"), false, app.session.prefs.accent != "#02DA96").on_hover_text(tr("Reset accent color"));
        a11y::button(&reset, tr("Reset accent color"));
        if reset.clicked() {
            app.run("pref.accentColor", json!({"reset":true}));
        }
    });
    if draft.invalid {
        let t = crate::theme::Tokens::get(ui.ctx());
        widgets::label(ui, tr("Use six hexadecimal digits, for example #0D99FF."), t.text_secondary);
    }
    ui.data_mut(|d| d.insert_temp(state_id, draft));
}
