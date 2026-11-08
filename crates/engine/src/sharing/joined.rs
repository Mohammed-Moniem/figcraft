//! Local reopen history for files joined with a link or code. Capabilities are passwords: only
//! preferences serialize them; Debug, commands and Home metadata leave them out. Loading history
//! never connects to a server. Owned-file credentials remain in their existing separate records.

use figcraft_collab::invite::{Endpoint, Invite};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::cmd::CommandSpec;
use crate::live::{Access, Role, SessionInfo, start_join};
use crate::params::{req_string, string};
use crate::{Result, Session, bad, spec};

/// Most joined files remembered, newest first.
pub const MAX_JOINED_FILES: usize = 100;
// Covers the existing parser's largest host plus proxy path, without shrinking its contract.
const MAX_ADDRESS: usize = 1024;

/// A successfully joined file. `link` is deliberately private; opening it uses a command.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JoinedFile {
    pub address: String,
    pub file: String,
    pub name: String,
    link: String,
    #[serde(default = "view_access")]
    pub access: Access,
    pub opened: u64,
}

fn view_access() -> Access {
    Access::View
}

impl std::fmt::Debug for JoinedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinedFile")
            .field("address", &self.address)
            .field("file", &self.file)
            .field("name", &self.name)
            .field("access", &self.access)
            .field("opened", &self.opened)
            .finish_non_exhaustive()
    }
}

fn endpoint_key(mut endpoint: Endpoint) -> String {
    endpoint.host.make_ascii_lowercase();
    // The parser supplies a scheme's default port and closing slash. Only hosts are case
    // insensitive: reverse-proxy paths keep their exact case and namespace.
    endpoint.url()
}

fn address_key(address: &str) -> Option<String> {
    if address.len() > MAX_ADDRESS {
        return None;
    }
    let (endpoint, _) = Endpoint::parse(address).ok()?;
    Some(endpoint_key(endpoint))
}

/// Compare complete server endpoints (including protocol, port and case-sensitive proxy path).
pub fn same_endpoint(a: &str, b: &str) -> bool {
    address_key(a).zip(address_key(b)).is_some_and(|(a, b)| a == b)
}

pub(crate) fn identity(link: &str) -> Option<(String, String)> {
    let invite = Invite::parse(link).ok()?;
    Some((endpoint_key(invite.server), invite.file))
}

impl JoinedFile {
    fn clean(&self) -> Option<Self> {
        if self.link.len() > 2048 || self.address.len() > MAX_ADDRESS || self.file.len() > 64 {
            return None;
        }
        let (address, file) = identity(&self.link)?;
        if self.file != file || address_key(&self.address)? != address {
            return None;
        }
        Some(Self {
            address,
            file,
            name: self.name.trim().chars().filter(|c| !c.is_control()).take(200).collect(),
            link: self.link.clone(),
            access: self.access,
            opened: self.opened,
        })
    }
}

/// A bounded, validated snapshot for selective platform storage. The first usable record for an
/// endpoint/file wins, so reloading preserves MRU order and cannot restore duplicate cards.
pub fn persisted(files: &[JoinedFile]) -> Vec<JoinedFile> {
    let mut out: Vec<JoinedFile> = Vec::new();
    for file in files.iter().take(4 * MAX_JOINED_FILES).filter_map(JoinedFile::clean) {
        if !out.iter().any(|f| f.address == file.address && f.file == file.file) {
            out.push(file);
            if out.len() == MAX_JOINED_FILES {
                break;
            }
        }
    }
    out
}

/// Junk in this field does not discard unrelated preferences or owner credentials.
pub(crate) fn read<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<JoinedFile>, D::Error> {
    let value = Value::deserialize(d).unwrap_or(Value::Null);
    let entries = value.as_array().map(Vec::as_slice).unwrap_or_default();
    let files: Vec<JoinedFile> = entries
        .iter()
        .take(4 * MAX_JOINED_FILES)
        .filter(|e| {
            [("link", 2048), ("address", MAX_ADDRESS), ("file", 64), ("name", 4096)]
                .iter()
                .all(|(key, max)| e.get(key).and_then(Value::as_str).is_none_or(|s| s.len() <= *max))
        })
        .filter_map(|e| serde_json::from_value(e.clone()).ok())
        .collect();
    Ok(persisted(&files))
}

fn now() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Session {
    /// First document welcome only. Retain Forget even if a failure reset this same session
    /// before a late callback; a newly initiated session has an empty suppression set.
    pub(crate) fn joined_welcome(&mut self, id: crate::live::SessionId) {
        let Some(live) = self.live.get(id).filter(|l| !l.joined_forget_all) else { return };
        let Some(mut info) = live.info.clone() else { return };
        let Some(key) = identity(&info.link) else { return };
        if live.joined_forgets.contains(&key) {
            return;
        }
        if let Some(client) = live.client() {
            info.access = client.access();
        }
        self.joined_record(&info);
    }

    /// Only the first successful document welcome calls this. Reconnecting an existing client
    /// never recreates forgotten history. Keep the newest negotiated capability, even when an
    /// older entry allowed more access; never synthesize an edit link from a view/comment link.
    pub(crate) fn joined_record(&mut self, info: &SessionInfo) {
        if info.role != Role::Guest {
            return;
        }
        let Some((address, file)) = identity(&info.link) else { return };
        let record = JoinedFile { address, file, name: info.file_name.clone(), link: info.link.clone(), access: info.access, opened: now() };
        let Some(record) = record.clean() else { return };
        self.prefs.joined_files.retain(|f| !(f.address == record.address && f.file == record.file));
        self.prefs.joined_files.insert(0, record);
        self.prefs.joined_files.truncate(MAX_JOINED_FILES);
        self.live.unsaved = true;
    }

    fn joined_named(&self, cmd: &str, params: &Value) -> Result<usize> {
        let file = req_string(cmd, params, "file")?;
        let address = string(params, "address");
        let key = match address {
            Some(address) => Some(address_key(&address).ok_or_else(|| bad(cmd, "`address` must name a server endpoint"))?),
            None => None,
        };
        let mut hits = self.prefs.joined_files.iter().enumerate().filter(|(_, f)| f.file == file && key.as_ref().is_none_or(|k| k == &f.address));
        match (hits.next(), hits.next()) {
            (Some((index, _)), None) => Ok(index),
            (Some(_), Some(_)) => Err(bad(cmd, "several servers have that file id: give `address` too")),
            _ => Err(bad(cmd, "no joined file with that id is remembered here (`collab.joinedFiles` lists them)")),
        }
    }
}

fn files(s: &mut Session, _: &Value) -> Result<Value> {
    let files: Vec<Value> = s
        .prefs
        .joined_files
        .iter()
        .map(|f| json!({"address":f.address, "file":f.file, "name":f.name, "access":f.access, "opened":f.opened}))
        .collect();
    Ok(json!({"files":files, "max":MAX_JOINED_FILES}))
}

fn open(s: &mut Session, params: &Value) -> Result<Value> {
    const CMD: &str = "collab.openJoined";
    let index = s.joined_named(CMD, params)?;
    let Some(file) = s.prefs.joined_files.get(index).and_then(JoinedFile::clean) else { return Err(bad(CMD, "this remembered link is invalid")) };
    // Focus only an active session using this exact capability: a view link must not silently
    // focus a different, stronger link's editable session.
    let already = s
        .live
        .iter()
        .find(|l| {
            l.status.active() && (l.joining_link.as_deref() == Some(file.link.as_str()) || l.info.as_ref().is_some_and(|info| info.link == file.link))
        })
        .map(|l| (l.id, l.status.id(), l.doc_uid, l.syncing()));
    if let Some((id, status, uid, synced)) = already {
        let document = uid.and_then(|uid| s.docs.iter().position(|d| d.uid == uid));
        if let Some(document) = document {
            s.active = document;
        }
        if synced {
            if let Some(f) = s.prefs.joined_files.get_mut(index) {
                f.opened = now();
            }
            let f = s.prefs.joined_files.remove(index);
            s.prefs.joined_files.insert(0, f);
            s.live.unsaved = true;
        }
        return Ok(json!({"session":id, "status":status, "document":document, "alreadyOpen":true}));
    }
    let id = start_join(s, CMD, &file.link, None, None, None)?;
    Ok(json!({"session":id, "status":"connecting", "alreadyOpen":false}))
}

fn forget(s: &mut Session, params: &Value) -> Result<Value> {
    const CMD: &str = "collab.forgetJoined";
    let index = s.joined_named(CMD, params)?;
    let file = s.prefs.joined_files.remove(index);
    for live in s.live.sessions_mut() {
        if live.joined_forgets.len() >= MAX_JOINED_FILES {
            // A stalled join keeps running, but stops recording after too many local forgets.
            live.joined_forget_all = true;
        } else {
            live.joined_forgets.insert((file.address.clone(), file.file.clone()));
        }
    }
    s.live.unsaved = true;
    Ok(json!({"forgotten":true}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        spec!(
            "collab.joinedFiles",
            "Joined files",
            "{} → {files: [{address, file, name, access, opened}], max}: local history only; never connects or returns invite secrets",
            false,
            files
        ),
        spec!(
            "collab.openJoined",
            "Open joined file",
            "{file, address? (required when several servers have that id)} → {session, status, document?, alreadyOpen}: rejoin with the private saved capability, or focus its open tab",
            false,
            open
        ),
        spec!(
            "collab.forgetJoined",
            "Forget joined file",
            "{file, address?} → {forgotten}: remove this local history entry; leave the server link and active document alone",
            false,
            forget
        ),
    ]
}
