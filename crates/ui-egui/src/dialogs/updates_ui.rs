//! Software updates, background check at startup, and startup prompt modal.

use crate::FigApp;

#[cfg(target_arch = "wasm32")]
use crate::i18n::tr;
#[cfg(not(target_arch = "wasm32"))]
use craft_update::updater::Updater;
#[cfg(not(target_arch = "wasm32"))]
const UPSTREAM_APP: craft_update::app::App =
    craft_update::app::App { name: "FigCraft upstream", developer: "upstream contributors", icon_png: None, ..crate::UPDATE_APP };
#[cfg(not(target_arch = "wasm32"))]
const FORK_NOTICE: &str = "Official upstream release information only. This local build includes additional repairs. Replace it manually with a verified local kit; automatic download, installation and replacement are disabled.";
#[cfg(not(target_arch = "wasm32"))]
use craft_update::{CheckResultDialog, CheckResultResponse};

pub(crate) struct Runtime {
    #[cfg(not(target_arch = "wasm32"))]
    updater: Updater,
    #[cfg(not(target_arch = "wasm32"))]
    release: std::sync::Arc<std::sync::Mutex<Option<craft_update::release::Release>>>,
    /// The desktop app turns the check at start on; tests and the web build leave it off.
    #[cfg(not(target_arch = "wasm32"))]
    startup_check: bool,
    #[cfg(not(target_arch = "wasm32"))]
    check_requested: bool,
    /// The context of the last frame, to wake the UI when a check is queued from a menu callback.
    #[cfg(not(target_arch = "wasm32"))]
    repaint: Option<egui::Context>,
    /// The result dialog of a manual check is open.
    pub(crate) manual_window_open: bool,
    /// The web build has no updater; its only result is that checks are unsupported.
    #[cfg(target_arch = "wasm32")]
    pub(crate) status_message: Option<String>,
}

impl Default for Runtime {
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let release = std::sync::Arc::new(std::sync::Mutex::new(None));
        #[cfg(not(target_arch = "wasm32"))]
        let updater = {
            let result = release.clone();
            Updater::new(&UPSTREAM_APP, env!("CARGO_PKG_VERSION")).with_source(std::sync::Arc::new(move || {
                let answer = craft_update::Http::new("FigCraftLocal/UpstreamInfo")
                    .and_then(|http| craft_update::check_for_update(&UPSTREAM_APP, &http, env!("CARGO_PKG_VERSION")));
                if let Ok(found) = &answer {
                    *result.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = found.clone();
                }
                answer
            }))
        };
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            updater,
            #[cfg(not(target_arch = "wasm32"))]
            release,
            #[cfg(not(target_arch = "wasm32"))]
            startup_check: false,
            #[cfg(not(target_arch = "wasm32"))]
            check_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            repaint: None,
            manual_window_open: false,
            #[cfg(target_arch = "wasm32")]
            status_message: None,
        }
    }
}

impl FigApp {
    /// Whether the app asks for a newer release when it starts (off until the desktop app turns it on).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_startup_update_check(&mut self, on: bool) {
        self.updates_rt.startup_check = on;
    }
}

pub(crate) fn check(app: &mut FigApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.updates_rt.check_requested = true;
        app.updates_rt.manual_window_open = true;
        if let Some(ctx) = &app.updates_rt.repaint {
            ctx.request_repaint();
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        app.updates_rt.manual_window_open = true;
        app.updates_rt.status_message = Some(tr("Update check is not supported in the web browser.").to_string());
    }
}

pub(crate) fn tick(app: &mut FigApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let rt = &mut app.updates_rt;
        rt.repaint = Some(ctx.clone());
        if std::mem::take(&mut rt.check_requested) {
            rt.updater.check_now(ctx);
        } else if rt.startup_check {
            rt.updater.check_at_start(ctx);
        }
        rt.updater.tick(ctx);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

pub(crate) fn show(app: &mut FigApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Never call Updater::show/show_if: those expose official-package installation.
        let rt = &mut app.updates_rt;
        if rt.manual_window_open {
            let release = rt.release.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
            let response = if let Some(release) =
                release.filter(|_| !matches!(rt.updater.status(), craft_update::updater::Status::Checking | craft_update::updater::Status::Failed(_)))
            {
                let mut close = false;
                let mut again = false;
                let modal = egui::Modal::new(egui::Id::unique("figcraft.upstream-release-info")).show(ctx, |ui| {
                    ui.set_max_width(420.0);
                    ui.heading("Official FigCraft upstream release");
                    ui.label(format!("Upstream version: {}. Local base version: {}.", release.tag, env!("CARGO_PKG_VERSION")));
                    ui.label(FORK_NOTICE);
                    ui.hyperlink_to("View official release information", UPSTREAM_APP.releases_url());
                    ui.horizontal(|ui| {
                        close = ui.button("Close").clicked();
                        again = ui.button("Check again").clicked();
                    });
                });
                if close || modal.should_close() {
                    CheckResultResponse::Close
                } else if again {
                    CheckResultResponse::CheckAgain
                } else {
                    CheckResultResponse::None
                }
            } else {
                egui::Area::new(egui::Id::unique("figcraft.local-update-policy"))
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 170.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        ui.set_max_width(400.0);
                        ui.label(FORK_NOTICE);
                        ui.hyperlink_to("Official upstream releases", UPSTREAM_APP.releases_url());
                    });
                CheckResultDialog::new(&UPSTREAM_APP, env!("CARGO_PKG_VERSION")).show(ctx, rt.updater.status())
            };
            match response {
                CheckResultResponse::None => {}
                CheckResultResponse::Close => {
                    rt.manual_window_open = false;
                    rt.updater.dismiss_status();
                }
                CheckResultResponse::CheckAgain => rt.check_requested = true,
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    if app.updates_rt.manual_window_open {
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique("figcraft.update-result")).show(ctx, |ui| {
            ui.heading(tr("Check for Updates"));
            if let Some(msg) = &app.updates_rt.status_message {
                ui.label(msg);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        });
        if response.should_close() || close {
            app.updates_rt.manual_window_open = false;
            app.updates_rt.status_message = None;
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    fn a_new_official_release_never_exposes_package_installation() {
        let mut app = FigApp::new(figcraft_engine::Session::new(), Box::new(crate::NoServices));
        let release = craft_update::release::Release { tag: "v99.0.0".into(), version: craft_update::version::parse("99.0.0"), ..Default::default() };
        app.updates_rt.updater.offer(release.clone());
        *app.updates_rt.release.lock().unwrap() = Some(release);
        app.updates_rt.manual_window_open = true;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut warmup = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui.ctx()));
        warmup.textures_delta.clear();
        let mut out = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))), ..Default::default() },
            |ui| show(&mut app, ui.ctx()),
        );
        out.textures_delta.clear();
        let labels: Vec<_> = out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .filter_map(|(_, node)| node.label().or(node.value()).map(str::to_string))
            .collect();
        assert!(labels.iter().any(|s| s.contains("Official FigCraft upstream release")), "{labels:?}");
        assert!(labels.iter().any(|s| s.contains("automatic download, installation and replacement are disabled")));
        assert!(!labels.iter().any(|s| s.contains("UPDATE AND RESTART") || s == "Download"));
        assert!(!out.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close))));
    }
}
