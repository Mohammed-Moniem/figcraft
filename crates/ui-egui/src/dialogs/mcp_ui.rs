//! Local MCP server & LLM harness configuration dialog (`File ▸ Local MCP server…` and
//! `Plugins ▸ Manage plugins… ▸ MCP`).

use egui::{Context, RichText};
use figcraft_engine::harness::{detect_cli_binary, inspect_all};
use serde_json::json;

use crate::FigApp;
use crate::i18n::{tr, trf};
use crate::theme::{Tokens, regular, semibold};

/// Default width of the standalone Local MCP Server window on desktop viewports.
pub const MCP_WINDOW_W: f32 = 440.0;

/// Draw the standalone Local MCP Server window when `app.ui.mcp_window` is open.
pub fn show(app: &mut FigApp, ctx: &Context) {
    if !app.ui.mcp_window {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let w = (ctx.content_rect().width() - 48.0).clamp(260.0, MCP_WINDOW_W);
    super::floating(tr("Local MCP Server"), ctx)
        .id(egui::Id::unique("Local MCP Server"))
        .default_pos(ctx.content_rect().center())
        .pivot(egui::Align2::CENTER_CENTER)
        .constrain(true)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(w);
            ui.set_max_width(w);
            body(app, ui, &t);
        });
    if !open {
        app.ui.mcp_window = false;
    }
}

/// Cached CLI binary path and harness inspection results for the open MCP dialog.
#[derive(Clone, Default)]
struct McpHarnessCache {
    port: u16,
    cli_bin: String,
    statuses: Vec<figcraft_engine::harness::HarnessStatus>,
}

/// Recompute and store the cached CLI binary path and harness statuses for `active_port`.
fn refresh_harness_cache(ui: &mut egui::Ui, cache_id: egui::Id, active_port: u16) -> McpHarnessCache {
    let cli_bin = detect_cli_binary();
    let statuses = inspect_all(&cli_bin, Some(active_port));
    let cache = McpHarnessCache { port: active_port, cli_bin, statuses };
    ui.data_mut(|d| d.insert_temp(cache_id, cache.clone()));
    cache
}

/// Draw the responsive Local MCP Server controls and LLM harness cards inside `ui`.
pub(crate) fn body(app: &mut FigApp, ui: &mut egui::Ui, t: &Tokens) {
    if cfg!(target_arch = "wasm32") {
        ui.heading("Local browser control");
        ui.label("Use the local host browser CLI or figcraft-web-mcp.mjs. List live sessions, then choose the exact session and document. The native TCP control server runs in the desktop app.");
        return;
    }
    let avail_w = ui.available_width();
    ui.set_max_width(avail_w);

    let enabled = app.session.prefs.mcp_enabled;
    let pref_port = app.session.prefs.mcp_port;
    let pending_id = egui::Id::unique("mcp_pending_port");
    let synced_id = egui::Id::unique("mcp_last_pref_port");
    let cache_id = egui::Id::unique("mcp_harness_cache");
    let mut port = ui.data_mut(|d| {
        let last_pref = *d.get_temp_mut_or(synced_id, pref_port);
        let p = d.get_temp_mut_or(pending_id, pref_port);
        if last_pref != pref_port {
            *p = pref_port;
            let cur = *p;
            d.insert_temp(synced_id, pref_port);
            cur
        } else {
            *p
        }
    });

    let toggle_checkbox = |ui: &mut egui::Ui, app: &mut FigApp| {
        if crate::widgets::checkbox(ui, enabled, tr("Enable local MCP control server")) {
            app.run("pref.mcp", json!({"enabled": !enabled}));
        }
    };

    let commit_port = |ui: &mut egui::Ui, app: &mut FigApp, new_port: u16| {
        ui.data_mut(|d| {
            d.insert_temp(pending_id, new_port);
            d.insert_temp(synced_id, new_port);
            d.remove::<McpHarnessCache>(cache_id);
        });
        app.run("pref.mcp", json!({"port": new_port}));
    };

    if avail_w >= 380.0 {
        ui.horizontal(|ui| {
            toggle_checkbox(ui, app);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_default = port == figcraft_engine::DEFAULT_MCP_PORT;
                if crate::widgets::button(ui, tr("Reset"), false, !is_default).on_hover_text(tr("Reset to default port 47800")).clicked() {
                    port = figcraft_engine::DEFAULT_MCP_PORT;
                    commit_port(ui, app, port);
                }
                let drag = ui.add(egui::DragValue::new(&mut port).range(1024..=65535));
                if drag.changed() {
                    ui.data_mut(|d| d.insert_temp(pending_id, port));
                }
                if (drag.drag_stopped() || drag.lost_focus()) && port != app.session.prefs.mcp_port {
                    commit_port(ui, app, port);
                }
                ui.label(RichText::new(tr("Port")).color(t.text_secondary).font(regular(11.0)));
            });
        });
    } else {
        toggle_checkbox(ui, app);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Port")).color(t.text_secondary).font(regular(11.0)));
            let drag = ui.add(egui::DragValue::new(&mut port).range(1024..=65535));
            if drag.changed() {
                ui.data_mut(|d| d.insert_temp(pending_id, port));
            }
            if (drag.drag_stopped() || drag.lost_focus()) && port != app.session.prefs.mcp_port {
                commit_port(ui, app, port);
            }
            let is_default = port == figcraft_engine::DEFAULT_MCP_PORT;
            if crate::widgets::button(ui, tr("Reset"), false, !is_default).on_hover_text(tr("Reset to default port 47800")).clicked() {
                port = figcraft_engine::DEFAULT_MCP_PORT;
                commit_port(ui, app, port);
            }
        });
    }

    ui.add_space(6.0);
    let server_status = app.services.mcp_server_status();
    if let Some((listening, active, ref err)) = server_status {
        if listening != 0 {
            let status_text = trf("Listening on 127.0.0.1:{port} · {count} active connection(s)", &[("port", &listening), ("count", &active)]);
            ui.add(egui::Label::new(RichText::new(status_text).color(t.accent).font(semibold(11.0))).wrap());
        } else if let Some(err) = err {
            ui.add(egui::Label::new(RichText::new(trf("Error: {error}", &[("error", &err)])).color(t.danger).font(regular(11.0))).wrap());
        } else {
            ui.add(egui::Label::new(RichText::new(tr("Server stopped")).color(t.text_secondary).font(regular(11.0))).wrap());
        }
    } else {
        ui.add(
            egui::Label::new(
                RichText::new(tr("Local MCP control server is available in the desktop app.")).color(t.text_secondary).font(regular(11.0)),
            )
            .wrap(),
        );
    }

    let active_port = server_status.as_ref().map(|(listening, _, _)| *listening).filter(|p| *p != 0).unwrap_or(app.session.prefs.mcp_port);
    let mut cache = ui
        .data_mut(|d| d.get_temp::<McpHarnessCache>(cache_id))
        .filter(|c| c.port == active_port && !c.cli_bin.is_empty())
        .unwrap_or_else(|| refresh_harness_cache(ui, cache_id, active_port));
    let token_file = app
        .services
        .mcp_control_token_file(active_port)
        .filter(|path| figcraft_engine::harness::validate_token_file(path).is_ok())
        .and_then(|p| p.canonicalize().ok());
    let can_connect = token_file.is_some();
    if let Some(path) = token_file.as_deref() {
        for h in &mut cache.statuses {
            if let Some(kind) = figcraft_engine::harness::HarnessKind::parse(&h.id) {
                h.snippet = figcraft_engine::harness::snippet_with_token_file(kind, &cache.cli_bin, active_port, path);
            }
        }
    }
    let attach_cmd = token_file.as_deref().map_or_else(
        || "Start the local control server to generate an authenticated connection.".to_owned(),
        |path| figcraft_engine::harness::attach_command_with_token_file(&cache.cli_bin, active_port, path),
    );

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("CLI bridge")).color(t.text_secondary).font(semibold(11.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::button(ui, tr("Copy Command"), false, can_connect).clicked() {
                ui.ctx().copy_text(attach_cmd.clone());
                app.toast(tr("Copied MCP command"));
            }
        });
    });
    ui.add_space(2.0);
    egui::Frame::NONE.fill(t.field).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        let inner_w = (avail_w - 16.0).max(120.0);
        ui.set_width(inner_w);
        ui.set_max_width(inner_w);
        ui.add(egui::Label::new(RichText::new(&attach_cmd).color(t.text_secondary).font(egui::FontId::monospace(10.5))).wrap());
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(RichText::new(tr("LLM Harnesses")).color(t.text).font(semibold(12.0)));
    ui.horizontal_wrapped(|ui| {
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::button(ui, "Install all clients", true, can_connect).clicked() {
                let mut installed = 0;
                let mut errors = Vec::new();
                for kind in figcraft_engine::harness::HarnessKind::ALL {
                    match app.services.install_mcp_harness(kind, &cache.cli_bin, active_port) {
                        Ok(_) => installed += 1,
                        Err(error) => errors.push(format!("{}: {error}", kind.label())),
                    }
                }
                app.toast(format!("Installed {installed} client configuration(s). {}", errors.join("; ")));
                cache = refresh_harness_cache(ui, cache_id, active_port);
            }
            if crate::widgets::button(ui, "Copy all configurations", false, can_connect).clicked() {
                if let Some(token) = token_file.as_deref() {
                    ui.ctx().copy_text(
                        figcraft_engine::harness::HarnessKind::ALL
                            .iter()
                            .map(|kind| {
                                format!(
                                    "{}\n{}",
                                    kind.label(),
                                    figcraft_engine::harness::snippet_with_token_file(*kind, &cache.cli_bin, active_port, token)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n"),
                    );
                }
                app.toast("Copied configuration snippets");
            }
            if crate::widgets::button(ui, tr("Refresh"), false, true).on_hover_text(tr("Re-check harness configuration files")).clicked() {
                cache = refresh_harness_cache(ui, cache_id, active_port);
            }
        });
    });
    ui.add(
        egui::Label::new(
            RichText::new(tr("Connect Antigravity, Claude Code, Claude Desktop or Codex directly to this live FigCraft window."))
                .color(t.text_secondary)
                .font(regular(11.0)),
        )
        .wrap(),
    );
    ui.add_space(8.0);

    let mut installed_any = false;
    let statuses = cache.statuses;
    let scroll_max_h = (ui.ctx().content_rect().height() - 260.0).clamp(180.0, 380.0);
    egui::ScrollArea::vertical().id_salt("mcp_harnesses_scroll").max_height(scroll_max_h).show(ui, |ui| {
        let card_inner_w = (avail_w - 20.0).max(160.0);
        for h in statuses {
            egui::Frame::NONE.fill(t.field).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(card_inner_w);
                ui.set_max_width(card_inner_w);
                let (badge, color) = if h.configured { (tr("Configured"), t.accent) } else { (tr("Not configured"), t.text_secondary) };
                let mut action_buttons = |ui: &mut egui::Ui, app: &mut FigApp| {
                    if crate::widgets::button(ui, tr("Install"), true, can_connect).clicked()
                        && let Some(kind) = figcraft_engine::harness::HarnessKind::parse(&h.id)
                    {
                        match app.services.install_mcp_harness(kind, &cache.cli_bin, active_port) {
                            Ok(status) => {
                                installed_any = true;
                                app.toast(format!("Installed {} configuration: {}", status.label, status.config_path.display()));
                            }
                            Err(error) => app.toast(format!("Client installation failed: {error}")),
                        }
                    }
                    if crate::widgets::button(ui, tr("Copy Config"), false, can_connect).clicked() {
                        if let (Some(kind), Some(token)) = (figcraft_engine::harness::HarnessKind::parse(&h.id), token_file.as_deref()) {
                            ui.ctx().copy_text(figcraft_engine::harness::snippet_with_token_file(kind, &cache.cli_bin, active_port, token));
                        }
                        app.toast(trf("Copied {name} config", &[("name", &h.label)]));
                    }
                };

                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&h.label).color(t.text).font(semibold(12.0)));
                    ui.label(RichText::new(badge).color(color).font(regular(11.0)));
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| action_buttons(ui, app));

                ui.add(egui::Label::new(RichText::new(h.config_path.display().to_string()).color(t.text_secondary).font(regular(10.5))).wrap());
            });
            ui.add_space(6.0);
        }
    });
    if installed_any {
        let _ = refresh_harness_cache(ui, cache_id, active_port);
    }
}
