//! Explicit client installation. Only this writer changes client configs; inspection is read-only.
//! Tests call the same writer with temporary roots. No credential contents enter a configuration.
#![allow(clippy::disallowed_methods)]

use super::{HarnessKind, HarnessStatus, config_path, inspect_at, mcp_args_for};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: u64 = 8 << 20;

/// An attached client needs the actual published token file, never a fabricated filename.
/// Its contents remain private to the bridge process; only the path is installed.
pub fn validate_token_file(path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("control token file is unavailable: {e}"))?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > 4096 {
        return Err("control token path must name the current regular token file".into());
    }
    Ok(())
}

fn args(cli: &str, port: Option<u16>, token: Option<&Path>) -> Result<Vec<String>, String> {
    if cli.trim().is_empty() || cli.chars().any(char::is_control) {
        return Err("CLI command must be nonempty and contain no control characters".into());
    }
    let mut result = mcp_args_for(cli, port);
    if let Some(port) = port {
        if port == 0 {
            return Err("attached control port must be nonzero".into());
        }
        let token = token.ok_or("Start the local control server and supply its actual control token file before installing an attached client.")?;
        validate_token_file(token)?;
        let absolute = std::fs::canonicalize(token).map_err(|e| e.to_string())?;
        result.extend(["--control-token-file".into(), absolute.to_str().ok_or("control token path must be valid UTF-8")?.to_owned()]);
    } else if token.is_some() {
        return Err("a control token file requires an attached port".into());
    }
    Ok(result)
}

fn merge_json(existing: Option<&str>, cli: &str, args: &[String]) -> Result<String, String> {
    let mut root = match existing {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str::<Value>(text).map_err(|e| format!("existing config is not valid JSON: {e}"))?
        }
        _ => json!({}),
    };
    let object = root.as_object_mut().ok_or("existing config root is not a JSON object")?;
    let servers = object.entry("mcpServers").or_insert_with(|| json!({})).as_object_mut().ok_or("mcpServers must be a JSON object")?;
    servers.insert("figcraft".into(), json!({"command":cli,"args":args}));
    serde_json::to_string_pretty(&root)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|e| e.to_string())
}

fn merge_toml(existing: Option<&str>, cli: &str, args: &[String]) -> Result<String, String> {
    use toml_edit::{DocumentMut, Item, Table, value};
    let mut doc = existing.unwrap_or("").parse::<DocumentMut>().map_err(|error| {
        format!("existing config is not valid TOML (byte offset {}); repair it before installing", error.span().map_or(0, |span| span.start))
    })?;
    if !doc.contains_key("mcp_servers") {
        doc["mcp_servers"] = Item::Table(Table::new());
    }
    let servers = doc.get_mut("mcp_servers").and_then(Item::as_table_like_mut).ok_or("mcp_servers must be a TOML table")?;
    let mut entry = Table::new();
    entry["command"] = value(cli);
    let mut list = toml_edit::Array::new();
    for arg in args {
        list.push(arg.as_str());
    }
    entry["args"] = value(list);
    servers.insert("figcraft", Item::Table(entry));
    Ok(doc.to_string())
}

#[cfg(test)]
pub(super) fn merge_json_config(existing: Option<&str>, cli: &str, port: Option<u16>) -> Result<String, String> {
    merge_json(existing, cli, &mcp_args_for(cli, port))
}
#[cfg(test)]
pub(super) fn merge_codex_toml(existing: Option<&str>, cli: &str, port: Option<u16>) -> String {
    merge_toml(existing, cli, &mcp_args_for(cli, port)).unwrap_or_default()
}

/// Preserve final-component symlinks, including relative dangling links. A bounded chain avoids
/// cycles; atomic replacement must update the target rather than replacing the user's dotfile link.
fn resolve_target(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    for _ in 0..32 {
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = std::fs::read_link(&current).map_err(|e| e.to_string())?;
                current = if link.is_absolute() { link } else { current.parent().unwrap_or(Path::new(".")).join(link) };
            }
            Ok(_) => return Ok(current),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(current),
            Err(e) => return Err(format!("couldn't inspect configuration: {e}")),
        }
    }
    Err("too many symlink levels in configuration path".into())
}

struct Snapshot {
    bytes: Vec<u8>,
    meta: std::fs::Metadata,
}
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (a.dev(), a.ino(), a.mode()) != (b.dev(), b.ino(), b.mode()) {
            return false;
        }
    }
    a.len() == b.len() && a.modified().ok() == b.modified().ok() && a.permissions() == b.permissions()
}
fn read(path: &Path) -> Result<Option<Snapshot>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err("configuration must be a regular file of at most 8 MiB".into());
    }
    // NONBLOCK prevents a concurrent replacement by a FIFO from hanging the app; NOFOLLOW prevents
    // a final symlink replacement after resolution. The opened descriptor is checked again.
    #[cfg(unix)]
    let mut file = {
        use rustix::fs::{Mode, OFlags, open};
        std::fs::File::from(open(path, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK, Mode::empty()).map_err(|e| e.to_string())?)
    };
    #[cfg(not(unix))]
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || !same_file(&metadata, &opened) {
        return Err("configuration changed while it was opened; retry installation".into());
    }
    let mut bytes = Vec::new();
    (&mut file).take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let end = file.metadata().map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CONFIG_BYTES as usize || !same_file(&opened, &end) {
        return Err("configuration changed while it was read; retry installation".into());
    }
    Ok(Some(Snapshot { bytes, meta: end }))
}
fn unchanged(before: &Option<Snapshot>, after: &Option<Snapshot>) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(a), Some(b)) => a.bytes == b.bytes && same_file(&a.meta, &b.meta),
        _ => false,
    }
}
struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_at(
    kind: HarnessKind,
    path: &Path,
    cli: &str,
    port: Option<u16>,
    token: Option<&Path>,
    before_commit: impl FnOnce(),
) -> Result<HarnessStatus, String> {
    let arguments = args(cli, port, token)?;
    let target = resolve_target(path)?;
    let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| format!("couldn't create client config folder: {e}"))?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let lock = target.with_extension("figcraft-install.lock");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // A persistent advisory lock on macOS/Unix is released by the kernel on a crash. Never unlink
    // its inode: doing so would let a third installer lock a different inode while another waits.
    #[cfg(unix)]
    let _lock_file = {
        use rustix::fs::{FlockOperation, Mode, OFlags, flock, open};
        let fd = open(&lock, OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK, Mode::from_bits_truncate(0o600))
            .map_err(|e| format!("install lock unavailable: {e}"))?;
        let file = std::fs::File::from(fd);
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("install lock must be a regular file".into());
        }
        flock(&file, FlockOperation::NonBlockingLockExclusive).map_err(|e| format!("client configuration is busy: {e}"))?;
        file
    };
    #[cfg(not(unix))]
    let _lock_file = options.open(&lock).map_err(|e| format!("client configuration is busy or its install lock is unavailable: {e}"))?;
    #[cfg(not(unix))]
    let _lock_guard = RemoveOnDrop(lock);
    let original = read(&target)?;
    let existing = original.as_ref().map(|s| std::str::from_utf8(&s.bytes)).transpose().map_err(|_| "existing client configuration is not UTF-8")?;
    let updated = match kind {
        HarnessKind::Codex => merge_toml(existing, cli, &arguments)?,
        _ => merge_json(existing, cli, &arguments)?,
    };
    if updated.len() > MAX_CONFIG_BYTES as usize {
        return Err("updated configuration exceeds 8 MiB".into());
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temp = target.with_extension(format!("figcraft-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let mut file = options.open(&temp).map_err(|e| format!("couldn't create temporary client config: {e}"))?;
    let _temp_guard = RemoveOnDrop(temp.clone());
    file.write_all(updated.as_bytes()).map_err(|e| e.to_string())?;
    if let Some(snapshot) = &original {
        file.set_permissions(snapshot.meta.permissions()).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    before_commit();
    if resolve_target(path)? != target
        || std::fs::canonicalize(parent).map_err(|e| e.to_string())? != canonical_parent
        || !unchanged(&original, &read(&target)?)
    {
        return Err("client configuration changed during installation; nothing was overwritten, retry".into());
    }
    // Other FigCraft installs cooperate via the lock. Uncooperative external writers cannot be
    // atomically compared-and-renamed with portable filesystem APIs; this last check bounds that race.
    std::fs::rename(&temp, &target).map_err(|e| format!("couldn't commit client configuration: {e}"))?;
    let mut status = inspect_at(kind, path, cli, port);
    status.snippet = match kind {
        HarnessKind::Codex => merge_toml(None, cli, &arguments)?,
        _ => merge_json(None, cli, &arguments)?,
    };
    Ok(status)
}

/// Explicit headless installation. Attached installs must use `install_at_with_token_file`.
pub fn install_at(kind: HarnessKind, path: &Path, cli: &str, port: Option<u16>) -> Result<HarnessStatus, String> {
    write_at(kind, path, cli, port, None, || {})
}
pub fn install_at_with_token_file(kind: HarnessKind, path: &Path, cli: &str, port: u16, token: &Path) -> Result<HarnessStatus, String> {
    write_at(kind, path, cli, Some(port), Some(token), || {})
}
/// Explicitly install into the selected client's normal config path. No installer is run at startup.
pub fn install(kind: HarnessKind, cli: &str, port: Option<u16>) -> Result<HarnessStatus, String> {
    let path = config_path(kind).ok_or("client config path is unavailable")?;
    install_at(kind, &path, cli, port)
}
pub fn install_with_token_file(kind: HarnessKind, cli: &str, port: u16, token: &Path) -> Result<HarnessStatus, String> {
    let path = config_path(kind).ok_or("client config path is unavailable")?;
    install_at_with_token_file(kind, &path, cli, port, token)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(label: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "figcraft-explicit-installer-{}-{}-{label}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn production_installer_updates_all_clients_only_in_temporary_roots_with_token_paths() {
        let root = dir("clients");
        let token = root.join("actual.token");
        std::fs::write(&token, "synthetic-private-token-not-for-config").unwrap();
        for kind in HarnessKind::ALL {
            let path = super::super::config_path_in(kind, &root);
            let parent = path.parent().unwrap();
            std::fs::create_dir_all(parent).unwrap();
            let initial = if kind == HarnessKind::Codex {
                "# preserve comment\nmodel = 'synthetic'\n[mcp_servers.other]\ncommand = 'other'\n"
            } else {
                "{\"theme\":\"synthetic\",\"mcpServers\":{\"other\":{\"command\":\"other\"}}}"
            };
            std::fs::write(&path, initial).unwrap();
            for _ in 0..2 {
                let status = install_at_with_token_file(kind, &path, "/synthetic/CLI with spaces", 47800, &token).unwrap();
                assert!(status.configured);
                assert_eq!(status.attached_port, Some(47800));
            }
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains("other") && text.contains("synthetic"));
            assert!(text.contains("--control-token-file"));
            assert!(!text.contains("synthetic-private-token-not-for-config"));
            if kind == HarnessKind::Codex {
                assert!(text.contains("# preserve comment"));
                let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
                assert_eq!(doc["mcp_servers"]["figcraft"]["args"].as_array().unwrap().len(), 5);
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn production_installer_refuses_malformed_oversized_special_and_missing_token_without_writes() {
        let root = dir("refuse");
        let token = root.join("actual.token");
        std::fs::write(&token, "synthetic").unwrap();
        for (kind, value) in [
            (HarnessKind::Codex, "[broken"),
            (HarnessKind::Codex, "mcp_servers = 3"),
            (HarnessKind::ClaudeCode, "{broken"),
            (HarnessKind::ClaudeCode, "[]"),
            (HarnessKind::ClaudeCode, "{\"mcpServers\":4}"),
        ] {
            let path = root.join("config");
            std::fs::write(&path, value).unwrap();
            assert!(install_at_with_token_file(kind, &path, "cli", 47800, &token).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), value);
        }
        let path = root.join("huge");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        drop(file);
        assert!(install_at_with_token_file(HarnessKind::ClaudeCode, &path, "cli", 47800, &token).is_err());
        assert!(install_at_with_token_file(HarnessKind::Codex, &root, "cli", 47800, &token).is_err());
        let missing = root.join("never-created");
        assert!(install_at(HarnessKind::Codex, &missing, "cli", Some(47800)).is_err());
        assert!(!missing.exists());
        assert!(install_at_with_token_file(HarnessKind::Codex, &missing, "cli", 47800, &root.join("missing.token")).is_err());
        assert!(!missing.exists());
        assert!(std::fs::read_dir(&root).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn production_installer_detects_concurrent_edits_and_preserves_the_winner() {
        let root = dir("concurrent");
        let path = root.join("client.json");
        std::fs::write(&path, "{}").unwrap();
        let error = write_at(HarnessKind::ClaudeCode, &path, "cli", None, None, || std::fs::write(&path, "{\"changed\":true}").unwrap()).unwrap_err();
        assert!(error.contains("changed"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"changed\":true}");
        assert!(std::fs::read_dir(&root).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn production_installer_keeps_inline_toml_and_refuses_malformed_utf8() {
        let root = dir("inline");
        let path = root.join("config.toml");
        std::fs::write(&path, "# inline preserved\nmcp_servers = { other = { command = 'other' } }\n").unwrap();
        let token = root.join("synthetic.token");
        std::fs::write(&token, "synthetic-token-for-an-isolated-test").unwrap();
        let status = install_at_with_token_file(HarnessKind::Codex, &path, "cli", 47800, &token).unwrap();
        assert!(status.configured);
        assert_eq!(status.attached_port, Some(47800));
        let text = std::fs::read_to_string(&path).unwrap();
        let doc = text.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(doc["mcp_servers"]["other"]["command"].as_str(), Some("other"));
        assert_eq!(doc["mcp_servers"]["figcraft"]["command"].as_str(), Some("cli"));
        assert!(text.contains("# inline preserved"));
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(install_at(HarnessKind::Codex, &path, "cli", None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe]);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_toml_never_copies_configuration_values_into_errors() {
        let root = dir("private-parser-error");
        let path = root.join("config.toml");
        let sentinel = "SYNTHETIC_CONFIG_VALUE_MUST_STAY_PRIVATE";
        let malformed = format!("private_key = '{sentinel}\n");
        std::fs::write(&path, &malformed).unwrap();
        let error = install_at(HarnessKind::Codex, &path, "cli", None).unwrap_err();
        assert!(error.contains("not valid TOML") && error.contains("byte offset"));
        assert!(!error.contains(sentinel) && error.len() < 200);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn production_installer_reuses_crash_left_lock_and_refuses_a_concurrent_owner() {
        use rustix::fs::{FlockOperation, flock};
        use std::os::unix::fs::symlink;
        let root = dir("advisory");
        let path = root.join("config.json");
        let lock = path.with_extension("figcraft-install.lock");
        std::fs::write(&lock, "old crash marker").unwrap();
        let held = std::fs::File::open(&lock).unwrap();
        flock(&held, FlockOperation::NonBlockingLockExclusive).unwrap();
        assert!(install_at(HarnessKind::ClaudeCode, &path, "cli", None).unwrap_err().contains("busy"));
        assert!(!path.exists());
        drop(held);
        install_at(HarnessKind::ClaudeCode, &path, "cli", None).unwrap();
        assert!(path.is_file());
        let before = std::fs::read(&path).unwrap();
        std::fs::remove_file(&lock).unwrap();
        let victim = root.join("victim");
        std::fs::write(&victim, "preserve").unwrap();
        symlink(&victim, &lock).unwrap();
        assert!(install_at(HarnessKind::ClaudeCode, &path, "cli", None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_to_string(victim).unwrap(), "preserve");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn production_installer_preserves_permissions_and_symlinks_and_rejects_retarget() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = dir("links");
        let path = root.join("client.json");
        let target = root.join("actual.json");
        std::fs::write(&target, "{}").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        symlink("actual.json", &path).unwrap();
        install_at(HarnessKind::ClaudeCode, &path, "cli", None).unwrap();
        assert!(std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
        let fresh = root.join("fresh.json");
        install_at(HarnessKind::ClaudeCode, &fresh, "cli", None).unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o600);
        let other = root.join("other.json");
        std::fs::write(&other, "{\"other\":true}").unwrap();
        let before = std::fs::read(&target).unwrap();
        assert!(
            write_at(HarnessKind::ClaudeCode, &path, "cli", None, None, || {
                std::fs::remove_file(&path).unwrap();
                symlink("other.json", &path).unwrap();
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&target).unwrap(), before);
        assert_eq!(std::fs::read_to_string(other).unwrap(), "{\"other\":true}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
