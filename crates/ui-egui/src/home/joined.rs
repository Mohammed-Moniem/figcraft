//! Remembered joins share Home ▸ Online with owned files. Only presentation hides a duplicate
//! owner card: retaining the capability matters in the browser, which does not persist owners.

use egui::{CornerRadius, Rect, Sense, Ui, pos2, vec2};
use figcraft_engine::joined::{JoinedFile, same_endpoint};
use figcraft_engine::live::{Access, utc_date};
use serde_json::json;

use crate::a11y::{self, Tip};
use crate::home::{FileRow, file_row};
use crate::i18n::{tr, trf};
use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{FigApp, widgets};

pub(crate) fn visible(session: &figcraft_engine::Session) -> Vec<JoinedFile> {
    session
        .prefs
        .joined_files
        .iter()
        .filter(|joined| !session.prefs.online_files.iter().any(|owned| owned.file == joined.file && same_endpoint(&owned.server, &joined.address)))
        .cloned()
        .collect()
}

fn detail(file: &JoinedFile) -> String {
    let access = match file.access {
        Access::Edit => tr("Can edit"),
        Access::View => tr("View only"),
        Access::Comment => tr("Can comment"),
    };
    let date = utc_date(file.opened);
    trf("{server} · {access} · Opened {date}", &[("server", &file.address), ("access", &access), ("date", &date.get(..10).unwrap_or(&date))])
}

pub(crate) fn open(app: &mut FigApp, file: &JoinedFile) {
    let Some(result) = app.run("collab.openJoined", json!({"address":file.address, "file":file.file})) else { return };
    if result["alreadyOpen"] == true && !result["document"].is_null() {
        app.ui.home = false;
    } else {
        app.ui.live.home_join = result["session"].as_u64();
    }
}

pub fn section(app: &mut FigApp, ui: &mut Ui, tokens: &Tokens, files: &[JoinedFile]) {
    if files.is_empty() {
        return;
    }
    ui.add_space(12.0);
    widgets::label(ui, tr("Joined files"), tokens.text);
    widgets::label(ui, tr("Files you joined with a link or code. Forgetting one here does not remove it from its server."), tokens.text_secondary);
    for file in files {
        row(app, ui, tokens, file);
    }
}

fn row(app: &mut FigApp, ui: &mut Ui, tokens: &Tokens, file: &JoinedFile) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let more = Rect::from_center_size(pos2(rect.max.x - 18.0, rect.center().y), vec2(24.0, 24.0));
    let detail = detail(file);
    a11y::button(&response, &format!("{}, {detail}", file.name));
    let shown = FileRow {
        mark: Some((Icon::LiveShared, tokens.icon_secondary)),
        name: &file.name,
        ink: tokens.text,
        detail: &detail,
        detail_size: 11.0,
        detail_ink: tokens.text_secondary,
    };
    file_row(ui, tokens, rect.with_max_x(more.min.x - 4.0), &response, &shown);
    let menu = ui
        .interact(more, ui.scope_id().with(("joined_more", &file.address, &file.file)), Sense::click())
        .tip(trf("Actions for {name}", &[("name", &file.name)]));
    a11y::menu_button(&menu, &trf("Actions for {name}", &[("name", &file.name)]), a11y::menu_open(&menu));
    if menu.hovered() {
        ui.painter().rect_filled(more, CornerRadius::same(5), tokens.pressed);
    }
    icons::paint(ui.painter(), more.shrink(5.0), Icon::Dots, tokens.icon);
    crate::right::menu_popup(&menu).width(240.0).show(|ui| {
        if crate::right::menu_item(ui, tr("Open"), "") {
            open(app, file);
            ui.close();
        }
        let label = if cfg!(target_arch = "wasm32") { tr("Forget on this browser") } else { tr("Forget on this computer") };
        if crate::right::menu_item(ui, label, "") {
            app.run("collab.forgetJoined", json!({"address":file.address, "file":file.file}));
            ui.close();
        }
    });
    if response.clicked() {
        open(app, file);
    }
}
