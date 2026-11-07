//! Local MCP harness configuration for LLM coding agents (`Antigravity`, `Claude Code`,
//! `Claude Desktop`, `Codex`).
//!
//! Reads, inspects, and updates each harness's local configuration file so `figcraft-cli mcp
//! --attach <port>` (or headless `figcraft-cli mcp`) is registered under `figcraft`.
//! Because these files live in the user's home/config directories (`~/.gemini/antigravity/…`,
//! `~/.claude.json`, `~/.codex/config.toml`), file operations here use `#[allow(clippy::disallowed_methods)]`
//! just like preferences and installed plugins.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Supported LLM harnesses that speak the Model Context Protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarnessKind {
    Antigravity,
    ClaudeCode,
    ClaudeDesktop,
    Codex,
}

impl HarnessKind {
    pub const ALL: [HarnessKind; 4] = [HarnessKind::Antigravity, HarnessKind::ClaudeCode, HarnessKind::ClaudeDesktop, HarnessKind::Codex];

    /// Machine-readable identifier used in CLI flags and status output.
    pub fn id(self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::ClaudeCode => "claude-code",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Codex => "codex",
        }
    }

    /// Human-readable display name shown in the UI and CLI output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Antigravity => "Antigravity",
            Self::ClaudeCode => "Claude Code",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::Codex => "Codex",
        }
    }

    /// Parse a harness identifier or alias (`antigravity`, `claude-code`, `claude-desktop`, `codex`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "antigravity" | "agy" => Some(Self::Antigravity),
            "claude" | "claude-code" | "claudecode" => Some(Self::ClaudeCode),
            "claude-desktop" | "claudedesktop" => Some(Self::ClaudeDesktop),
            "codex" | "openai-codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

/// Inspection status of a single LLM harness on this machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessStatus {
    pub id: String,
    pub label: String,
    pub config_path: PathBuf,
    pub configured: bool,
    pub attached_port: Option<u16>,
    pub snippet: String,
}

/// Home directory for the current user.
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Default configuration file path for `kind` on `os` (`"macos"`, `"windows"`, `"linux"`, etc.)
/// given `home` and optional platform environment overrides (`APPDATA`, `XDG_CONFIG_HOME`,
/// `CODEX_HOME`).
pub fn config_path_for_os(
    kind: HarnessKind,
    os: &str,
    home: &Path,
    appdata: Option<&Path>,
    xdg_config: Option<&Path>,
    codex_home: Option<&Path>,
) -> PathBuf {
    match kind {
        HarnessKind::Antigravity => home.join(".gemini").join("antigravity").join("mcp_config.json"),
        HarnessKind::ClaudeCode => home.join(".claude.json"),
        HarnessKind::ClaudeDesktop => match os {
            "macos" => home.join("Library").join("Application Support").join("Claude").join("claude_desktop_config.json"),
            "windows" => appdata
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                .join("Claude")
                .join("claude_desktop_config.json"),
            _ => xdg_config.map(Path::to_path_buf).unwrap_or_else(|| home.join(".config")).join("Claude").join("claude_desktop_config.json"),
        },
        HarnessKind::Codex => codex_home.map(Path::to_path_buf).unwrap_or_else(|| home.join(".codex")).join("config.toml"),
    }
}

/// Default configuration file path for `kind` under `home`.
pub fn config_path_in(kind: HarnessKind, home: &Path) -> PathBuf {
    config_path_for_os(kind, std::env::consts::OS, home, None, None, None)
}

/// Read a non-empty environment variable as a `PathBuf`.
fn non_empty_env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Default configuration file path for `kind` on this system.
pub fn config_path(kind: HarnessKind) -> Option<PathBuf> {
    let home = home_dir()?;
    let appdata = non_empty_env_path("APPDATA");
    let xdg_config = non_empty_env_path("XDG_CONFIG_HOME");
    let codex_home = non_empty_env_path("CODEX_HOME");
    Some(config_path_for_os(kind, std::env::consts::OS, &home, appdata.as_deref(), xdg_config.as_deref(), codex_home.as_deref()))
}

/// Locate the `figcraft-cli` binary to invoke from MCP harness configs.
/// Checks `FIGCRAFT_CLI` first, then the current executable and its sibling directory, Linux
/// `APPIMAGE`, `~/.cargo/bin`, and falls back to `"figcraft-cli"` (or `"figcraft-cli.exe"` on
/// Windows) on `PATH`.
#[allow(clippy::disallowed_methods)]
pub fn detect_cli_binary() -> String {
    let bin_name = if cfg!(windows) { "figcraft-cli.exe" } else { "figcraft-cli" };
    if let Some(explicit) = non_empty_env_path("FIGCRAFT_CLI")
        && explicit.is_file()
    {
        return explicit.to_string_lossy().into_owned();
    }
    if let Ok(exe) = std::env::current_exe() {
        if exe.file_name().and_then(|n| n.to_str()) == Some(bin_name) && exe.is_file() {
            return exe.to_string_lossy().into_owned();
        }
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(bin_name);
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    if let Some(appimage) = non_empty_env_path("APPIMAGE")
        && appimage.is_file()
    {
        return appimage.to_string_lossy().into_owned();
    }
    if let Some(home) = home_dir() {
        let cargo_bin = home.join(".cargo").join("bin").join(bin_name);
        if cargo_bin.is_file() {
            return cargo_bin.to_string_lossy().into_owned();
        }
    }
    bin_name.to_string()
}

/// Whether `cli_bin` is a Linux `.AppImage` bundle (which runs the CLI via `<AppImage> cli ...`).
pub fn is_appimage(cli_bin: &str) -> bool {
    cli_bin.trim().to_ascii_lowercase().ends_with(".appimage")
}

/// Build the args array for `figcraft-cli mcp` (`Some(port)` attaches to the running desktop app;
/// `None` runs a standalone headless MCP session).
pub fn mcp_args(port: Option<u16>) -> Vec<String> {
    match port {
        Some(p) => vec!["mcp".into(), "--attach".into(), p.to_string()],
        None => vec!["mcp".into()],
    }
}

/// Build the args array for `cli_bin` (`FigCraft.AppImage` takes a leading `"cli"` subcommand).
pub fn mcp_args_for(cli_bin: &str, port: Option<u16>) -> Vec<String> {
    let mut args = if is_appimage(cli_bin) { vec!["cli".to_string()] } else { Vec::new() };
    args.extend(mcp_args(port));
    args
}

/// Format a copy-ready command line for attaching `cli_bin` to `port`, quoting paths with spaces.
pub fn attach_command(cli_bin: &str, port: u16) -> String {
    let args = mcp_args_for(cli_bin, Some(port)).join(" ");
    if cli_bin.contains(' ') { format!("\"{cli_bin}\" {args}") } else { format!("{cli_bin} {args}") }
}

/// Ready-to-copy configuration snippet for `kind`.
pub fn snippet(kind: HarnessKind, cli_bin: &str, port: Option<u16>) -> String {
    let args = mcp_args_for(cli_bin, port);
    match kind {
        HarnessKind::Antigravity | HarnessKind::ClaudeCode | HarnessKind::ClaudeDesktop => {
            let doc = json!({
                "mcpServers": {
                    "figcraft": {
                        "command": cli_bin,
                        "args": args,
                    }
                }
            });
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        }
        HarnessKind::Codex => {
            let quoted_args: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
            format!("[mcp_servers.figcraft]\ncommand = {:?}\nargs = [{}]\n", cli_bin, quoted_args.join(", "))
        }
    }
}

/// Client configuration is outside document roots but is read-only, regular and bounded.
#[allow(clippy::disallowed_methods)]
fn read_config(path: &Path) -> Option<String> {
    use std::io::Read as _;
    const MAX_CONFIG_BYTES: u64 = 8 << 20;
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut text).ok()?;
    (text.len() <= MAX_CONFIG_BYTES as usize).then_some(text)
}

/// Inspect a single harness's configuration at `path`.
pub fn inspect_at(kind: HarnessKind, path: &Path, cli_bin: &str, port: Option<u16>) -> HarnessStatus {
    let snip = snippet(kind, cli_bin, port);
    let content = read_config(path);
    let (configured, attached_port) = match (kind, content.as_deref()) {
        (HarnessKind::Antigravity | HarnessKind::ClaudeCode | HarnessKind::ClaudeDesktop, Some(text)) => inspect_json(text),
        (HarnessKind::Codex, Some(text)) => inspect_codex_toml(text),
        (_, None) => (false, None),
    };
    HarnessStatus {
        id: kind.id().to_string(),
        label: kind.label().to_string(),
        config_path: path.to_path_buf(),
        configured,
        attached_port,
        snippet: snip,
    }
}

/// Inspect a JSON MCP config (`mcpServers.figcraft`) and return `(configured, attached_port)`.
fn inspect_json(text: &str) -> (bool, Option<u16>) {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        return (false, None);
    };
    let Some(entry) = val.get("mcpServers").and_then(|s| s.get("figcraft")) else {
        return (false, None);
    };
    let has_cmd = entry.get("command").and_then(Value::as_str).is_some_and(|c| !c.is_empty());
    let mut attached_port = None;
    if let Some(args) = entry.get("args").and_then(Value::as_array) {
        let mut iter = args.iter().filter_map(Value::as_str);
        while let Some(arg) = iter.next() {
            if arg == "--attach"
                && let Some(p) = iter.next().and_then(|v| v.parse::<u16>().ok())
            {
                attached_port = Some(p);
            } else if let Some(rest) = arg.strip_prefix("--attach=")
                && let Ok(p) = rest.parse::<u16>()
            {
                attached_port = Some(p);
            }
        }
    }
    (has_cmd, attached_port)
}

/// Inspect a Codex TOML config (`[mcp_servers.figcraft]`) and return `(configured, attached_port)`.
fn inspect_codex_toml(text: &str) -> (bool, Option<u16>) {
    let Ok(document) = text.parse::<toml_edit::DocumentMut>() else { return (false, None) };
    let Some(entry) = document
        .get("mcp_servers")
        .and_then(toml_edit::Item::as_table_like)
        .and_then(|servers| servers.get("figcraft"))
        .and_then(toml_edit::Item::as_table_like)
    else {
        return (false, None);
    };
    let configured = entry.get("command").and_then(toml_edit::Item::as_str).is_some_and(|command| !command.trim().is_empty());
    let mut attached_port = None;
    if let Some(args) = entry.get("args").and_then(toml_edit::Item::as_array) {
        let mut args = args.iter().filter_map(toml_edit::Value::as_str);
        while let Some(arg) = args.next() {
            if arg == "--attach" {
                if let Some(port) = args.next().and_then(|port| port.parse::<u16>().ok()) {
                    attached_port = Some(port);
                }
            } else if let Some(port) = arg.strip_prefix("--attach=").and_then(|port| port.parse::<u16>().ok()) {
                attached_port = Some(port);
            }
        }
    }
    (configured, attached_port)
}

/// Inspect all supported harnesses using the current user's platform config paths.
pub fn inspect_all(cli_bin: &str, port: Option<u16>) -> Vec<HarnessStatus> {
    HarnessKind::ALL.into_iter().filter_map(|k| config_path(k).map(|p| inspect_at(k, &p, cli_bin, port))).collect()
}

mod install;
pub use install::{install, install_at, install_at_with_token_file, install_with_token_file, validate_token_file};
#[cfg(test)]
use install::{merge_codex_toml, merge_json_config};

/// Authenticated local-control snippet. The credential path is copied, never its contents.
pub fn snippet_with_token_file(kind: HarnessKind, cli_bin: &str, port: u16, path: &Path) -> String {
    let mut args = mcp_args_for(cli_bin, Some(port));
    args.extend(["--control-token-file".into(), path.to_string_lossy().into_owned()]);
    match kind {
        HarnessKind::Codex => {
            let quoted: Vec<String> = args.iter().map(|s| serde_json::to_string(s).unwrap_or_default()).collect();
            format!("[mcp_servers.figcraft]\ncommand = {}\nargs = [{}]\n", serde_json::to_string(cli_bin).unwrap_or_default(), quoted.join(", "))
        }
        _ => serde_json::to_string_pretty(&json!({"mcpServers":{"figcraft":{"command":cli_bin,"args":args}}})).unwrap_or_default(),
    }
}
/// POSIX-shell command suitable for the native macOS UI. Arguments are quoted individually.
pub fn attach_command_with_token_file(cli_bin: &str, port: u16, path: &Path) -> String {
    let mut args = vec![cli_bin.to_string()];
    args.extend(mcp_args_for(cli_bin, Some(port)));
    args.extend(["--control-token-file".into(), path.to_string_lossy().into_owned()]);
    args.iter().map(|arg| format!("'{}'", arg.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_json_preserving_other_servers_and_keys() {
        let existing = r#"{
  "theme": "dark",
  "mcpServers": {
    "other": {
      "command": "npx",
      "args": ["-y", "other-mcp"]
    }
  }
}"#;
        let merged = merge_json_config(Some(existing), "/usr/local/bin/figcraft-cli", Some(47800)).unwrap();
        let val: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(val["theme"], "dark");
        assert_eq!(val["mcpServers"]["other"]["command"], "npx");
        assert_eq!(val["mcpServers"]["figcraft"]["command"], "/usr/local/bin/figcraft-cli");
        assert_eq!(val["mcpServers"]["figcraft"]["args"], json!(["mcp", "--attach", "47800"]));
        let (ok, port) = inspect_json(&merged);
        assert!(ok);
        assert_eq!(port, Some(47800));
    }

    #[test]
    fn merges_codex_toml_preserving_existing_sections() {
        let existing = r#"model = "o3"

[mcp_servers.other]
command = "other"
args = ["serve"]
"#;
        let merged = merge_codex_toml(Some(existing), "/bin/figcraft-cli", Some(47800));
        assert!(merged.contains("model = \"o3\""));
        assert!(merged.contains("[mcp_servers.other]"));
        assert!(merged.contains("[mcp_servers.figcraft]"));
        let (ok, port) = inspect_codex_toml(&merged);
        assert!(ok);
        assert_eq!(port, Some(47800));

        // Updating again replaces the section in place without duplicating it.
        let updated = merge_codex_toml(Some(&merged), "/bin/figcraft-cli", Some(47999));
        assert_eq!(updated.matches("[mcp_servers.figcraft]").count(), 1);
        let (ok2, port2) = inspect_codex_toml(&updated);
        assert!(ok2);
        assert_eq!(port2, Some(47999));
    }

    #[test]
    fn codex_status_reads_valid_inline_dotted_quoted_and_multiline_toml() {
        for config in [
            r#"mcp_servers={figcraft={command="cli",args=["mcp","--attach","47800"]}}"#,
            "mcp_servers.figcraft.command = 'cli'\nmcp_servers.figcraft.args = ['mcp', '--attach=47800']\n",
            "[mcp_servers.'figcraft']\ncommand='cli'\nargs=[\n 'mcp',\n '--attach',\n '47800',\n]\n",
            "[mcp_servers]\nfigcraft={command='cli', args=['mcp','--attach','47800']}\n",
        ] {
            assert_eq!(inspect_codex_toml(config), (true, Some(47800)), "{config}");
        }
        for invalid in ["mcp_servers = [", "mcp_servers={figcraft={command=17}}", "mcp_servers={figcraft={command=''}}"] {
            assert_eq!(inspect_codex_toml(invalid), (false, None));
        }
    }

    #[test]
    fn resolves_config_paths_across_macos_linux_and_windows() {
        let home = Path::new("/home/alice");
        assert_eq!(
            config_path_for_os(HarnessKind::Antigravity, "linux", home, None, None, None),
            PathBuf::from("/home/alice/.gemini/antigravity/mcp_config.json")
        );
        assert_eq!(config_path_for_os(HarnessKind::ClaudeCode, "linux", home, None, None, None), PathBuf::from("/home/alice/.claude.json"));
        assert_eq!(
            config_path_for_os(HarnessKind::ClaudeDesktop, "linux", home, None, None, None),
            PathBuf::from("/home/alice/.config/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path_for_os(HarnessKind::ClaudeDesktop, "linux", home, None, Some(Path::new("/custom/xdg")), None),
            PathBuf::from("/custom/xdg/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path_for_os(HarnessKind::ClaudeDesktop, "macos", home, None, None, None),
            PathBuf::from("/home/alice/Library/Application Support/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path_for_os(HarnessKind::ClaudeDesktop, "windows", home, None, None, None),
            PathBuf::from("/home/alice/AppData/Roaming/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path_for_os(HarnessKind::ClaudeDesktop, "windows", home, Some(Path::new("C:/Users/Alice/AppData/Roaming")), None, None),
            PathBuf::from("C:/Users/Alice/AppData/Roaming/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path_for_os(HarnessKind::Codex, "linux", home, None, None, Some(Path::new("/custom/codex"))),
            PathBuf::from("/custom/codex/config.toml")
        );
    }

    #[test]
    fn supports_linux_appimage_and_windows_backslash_paths() {
        let appimage = "/opt/FigCraft-x86_64.AppImage";
        assert!(is_appimage(appimage));
        assert_eq!(mcp_args_for(appimage, Some(47800)), vec!["cli", "mcp", "--attach", "47800"]);
        assert_eq!(attach_command(appimage, 47800), "/opt/FigCraft-x86_64.AppImage cli mcp --attach 47800");
        let linux_json = merge_json_config(None, appimage, Some(47800)).unwrap();
        let val: Value = serde_json::from_str(&linux_json).unwrap();
        assert_eq!(val["mcpServers"]["figcraft"]["args"], json!(["cli", "mcp", "--attach", "47800"]));
        assert_eq!(inspect_json(&linux_json), (true, Some(47800)));

        let win_bin = r"C:\Program Files\FigCraft\figcraft-cli.exe";
        assert_eq!(attach_command(win_bin, 47800), r#""C:\Program Files\FigCraft\figcraft-cli.exe" mcp --attach 47800"#);
        let win_json = merge_json_config(None, win_bin, Some(47800)).unwrap();
        let win_val: Value = serde_json::from_str(&win_json).unwrap();
        assert_eq!(win_val["mcpServers"]["figcraft"]["command"], win_bin);
        assert_eq!(inspect_json(&win_json), (true, Some(47800)));

        let win_toml = merge_codex_toml(None, win_bin, Some(47800));
        let parsed = win_toml.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(parsed["mcp_servers"]["figcraft"]["command"].as_str(), Some(win_bin));
        assert_eq!(inspect_codex_toml(&win_toml), (true, Some(47800)));
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn install_at_preserves_symlinks_and_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("figcraft-harness-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dotfiles")).unwrap();

        let real_cfg = dir.join("dotfiles").join("claude.json");
        std::fs::write(&real_cfg, r#"{"theme":"dark"}"#).unwrap();
        std::fs::set_permissions(&real_cfg, std::fs::Permissions::from_mode(0o600)).unwrap();

        let link_cfg = dir.join(".claude.json");
        symlink("dotfiles/claude.json", &link_cfg).unwrap();

        let status = install_at(HarnessKind::ClaudeCode, &link_cfg, "/usr/local/bin/figcraft-cli", None).unwrap();
        assert!(status.configured);
        assert!(std::fs::symlink_metadata(&link_cfg).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::metadata(&real_cfg).unwrap().permissions().mode() & 0o777, 0o600);

        // Also test a dangling relative symlink.
        let dangling_link = dir.join("dangling.json");
        let dangling_target = dir.join("dotfiles").join("new_mcp.json");
        symlink("dotfiles/new_mcp.json", &dangling_link).unwrap();
        let status2 = install_at(HarnessKind::Antigravity, &dangling_link, "/usr/local/bin/figcraft-cli", None).unwrap();
        assert!(status2.configured);
        assert!(std::fs::symlink_metadata(&dangling_link).unwrap().file_type().is_symlink());
        assert!(dangling_target.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn harness_inspection_reads_only_bounded_regular_config_files() {
        let dir = std::env::temp_dir().join(format!("figcraft-harness-bound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("client.json");
        let config = r#"{"mcpServers":{"figcraft":{"command":"/synthetic/figcraft-cli","args":["mcp","--attach","47800"]}}}"#;
        std::fs::write(&path, config).unwrap();
        assert!(inspect_at(HarnessKind::ClaudeCode, &path, "/synthetic/figcraft-cli", Some(47800)).configured);
        let bytes = format!("{}{}", " ".repeat((8 << 20) + 1), config);
        std::fs::write(&path, bytes.as_bytes()).unwrap();
        assert!(
            !inspect_at(HarnessKind::ClaudeCode, &path, "/synthetic/figcraft-cli", Some(47800)).configured,
            "oversized global client metadata is not admitted to UI parsing"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes(), "inspection never changes client configuration");
        assert!(!inspect_at(HarnessKind::ClaudeCode, &dir, "/synthetic/figcraft-cli", Some(47800)).configured);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
