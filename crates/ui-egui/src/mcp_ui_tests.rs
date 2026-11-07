//! The Local MCP Server window and the Plugins window's MCP and Development tabs, driven headless
//! through egui at both desktop (1440×900) and narrow (340×600) viewports: every control stays
//! inside the dialog without horizontal overflow, toggling the server applies and saves preferences
//! at once, and the Plugin Dev Console displays and clears execution history.

use std::sync::{Arc, Mutex};

use egui::accesskit::Node;
use egui::{Id, Pos2, RawInput, Rect, vec2};
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;
use crate::plugins_ui::PluginsTab;

#[derive(Default)]
struct McpLog {
    applied: Vec<(bool, u16)>,
    saved: Vec<figcraft_engine::Prefs>,
    token: Option<std::path::PathBuf>,
    config_root: Option<std::path::PathBuf>,
    installed: Vec<figcraft_engine::harness::HarnessKind>,
}

struct MockMcpServices(Arc<Mutex<McpLog>>);

impl crate::Services for MockMcpServices {
    fn mcp_control_token_file(&self, _port: u16) -> Option<std::path::PathBuf> {
        self.0.lock().unwrap().token.clone()
    }
    fn install_mcp_harness(
        &mut self,
        kind: figcraft_engine::harness::HarnessKind,
        cli: &str,
        port: u16,
    ) -> Result<figcraft_engine::harness::HarnessStatus, String> {
        let mut log = self.0.lock().unwrap();
        let root = log.config_root.as_deref().ok_or("test config root required")?;
        let token = log.token.as_deref().ok_or("test token required")?;
        let status =
            figcraft_engine::harness::install_at_with_token_file(kind, &figcraft_engine::harness::config_path_in(kind, root), cli, port, token)?;
        log.installed.push(kind);
        Ok(status)
    }
    fn mcp_server_status(&self) -> Option<(u16, usize, Option<String>)> {
        Some((47800, 1, None))
    }

    fn mcp_server_apply(&mut self, enabled: bool, port: u16) {
        self.0.lock().unwrap().applied.push((enabled, port));
    }

    fn save_prefs(&mut self, prefs: &figcraft_engine::Prefs) {
        self.0.lock().unwrap().saved.push(prefs.clone());
    }
}

/// Returns the center position of an AccessKit node's bounding box.
fn centre(n: &Node) -> Pos2 {
    let b = n.bounds().unwrap();
    Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
}

/// Runs one egui frame at `screen` resolution and collects all AccessKit nodes.
fn nodes_at(u: &mut Ui, screen: egui::Vec2) -> Vec<Node> {
    let out = u.frame_raw(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)), ..Default::default() });
    out.platform_output.accesskit_update.unwrap().nodes.into_iter().map(|(_, n)| n).collect()
}

/// Finds all AccessKit nodes whose label equals `label` at 1440×900.
fn named(u: &mut Ui, label: &str) -> Vec<Node> {
    nodes_at(u, vec2(1440.0, 900.0)).into_iter().filter(|n| n.label() == Some(label)).collect()
}

/// Returns whether any AccessKit node label or value contains `text` at 1440×900.
fn shows(u: &mut Ui, text: &str) -> bool {
    nodes_at(u, vec2(1440.0, 900.0)).iter().any(|n| n.label().or(n.value()).is_some_and(|l| l.contains(text)))
}

/// Simulates a primary mouse press and release at `at`.
fn click(u: &mut Ui, at: Pos2) {
    u.button(at, true);
    u.button(at, false);
}

/// Verifies that the MCP tab and standalone MCP dialog fit within both standard and narrow viewports.
#[test]
fn mcp_tab_and_window_fit_standard_and_narrow_viewports() {
    for screen in [vec2(1440.0, 900.0), vec2(340.0, 600.0)] {
        let mut u = Ui::new(Session::new());
        u.ctx.enable_accesskit();
        u.app.ui.plugins_window = true;
        u.app.ui.plugins_manage.tab = PluginsTab::Mcp;
        for _ in 0..3 {
            let _ = nodes_at(&mut u, screen);
        }
        let nodes = nodes_at(&mut u, screen);
        let win = u.ctx.memory(|m| m.area_rect(Id::unique("Plugins"))).expect("Plugins window rect");
        assert!(win.min.x >= -1.0 && win.max.x <= screen.x + 1.0, "Plugins window {win:?} stays inside screen {screen:?}");

        for label in [
            "Enable local MCP control server",
            "Copy Command",
            "Copy all configurations",
            "Copy Config",
            "Antigravity",
            "Claude Code",
            "Claude Desktop",
            "Codex",
        ] {
            let matching: Vec<&Node> = nodes.iter().filter(|n| n.label().or(n.value()) == Some(label) && n.bounds().is_some()).collect();
            assert!(!matching.is_empty(), "{label} is rendered on {screen:?}");
            for n in matching {
                let b = n.bounds().unwrap();
                assert!(
                    b.x0 as f32 >= win.min.x - 2.0 && b.x1 as f32 <= win.max.x + 2.0,
                    "{label} bounds {b:?} overflow window {win:?} on screen {screen:?}"
                );
            }
        }

        // Also verify the standalone Local MCP Server dialog on the same viewport.
        u.app.ui.plugins_window = false;
        u.app.ui.mcp_window = true;
        for _ in 0..3 {
            let _ = nodes_at(&mut u, screen);
        }
        let mcp_nodes = nodes_at(&mut u, screen);
        let mcp_win = u.ctx.memory(|m| m.area_rect(Id::unique("Local MCP Server"))).expect("Local MCP Server window rect");
        assert!(mcp_win.min.x >= -1.0 && mcp_win.max.x <= screen.x + 1.0, "Local MCP Server window {mcp_win:?} stays inside screen {screen:?}");
        for label in ["Copy Command", "Install all clients", "Copy all configurations", "Copy Config", "Install"] {
            let matching: Vec<&Node> = mcp_nodes.iter().filter(|n| n.label().or(n.value()) == Some(label) && n.bounds().is_some()).collect();
            assert!(!matching.is_empty(), "{label} is rendered in standalone dialog on {screen:?}");
            for n in matching {
                let b = n.bounds().unwrap();
                assert!(
                    b.x0 as f32 >= mcp_win.min.x - 2.0 && b.x1 as f32 <= mcp_win.max.x + 2.0,
                    "{label} bounds {b:?} overflow standalone window {mcp_win:?} on screen {screen:?}"
                );
            }
        }
    }
}

#[test]
fn explicit_install_buttons_use_production_writer_only_after_a_token_exists_in_temporary_roots() {
    use figcraft_engine::harness::{HarnessKind, config_path_in};
    let temp = tempfile::tempdir().unwrap();
    let token = temp.path().join("published-control.token");
    let log = Arc::new(Mutex::new(McpLog { config_root: Some(temp.path().to_path_buf()), ..Default::default() }));
    let mut u = Ui::new(Session::new());
    u.app.services = Box::new(MockMcpServices(log.clone()));
    u.ctx.enable_accesskit();
    u.app.ui.mcp_window = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let install = centre(&named(&mut u, "Install all clients")[0]);
    click(&mut u, install);
    assert!(log.lock().unwrap().installed.is_empty());
    assert!(HarnessKind::ALL.iter().all(|kind| !config_path_in(*kind, temp.path()).exists()));
    std::fs::write(&token, "synthetic-token-content-never-installed").unwrap();
    log.lock().unwrap().token = Some(token.clone());
    // The longer authenticated command reflows and recenters the floating dialog.
    for _ in 0..4 {
        u.frame(vec![]);
    }
    let buttons = named(&mut u, "Install");
    // AccessKit updates are not in visual order; later cards may be clipped by the scroll area.
    let button = buttons.iter().min_by(|a, b| a.bounds().unwrap().y0.total_cmp(&b.bounds().unwrap().y0)).unwrap();
    let one = centre(button);
    click(&mut u, one);
    assert_eq!(
        log.lock().unwrap().installed.len(),
        1,
        "the individual client button invokes the real temp-root installer; at {one:?}, button {:?}, window {:?}, toast {:?}",
        buttons[0],
        u.ctx.memory(|m| m.area_rect(Id::unique("Local MCP Server"))),
        u.app.ui.toast
    );
    let install = centre(&named(&mut u, "Install all clients")[0]);
    click(&mut u, install);
    assert_eq!(log.lock().unwrap().installed.len(), 5);
    for kind in HarnessKind::ALL {
        let text = std::fs::read_to_string(config_path_in(kind, temp.path())).unwrap();
        assert!(text.contains("--control-token-file") && text.contains("published-control.token"));
        assert!(!text.contains("synthetic-token-content-never-installed"));
    }
    // A refresh must not leave a stale unauthenticated snippet in the Copy action.
    let refresh = centre(&named(&mut u, "Refresh")[0]);
    click(&mut u, refresh);
    for _ in 0..4 {
        u.frame(vec![]);
    }
    let copy = centre(&named(&mut u, "Copy all configurations")[0]);
    u.frame(vec![egui::Event::PointerMoved(copy)]);
    let mut commands = Vec::new();
    for pressed in [true, false] {
        commands.extend(
            u.frame_output(vec![
                egui::Event::PointerMoved(copy),
                egui::Event::PointerButton { pos: copy, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() },
            ])
            .platform_output
            .commands,
        );
    }
    commands.extend(u.frame_output(vec![]).platform_output.commands);
    assert!(commands.iter().any(|c|matches!(c,egui::OutputCommand::CopyText(s)if s.contains("--control-token-file")&&s.contains("published-control.token")&&!s.contains("synthetic-token-content"))),"copy at {copy:?}, commands={commands:?}, toast={:?}",u.app.ui.toast);
}

/// Verifies toggling the MCP server, resetting the port, and copying the CLI command from the MCP tab.
#[test]
fn mcp_tab_toggles_server_resets_port_and_copies_command() {
    let temp = tempfile::tempdir().unwrap();
    let token = temp.path().join("actual.token");
    std::fs::write(&token, "synthetic").unwrap();
    let log = Arc::new(Mutex::new(McpLog::default()));
    log.lock().unwrap().token = Some(token);
    let mut u = Ui::new(Session::new());
    u.app.services = Box::new(MockMcpServices(log.clone()));
    u.app.ui.plugins_window = true;
    u.app.ui.plugins_manage.tab = PluginsTab::Installed;
    u.ctx.enable_accesskit();
    u.frame(vec![]);

    // Switch to the MCP tab via the tab strip.
    let mcp_tab = centre(&named(&mut u, "MCP")[0]);
    click(&mut u, mcp_tab);
    assert_eq!(u.app.ui.plugins_manage.tab, PluginsTab::Mcp);
    assert!(shows(&mut u, "Listening on 127.0.0.1:47800"));

    // Toggle the local MCP server on (from its opt-in `false` default) and verify Services::mcp_server_apply and save_prefs run.
    assert!(!u.app.session.prefs.mcp_enabled);
    let toggle = centre(&named(&mut u, "Enable local MCP control server")[0]);
    click(&mut u, toggle);
    assert!(u.app.session.prefs.mcp_enabled);
    assert_eq!(log.lock().unwrap().applied.last(), Some(&(true, 47800)));
    assert_eq!(log.lock().unwrap().saved.last().map(|p| p.mcp_enabled), Some(true));

    // Change the port, then click Reset to restore 47800.
    u.app.run("pref.mcp", json!({"port": 47911}));
    u.frame(vec![]);
    assert_eq!(u.app.session.prefs.mcp_port, 47911);
    let reset = centre(&named(&mut u, "Reset")[0]);
    click(&mut u, reset);
    assert_eq!(u.app.session.prefs.mcp_port, figcraft_engine::DEFAULT_MCP_PORT);

    // Click Copy Command and verify the toast confirmation.
    let copy_cmd = centre(&named(&mut u, "Copy Command")[0]);
    click(&mut u, copy_cmd);
    assert!(u.app.ui.toast.as_ref().is_some_and(|(t, _)| t.contains("Copied MCP command")), "{:?}", u.app.ui.toast);
}

/// Verifies that the Development tab displays and clears plugin execution console logs.
#[test]
fn development_tab_shows_and_clears_plugin_console_logs() {
    use base64::Engine as _;
    let mut s = Session::new();
    let wasm = wat::parse_str(
        r#"(module
            (import "figcraft" "log" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "hello from dev console")
            (func (export "figcraft_run") (param i32) (result i32)
                i32.const 0
                i32.const 22
                call $log
                i32.const 0))"#,
    )
    .unwrap();
    let manifest = json!({
        "id": "org.example.logger",
        "name": "Logger",
        "version": "1.0.0",
        "main": "l.wasm"
    });
    s.execute(
        "plugin.import",
        &json!({
            "manifest": manifest,
            "wasm": base64::engine::general_purpose::STANDARD.encode(&wasm),
        }),
    )
    .unwrap();
    s.execute("plugin.run", &json!({"id": "org.example.logger"})).unwrap();
    assert_eq!(s.plugins.history.len(), 1);

    let mut u = Ui::new(s);
    u.app.ui.plugins_window = true;
    u.app.ui.plugins_manage.tab = PluginsTab::Development;
    u.ctx.enable_accesskit();
    u.frame(vec![]);

    assert!(shows(&mut u, "hello from dev console"));
    let clear = centre(&named(&mut u, "Clear")[0]);
    click(&mut u, clear);
    assert!(u.app.session.plugins.history.is_empty());
}

#[test]
fn control_commands_apply_mcp_preferences_without_opening_dialogs() {
    let log = Arc::new(Mutex::new(McpLog::default()));
    let mut app = crate::FigApp::new(Session::new(), Box::new(MockMcpServices(log.clone())));
    let ctx = egui::Context::default();
    crate::control::handle(&mut app, &ctx, "engine.execute", &json!({"command":"pref.mcp","params":{"enabled":true,"port":47921}})).unwrap();
    assert_eq!(log.lock().unwrap().applied, vec![(true, 47921)]);
    assert_eq!(log.lock().unwrap().saved.last().map(|p| (p.mcp_enabled, p.mcp_port)), Some((true, 47921)));
    assert!(app.session.ui_requests.is_empty());
    assert!(crate::control::handle(&mut app, &ctx, "engine.execute", &json!({"command":"pref.mcp","params":{"port":0}})).is_err());
    assert_eq!(log.lock().unwrap().applied.len(), 1, "refused changes never reach the listener");
    crate::control::handle(&mut app, &ctx, "engine.execute", &json!({"command":"pref.mcp","params":{}})).unwrap();
    assert_eq!(log.lock().unwrap().applied.len(), 1, "queries cannot change listener leases");
    crate::control::handle(
        &mut app,
        &ctx,
        "engine.execute",
        &json!({"command":"command.batch","params":{"commands":[{"command":"pref.mcp","params":{"port":47922}}]}}),
    )
    .unwrap();
    assert_eq!(log.lock().unwrap().applied, vec![(true, 47921), (true, 47922)], "successful batches reconcile their final settings once");
    crate::control::handle(&mut app, &ctx, "engine.execute", &json!({"command":"pref.mcp","params":{"enabled":false}})).unwrap();
    assert_eq!(log.lock().unwrap().applied.last(), Some(&(false, 47922)));
    assert!(app.session.ui_requests.is_empty());
}
