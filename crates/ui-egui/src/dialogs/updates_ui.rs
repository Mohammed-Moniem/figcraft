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

// Exact production check/tick/show path with a bounded in-memory release source.
// These are deterministic egui fixtures, not native desktop interaction or network tests.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod lifecycle_tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use craft_update::updater::Status;
    use egui::accesskit::{Node, Role};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};

    type Answer = Result<Option<craft_update::release::Release>, String>;

    fn controlled() -> (FigApp, mpsc::Sender<Answer>, Arc<AtomicUsize>, tempfile::TempDir) {
        let mut app = FigApp::new(figcraft_engine::Session::new(), Box::new(crate::NoServices));
        let pins = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel::<Answer>();
        let rx = Mutex::new(rx);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_calls = calls.clone();
        let offered = app.updates_rt.release.clone();
        app.updates_rt.updater = Updater::new(&UPSTREAM_APP, env!("CARGO_PKG_VERSION"))
            .with_pins_path(pins.path().join("owned-absent-versions.json"))
            .with_source(Arc::new(move || {
                observed_calls.fetch_add(1, Ordering::SeqCst);
                let answer =
                    rx.lock().unwrap().recv_timeout(Duration::from_secs(3)).unwrap_or_else(|_| Err("owned deterministic source deadline".into()));
                // This is the same result handoff as Runtime::default; no HTTP client exists here.
                if let Ok(found) = &answer {
                    *offered.lock().unwrap() = found.clone();
                }
                answer
            }));
        (app, tx, calls, pins)
    }

    fn frame(app: &mut FigApp, ctx: &egui::Context, time: &mut f64, events: Vec<egui::Event>) -> egui::FullOutput {
        *time += 1.0 / 60.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ui| show(app, ui.ctx()),
        );
        output.textures_delta.clear();
        output
    }

    fn nodes(output: &egui::FullOutput) -> Vec<&Node> {
        output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().map(|(_, node)| node).collect()
    }

    fn has(output: &egui::FullOutput, text: &str) -> bool {
        nodes(output).iter().any(|node| node.label().or(node.value()).is_some_and(|label| label.contains(text)))
    }

    fn no_install(output: &egui::FullOutput) {
        assert!(
            !nodes(output).iter().any(|node| node
                .label()
                .or(node.value())
                .is_some_and(|label| label == "Download" || label.contains("UPDATE AND RESTART") || label.contains("Install update"))),
            "no replacement action"
        );
        assert!(
            !output.viewport_output.values().any(|viewport| viewport.commands.iter().any(|command| matches!(command, egui::ViewportCommand::Close))),
            "information dialog cannot quit the app"
        );
    }

    fn point(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        let bounds = nodes(output)
            .into_iter()
            .find(|node| node.label() == Some(label) && node.bounds().is_some())
            .unwrap_or_else(|| panic!("missing actual control: {label}"))
            .bounds()
            .unwrap();
        egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32)
    }

    fn click(app: &mut FigApp, ctx: &egui::Context, time: &mut f64, at: egui::Pos2) -> egui::FullOutput {
        frame(app, ctx, time, vec![egui::Event::PointerMoved(at)]);
        frame(
            app,
            ctx,
            time,
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }],
        );
        frame(
            app,
            ctx,
            time,
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }],
        )
    }

    fn settle(app: &mut FigApp, ctx: &egui::Context) {
        let limit = Instant::now() + Duration::from_secs(4);
        while matches!(app.updates_rt.updater.status(), Status::Checking) && Instant::now() < limit {
            tick(app, ctx);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!matches!(app.updates_rt.updater.status(), Status::Checking), "bounded deterministic check completed");
    }

    fn escape() -> egui::Event {
        egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
    }

    #[test]
    fn checking_spinner_then_up_to_date_close_uses_the_production_manual_lifecycle() {
        let (mut app, answer, calls, _pins) = controlled();
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        ctx.enable_accesskit();
        let document = app.session.ds().unwrap().doc.clone();
        let undo = app.session.undo_depth();
        check(&mut app);
        assert!(app.updates_rt.manual_window_open && app.updates_rt.check_requested);
        tick(&mut app, &ctx);
        assert_eq!(app.updates_rt.updater.status(), &Status::Checking);
        frame(&mut app, &ctx, &mut time, vec![]);
        let checking = frame(&mut app, &ctx, &mut time, vec![]);
        assert!(has(&checking, "Checking for updates"));
        assert!(nodes(&checking).iter().any(|node| node.role() == Role::ProgressIndicator), "real spinner widget is present");
        assert!(has(&checking, "automatic download, installation and replacement are disabled"));
        no_install(&checking);
        answer.send(Ok(None)).unwrap();
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.updater.status(), &Status::UpToDate);
        frame(&mut app, &ctx, &mut time, vec![]);
        let result = frame(&mut app, &ctx, &mut time, vec![]);
        assert!(has(&result, "newest version available"));
        assert!(!nodes(&result).iter().any(|node| node.role() == Role::ProgressIndicator));
        no_install(&result);
        let close = point(&result, "OK");
        no_install(&click(&mut app, &ctx, &mut time, close));
        assert!(!app.updates_rt.manual_window_open);
        assert_eq!(app.updates_rt.updater.status(), &Status::Idle);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(std::sync::Arc::ptr_eq(&document, &app.session.ds().unwrap().doc));
        assert_eq!(app.session.undo_depth(), undo);
    }

    #[test]
    fn failed_check_retries_only_after_actual_check_again_and_escape_closes() {
        let (mut app, answer, calls, _pins) = controlled();
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        ctx.enable_accesskit();
        check(&mut app);
        tick(&mut app, &ctx);
        answer.send(Err("owned offline release source".into())).unwrap();
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.updater.status(), &Status::Failed("owned offline release source".into()));
        frame(&mut app, &ctx, &mut time, vec![]);
        let failed = frame(&mut app, &ctx, &mut time, vec![]);
        assert!(has(&failed, "Could not check for updates") && has(&failed, "owned offline release source"));
        no_install(&failed);
        for _ in 0..3 {
            tick(&mut app, &ctx);
            frame(&mut app, &ctx, &mut time, vec![]);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "painting failure never retries a network operation");
        let retry = point(&failed, "CHECK AGAIN");
        click(&mut app, &ctx, &mut time, retry);
        assert!(app.updates_rt.check_requested);
        tick(&mut app, &ctx);
        assert_eq!(app.updates_rt.updater.status(), &Status::Checking);
        answer.send(Ok(None)).unwrap();
        settle(&mut app, &ctx);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        no_install(&frame(&mut app, &ctx, &mut time, vec![escape()]));
        assert!(!app.updates_rt.manual_window_open);
        assert_eq!(app.updates_rt.updater.status(), &Status::Idle);
    }

    #[test]
    fn offered_release_link_is_output_only_and_close_or_escape_never_installs() {
        for use_escape in [false, true] {
            let (mut app, answer, calls, _pins) = controlled();
            let (ctx, mut time) = (egui::Context::default(), 0.0);
            ctx.enable_accesskit();
            check(&mut app);
            tick(&mut app, &ctx);
            answer
                .send(Ok(Some(craft_update::release::Release {
                    tag: "v99.0.0".into(),
                    version: craft_update::version::parse("99.0.0"),
                    ..Default::default()
                })))
                .unwrap();
            settle(&mut app, &ctx);
            frame(&mut app, &ctx, &mut time, vec![]);
            let offered = frame(&mut app, &ctx, &mut time, vec![]);
            assert!(has(&offered, "Official FigCraft upstream release") && has(&offered, "v99.0.0"));
            no_install(&offered);
            let link = point(&offered, "View official release information");
            let linked = click(&mut app, &ctx, &mut time, link);
            assert!(
                linked.platform_output.commands.iter().any(|command| matches!(command,
                egui::OutputCommand::OpenUrl(url) if url.url == UPSTREAM_APP.releases_url())),
                "only an explicit release-page URL is handed to the host"
            );
            no_install(&linked);
            assert!(app.updates_rt.manual_window_open && !app.updates_rt.check_requested);
            frame(&mut app, &ctx, &mut time, vec![]);
            let shown = frame(&mut app, &ctx, &mut time, vec![]);
            let closed = if use_escape {
                frame(&mut app, &ctx, &mut time, vec![escape()])
            } else {
                let at = point(&shown, "Close");
                click(&mut app, &ctx, &mut time, at)
            };
            no_install(&closed);
            assert!(!app.updates_rt.manual_window_open);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn escape_during_check_keeps_a_late_release_from_reopening_the_manual_dialog() {
        let (mut app, answer, calls, _pins) = controlled();
        let (ctx, mut time) = (egui::Context::default(), 0.0);
        ctx.enable_accesskit();
        check(&mut app);
        tick(&mut app, &ctx);
        frame(&mut app, &ctx, &mut time, vec![]);
        no_install(&frame(&mut app, &ctx, &mut time, vec![escape()]));
        assert!(!app.updates_rt.manual_window_open);
        answer
            .send(Ok(Some(craft_update::release::Release {
                tag: "v99.0.0".into(),
                version: craft_update::version::parse("99.0.0"),
                ..Default::default()
            })))
            .unwrap();
        let limit = Instant::now() + Duration::from_secs(4);
        while calls.load(Ordering::SeqCst) == 0 && Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..20 {
            tick(&mut app, &ctx);
            no_install(&frame(&mut app, &ctx, &mut time, vec![]));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.updates_rt.manual_window_open, "late answer does not reopen cancelled manual UI");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // Full FigApp frame routing, using the existing deterministic in-memory updater source.
    // No check is requested: the owned offered-release modal exercises keyboard ownership
    // without HTTP, OS input or an application launch.
    struct ModalKeyboardFixture {
        ui: crate::layers_drag_tests::Ui,
        node: figcraft_doc::NodeId,
        _answer: mpsc::Sender<Answer>,
        calls: Arc<AtomicUsize>,
        _pins: tempfile::TempDir,
    }

    fn modal_keyboard_fixture() -> ModalKeyboardFixture {
        let (mut app, answer, calls, pins) = controlled();
        let value = app.session.execute("shape.rectangle", &serde_json::json!({"x": 20, "y": 20, "width": 80, "height": 60})).unwrap();
        let node = figcraft_doc::NodeId(value["id"].as_u64().unwrap());
        app.session.execute("select.set", &serde_json::json!({"ids": [node]})).unwrap();
        let mut ui = crate::layers_drag_tests::Ui::new(figcraft_engine::Session::new());
        ui.app = app;
        ui.ctx.enable_accesskit();
        for _ in 0..3 {
            ui.frame(vec![]);
        }
        assert_eq!(ui.app.session.selection(), vec![node]);
        ModalKeyboardFixture { ui, node, _answer: answer, calls, _pins: pins }
    }

    fn open_owned_release_modal(ui: &mut crate::layers_drag_tests::Ui) {
        let release = craft_update::release::Release { tag: "v99.0.0".into(), version: craft_update::version::parse("99.0.0"), ..Default::default() };
        ui.app.updates_rt.updater.offer(release.clone());
        *ui.app.updates_rt.release.lock().unwrap() = Some(release);
        ui.app.updates_rt.manual_window_open = true;
    }

    fn modal_key(key: egui::Key) -> Vec<egui::Event> {
        [true, false].map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() }).into()
    }

    struct ModalDocumentState {
        document: Arc<figcraft_doc::Document>,
        revision: u64,
        page: figcraft_doc::NodeId,
        selection: Vec<figcraft_doc::NodeId>,
        history: (usize, usize),
        dirty: bool,
    }

    fn modal_document_state(ui: &crate::layers_drag_tests::Ui) -> ModalDocumentState {
        let ds = ui.app.session.ds().unwrap();
        ModalDocumentState {
            document: ds.doc.clone(),
            revision: ds.revision,
            page: ds.page,
            selection: ds.selection.clone(),
            history: ui.app.session.undo_depth(),
            dirty: ds.is_dirty(),
        }
    }

    fn assert_modal_preserved_document(ui: &crate::layers_drag_tests::Ui, before: &ModalDocumentState) {
        let after = ui.app.session.ds().unwrap();
        assert!(Arc::ptr_eq(&before.document, &after.doc), "a modal key must not detach or edit authored content");
        assert_eq!(*before.document, *after.doc);
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.page, before.page);
        assert_eq!(after.selection, before.selection);
        assert_eq!(ui.app.session.undo_depth(), before.history);
        assert_eq!(after.is_dirty(), before.dirty);
    }

    #[test]
    fn updater_modal_escape_closes_without_deselecting_the_authored_selection() {
        let mut fixture = modal_keyboard_fixture();
        open_owned_release_modal(&mut fixture.ui);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(vec![]);
        assert!(fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_some()));
        let before = modal_document_state(&fixture.ui);
        fixture.ui.frame(modal_key(egui::Key::Escape));
        assert!(!fixture.ui.app.updates_rt.manual_window_open, "the modal still owns its normal Escape dismissal");
        assert_modal_preserved_document(&fixture.ui, &before);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0, "the fixture never requested a release check");
    }

    #[test]
    fn updater_modal_delete_cannot_edit_a_selected_node_on_its_first_frame() {
        let mut fixture = modal_keyboard_fixture();
        assert!(
            fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_none()),
            "no previous modal can accidentally protect the first-frame test"
        );
        let before = modal_document_state(&fixture.ui);
        open_owned_release_modal(&mut fixture.ui);
        fixture.ui.frame(modal_key(egui::Key::Delete));
        assert!(fixture.ui.app.updates_rt.manual_window_open);
        assert!(fixture.ui.app.session.doc().unwrap().node(fixture.node).is_some(), "Delete belongs to the opening modal");
        assert_modal_preserved_document(&fixture.ui, &before);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        // The opening may be an invisible sizing pass. Verify input ownership immediately
        // above, then require the real modal text on later ordinary painted frames.
        fixture.ui.frame(vec![]);
        let output = fixture.ui.frame_output(vec![]);
        assert!(has(&output, "Official FigCraft upstream release"), "the actual offered-release modal was drawn after settling");
        assert_modal_preserved_document(&fixture.ui, &before);
    }

    #[test]
    fn updater_modal_delete_cannot_edit_a_selected_node_after_the_modal_is_visible() {
        let mut fixture = modal_keyboard_fixture();
        open_owned_release_modal(&mut fixture.ui);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(vec![]);
        assert!(fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_some()));
        let before = modal_document_state(&fixture.ui);
        fixture.ui.frame(modal_key(egui::Key::Delete));
        assert!(fixture.ui.app.updates_rt.manual_window_open);
        assert!(fixture.ui.app.session.doc().unwrap().node(fixture.node).is_some());
        assert_modal_preserved_document(&fixture.ui, &before);
    }

    #[test]
    fn a_generic_egui_modal_also_blocks_the_canvas_delete_shortcut() {
        let mut fixture = modal_keyboard_fixture();
        assert!(!fixture.ui.app.updates_rt.manual_window_open, "this test must exercise generic modal ownership");
        let before = modal_document_state(&fixture.ui);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
            events: modal_key(egui::Key::Delete),
            ..Default::default()
        };
        let ctx = fixture.ui.ctx.clone();
        // egui reports the PREVIOUS frame's modal layer to route current input. Warm this
        // same modal once; a first-show assertion would fail independently of the dispatcher.
        let mut warmup = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))), ..Default::default() },
            |ui| {
                egui::Modal::new(egui::Id::unique("owned-generic-modal-keyboard-fixture")).show(ui.ctx(), |ui| {
                    ui.label("Owned generic modal");
                });
            },
        );
        warmup.textures_delta.clear();
        let mut output = ctx.run_ui(input, |ui| {
            egui::Modal::new(egui::Id::unique("owned-generic-modal-keyboard-fixture")).show(ui.ctx(), |ui| {
                ui.label("Owned generic modal");
            });
            assert!(ui.ctx().memory(|memory| memory.top_modal_layer().is_some()));
            crate::shortcuts::handle(&mut fixture.ui.app, ui.ctx(), false, Default::default());
        });
        output.textures_delta.clear();
        assert_modal_preserved_document(&fixture.ui, &before);
    }

    #[test]
    fn ordinary_escape_and_delete_still_work_without_a_modal() {
        let mut fixture = modal_keyboard_fixture();
        let before = fixture.ui.app.session.ds().unwrap().doc.clone();
        fixture.ui.frame(modal_key(egui::Key::Escape));
        assert!(fixture.ui.app.session.selection().is_empty());
        assert!(Arc::ptr_eq(&before, &fixture.ui.app.session.ds().unwrap().doc));
        fixture.ui.app.session.execute("select.set", &serde_json::json!({"ids": [fixture.node]})).unwrap();
        let history = fixture.ui.app.session.undo_depth();
        fixture.ui.frame(modal_key(egui::Key::Delete));
        assert!(fixture.ui.app.session.doc().unwrap().node(fixture.node).is_none());
        assert_eq!(fixture.ui.app.session.undo_depth().0, history.0 + 1);
    }

    #[test]
    fn closing_the_updater_modal_returns_delete_to_the_canvas() {
        let mut fixture = modal_keyboard_fixture();
        open_owned_release_modal(&mut fixture.ui);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(modal_key(egui::Key::Escape));
        assert!(!fixture.ui.app.updates_rt.manual_window_open);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(vec![]);
        assert!(fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_none()), "the old modal has retired");
        fixture.ui.app.session.execute("select.set", &serde_json::json!({"ids": [fixture.node]})).unwrap();
        fixture.ui.frame(modal_key(egui::Key::Delete));
        assert!(fixture.ui.app.session.doc().unwrap().node(fixture.node).is_none());
    }

    /// Find and click the actual Design X spin field, retaining its genuine uncommitted draft.
    fn focus_modal_x_draft(fixture: &mut ModalKeyboardFixture) {
        fixture.ui.frame(vec![]);
        let output = fixture.ui.frame_output(vec![]);
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let bounds =
            nodes.iter().map(|(_, node)| node).find(|node| node.role() == Role::SpinButton && node.label() == Some("X")).unwrap().bounds().unwrap();
        let at = egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32);
        fixture.ui.button(at, true);
        fixture.ui.button(at, false);
        fixture.ui.frame(vec![]);
        assert!(crate::a11y::field_focused(&fixture.ui.ctx));
        assert!(fixture.ui.ctx.text_edit_focused());
        assert_eq!(fixture.ui.ctx.memory(|memory| memory.focused()), Some(egui::Id::unique("pos_x").with("te")));
        fixture.ui.frame(vec![egui::Event::Text("77".into())]);
        assert_eq!(fixture.ui.ctx.data(|data| data.get_temp::<String>(egui::Id::unique("pos_x").with("editing"))), Some("77".into()));
        assert_eq!(fixture.ui.app.session.doc().unwrap().world_bounds(fixture.node).unwrap().x0, 20.0, "the real X draft has not committed");
    }

    #[test]
    fn opening_updater_modal_blocks_save_preparation_from_committing_a_focused_x_draft() {
        for save_as in [false, true] {
            let mut fixture = modal_keyboard_fixture();
            let path = fixture._pins.path().join(if save_as { "owned-shift-save.figcraft" } else { "owned-save.figcraft" });
            fixture.ui.app.session.execute("file.saveAs", &serde_json::json!({"path": path.to_string_lossy()})).unwrap();
            fixture.ui.frame(vec![]);
            let bytes = std::fs::read(&path).unwrap();
            focus_modal_x_draft(&mut fixture);
            let before = modal_document_state(&fixture.ui);
            assert!(fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_none()), "the first opening must not inherit a previous modal guard");
            open_owned_release_modal(&mut fixture.ui);
            let mut modifiers =
                if cfg!(target_os = "macos") { egui::Modifiers::COMMAND } else { egui::Modifiers::COMMAND.plus(egui::Modifiers::CTRL) };
            if save_as {
                modifiers = modifiers.plus(egui::Modifiers::SHIFT);
            }
            let events = [true, false].map(|pressed| egui::Event::Key {
                key: egui::Key::S,
                physical_key: Some(egui::Key::S),
                pressed,
                repeat: false,
                modifiers,
            });
            fixture.ui.frame_with(events.into(), modifiers);
            assert!(fixture.ui.app.updates_rt.manual_window_open);
            assert!(fixture.ui.app.ui.after_field.is_none(), "the modal key cannot queue a deferred save");
            assert!(fixture.ui.app.pending_dialog.is_none(), "the modal key cannot open Save as");
            assert_eq!(
                fixture.ui.app.session.doc().unwrap().world_bounds(fixture.node).unwrap().x0,
                20.0,
                "no first-frame save marker may force the real draft into the document"
            );
            assert_modal_preserved_document(&fixture.ui, &before);
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "the original native file stays unchanged");
            assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn updater_modal_blocks_canvas_text_and_paste_then_resumes_the_same_editor() {
        let mut fixture = modal_keyboard_fixture();
        let value = fixture.ui.app.session.execute("text.create", &serde_json::json!({"text": "Keep text", "x": 160, "y": 20})).unwrap();
        let text = figcraft_doc::NodeId(value["id"].as_u64().unwrap());
        fixture.ui.app.session.execute("select.set", &serde_json::json!({"ids": [text]})).unwrap();
        fixture.ui.frame(vec![]);
        fixture.ui.frame(modal_key(egui::Key::Enter));
        fixture.ui.frame(vec![]);
        let edit = fixture.ui.app.session.ds().unwrap().text_edit.clone().expect("the real Enter path begins canvas text editing");
        assert_eq!(edit.id, text);
        let before = modal_document_state(&fixture.ui);
        open_owned_release_modal(&mut fixture.ui);
        fixture.ui.frame(vec![egui::Event::Text("blocked text".into()), egui::Event::Paste("blocked paste".into())]);
        assert!(fixture.ui.app.updates_rt.manual_window_open);
        assert_modal_preserved_document(&fixture.ui, &before);
        let active = fixture.ui.app.session.ds().unwrap().text_edit.as_ref().unwrap();
        assert_eq!((active.id, active.editing_session), (text, edit.editing_session), "modal input must not replace or end the underlying editor");
        fixture.ui.frame(modal_key(egui::Key::Escape));
        assert!(!fixture.ui.app.updates_rt.manual_window_open);
        assert_modal_preserved_document(&fixture.ui, &before);
        fixture.ui.frame(vec![]);
        fixture.ui.frame(vec![]);
        assert!(fixture.ui.ctx.memory(|memory| memory.top_modal_layer().is_none()));
        fixture.ui.frame(vec![egui::Event::Text("resumed".into())]);
        let active = fixture.ui.app.session.ds().unwrap().text_edit.as_ref().unwrap();
        assert_eq!((active.id, active.editing_session), (text, edit.editing_session));
        let characters = &fixture.ui.app.session.doc().unwrap().node(text).unwrap().kind.text().unwrap().characters;
        assert!(characters.contains("resumed"), "ordinary input resumes in the same editor after dismissal");
        assert!(!characters.contains("blocked text") && !characters.contains("blocked paste"));
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    }
}

// This calls the cfg(wasm32) production branch. It is not executable in a native cargo test;
// actual browser acceptance or a configured WASM test runner is still required.
#[cfg(all(test, target_arch = "wasm32"))]
mod unsupported_browser_tests {
    use super::*;
    #[test]
    fn browser_check_is_an_unsupported_informational_modal_and_escape_clears_it() {
        let mut app = FigApp::new(figcraft_engine::Session::new(), Box::new(crate::NoServices));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        check(&mut app);
        assert!(app.updates_rt.manual_window_open);
        assert!(app.updates_rt.status_message.as_deref().is_some_and(|text| text.contains("not supported in the web browser")));
        for _ in 0..2 {
            tick(&mut app, &ctx);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut app, ui.ctx()));
            out.textures_delta.clear();
            assert!(out.platform_output.commands.is_empty(), "no automatic URL/fetch/install command is emitted");
        }
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ui| show(&mut app, ui.ctx()),
        );
        out.textures_delta.clear();
        assert!(!app.updates_rt.manual_window_open && app.updates_rt.status_message.is_none());
        assert!(out.platform_output.commands.is_empty());
    }
}
