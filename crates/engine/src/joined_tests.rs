//! Remembered joins exercise real command dispatch and successful document synchronization.

use figcraft_doc::Document;
use serde_json::{Value, json};

use crate::Prefs;
use crate::live::{Access, Event, Role};
use crate::live_tests::{info, push, serve_as, session};
use std::sync::{Arc, Mutex};

fn complete(s: &mut crate::Session, script: &Arc<Mutex<crate::live_tests::Script>>, link: &str, name: &str, access: Access) {
    let (_, id, sync) = serve_as(&Document::new(name), access);
    let invite = figcraft_collab::invite::Invite::parse(link).unwrap();
    let mut welcome = info(Role::Guest, id);
    welcome.link = link.into();
    welcome.url = invite.url();
    welcome.address = invite.server.display();
    welcome.access = access;
    push(script, vec![Event::Welcome { info: welcome, peers: vec![] }, sync]);
    s.live_poll();
}

const LINK: &str = "figcraft://join?host=127.0.0.1:47900&file=f&token=synthetic-view-secret";

fn record() -> Value {
    json!({"address":"ws://127.0.0.1:47900/", "file":"f", "name":"Synthetic shared drawing", "link":LINK, "access":"view", "opened":123})
}

#[test]
fn joined_files_are_remembered_only_after_successful_sync_and_survive_preferences_reload() {
    let (mut s, script) = session();
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    assert!(serde_json::to_value(&s.prefs).unwrap()["joined_files"].as_array().is_none_or(Vec::is_empty));
    let (_, id, sync) = serve_as(&Document::new("Synthetic shared drawing"), Access::View);
    let mut welcome = info(Role::Guest, id);
    welcome.link = LINK.into();
    welcome.url = "ws://127.0.0.1:47900/?file=f&token=synthetic-view-secret".into();
    welcome.access = Access::View;
    push(&script, vec![Event::Welcome { info: welcome, peers: vec![] }]);
    s.live_poll();
    assert!(
        serde_json::to_value(&s.prefs).unwrap()["joined_files"].as_array().is_none_or(Vec::is_empty),
        "a welcome without the document is not a completed join"
    );
    push(&script, vec![sync]);
    s.live_poll();
    let saved = serde_json::to_value(&s.prefs).unwrap();
    let joined = saved["joined_files"].as_array().expect("a completed join must be remembered");
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0]["name"], "Synthetic shared drawing");
    assert_eq!(joined[0]["access"], "view");
    assert_eq!(joined[0]["link"], LINK);
    let expected = Value::Array(joined.clone());
    let restored: Prefs = serde_json::from_value(saved).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap()["joined_files"], expected);
}

#[test]
fn joined_file_reopens_its_exact_saved_capability_without_changing_the_default_server() {
    let (mut s, script) = session();
    s.prefs = serde_json::from_value(json!({"publish_server":"wss://default.invalid/", "joined_files":[record()]})).unwrap();
    let opened = s.execute("collab.openJoined", &json!({"address":"ws://127.0.0.1:47900/", "file":"f"})).unwrap();
    assert!(opened["session"].as_u64().is_some());
    assert_eq!(script.lock().unwrap().joined, [LINK]);
    assert_eq!(s.prefs.publish_server, "wss://default.invalid/");
    assert!(!opened.to_string().contains("synthetic-view-secret"));
}

#[test]
fn joined_open_reuses_the_same_pending_capability_without_marking_it_opened_before_sync() {
    let (mut s, script) = session();
    s.prefs = serde_json::from_value(json!({"joined_files":[record()]})).unwrap();
    let first = s.execute("collab.openJoined", &json!({"file":"f"})).unwrap();
    let second = s.execute("collab.openJoined", &json!({"file":"f"})).unwrap();
    assert_eq!(second["session"], first["session"], "repeated Open must reuse an identical pending capability");
    assert_eq!(script.lock().unwrap().joined, [LINK]);
    assert_eq!(s.prefs.joined_files[0].opened, 123);
    let (_, client, sync) = serve_as(&Document::new("Pending file"), Access::View);
    let mut welcome = info(Role::Guest, client);
    welcome.link = LINK.into();
    welcome.access = Access::View;
    push(&script, vec![Event::Welcome { info: welcome, peers: vec![] }]);
    s.live_poll();
    let welcomed = s.execute("collab.openJoined", &json!({"file":"f"})).unwrap();
    assert_eq!(welcomed["session"], first["session"]);
    assert!(welcomed["document"].is_null());
    assert_eq!(s.prefs.joined_files[0].opened, 123, "a transport Welcome is still not a completed document join");
    push(&script, vec![sync]);
    s.live_poll();
    assert!(s.live().syncing());
    assert_eq!(s.prefs.joined_files.len(), 1);
}

#[test]
fn forgetting_a_joined_file_is_local_and_durable() {
    let (mut s, script) = session();
    s.prefs = serde_json::from_value(json!({"joined_files":[record()]})).unwrap();
    let before = serde_json::to_value(s.doc().unwrap()).unwrap();
    let forgotten = s.execute("collab.forgetJoined", &json!({"address":"ws://127.0.0.1:47900/", "file":"f"})).unwrap();
    assert_eq!(forgotten["forgotten"], true);
    assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
    assert!(script.lock().unwrap().joined.is_empty());
    assert_eq!(script.lock().unwrap().left, 0);
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert!(saved["joined_files"].as_array().is_none_or(Vec::is_empty));
    assert!(!saved.to_string().contains("synthetic-view-secret"));
}

#[test]
fn joined_history_forget_wins_pending_welcome_and_reconnect_until_a_new_explicit_join() {
    let (mut s, script) = session();
    s.prefs = serde_json::from_value(json!({"joined_files":[record()]})).unwrap();
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    s.execute("collab.forgetJoined", &json!({"file":"f"})).unwrap();
    complete(&mut s, &script, LINK, "Forgotten while pending", Access::View);
    assert!(s.live().syncing(), "Forget must not cancel the requested join");
    assert!(s.prefs.joined_files.is_empty(), "an older pending welcome cannot recreate the entry");
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    complete(&mut s, &script, LINK, "Explicitly joined again", Access::View);
    assert_eq!(s.prefs.joined_files.len(), 1);
    let docs = s.docs.len();
    let uid = s.ds().unwrap().uid;
    s.execute("collab.forgetJoined", &json!({"file":"f"})).unwrap();
    push(&script, vec![Event::Status(crate::live::Status::Reconnecting { attempt: 1 })]);
    s.live_poll();
    complete(&mut s, &script, LINK, "A reconnect", Access::View);
    assert!(s.prefs.joined_files.is_empty());
    assert_eq!(s.docs.len(), docs);
    assert_eq!(s.ds().unwrap().uid, uid);
}

#[test]
fn joined_history_failed_and_cancelled_sessions_are_not_remembered() {
    for cancel in [false, true] {
        let (mut s, script) = session();
        let id = s.execute("collab.join", &json!({"link":LINK})).unwrap()["session"].as_u64().unwrap();
        if cancel {
            s.execute("collab.leave", &json!({"session":id})).unwrap();
            complete(&mut s, &script, LINK, "Cancelled", Access::Edit);
        } else {
            push(&script, vec![Event::Status(crate::live::Status::Failed { reason: "Synthetic refused join".into() })]);
            s.live_poll();
        }
        assert!(s.prefs.joined_files.is_empty());
    }
}

#[test]
fn joined_history_replaces_an_older_stronger_capability_and_focuses_only_the_saved_one() {
    let (mut s, script) = session();
    let edit = LINK.replace("synthetic-view-secret", "synthetic-edit-secret");
    for (link, access) in [(edit.as_str(), Access::Edit), (LINK, Access::Comment)] {
        s.execute("collab.join", &json!({"link":link})).unwrap();
        complete(&mut s, &script, link, "Newest capability", access);
    }
    assert_eq!(s.prefs.joined_files.len(), 1);
    assert_eq!(s.prefs.joined_files[0].access, Access::Comment);
    let count = script.lock().unwrap().joined.len();
    s.active = 0;
    let reopened = s.execute("collab.openJoined", &json!({"file":"f"})).unwrap();
    assert_eq!(reopened["alreadyOpen"], true);
    assert_eq!(s.active, 2);
    assert_eq!(script.lock().unwrap().joined.len(), count, "the exact saved capability already has a tab");
    assert_eq!(s.live().info.as_ref().unwrap().access, Access::Comment);
    assert!(s.active_read_only().is_some());
    assert_eq!(serde_json::to_value(&s.prefs).unwrap()["joined_files"][0]["link"], LINK);
}

#[test]
fn joined_history_uses_full_server_identity_and_preserves_case_sensitive_proxy_paths() {
    let addresses =
        ["wss://EXAMPLE.invalid/Alpha", "wss://example.invalid:443/alpha/", "ws://example.invalid/Alpha/", "wss://example.invalid:444/Alpha/"];
    let mut rows = Vec::new();
    for address in addresses {
        let mut row = record();
        row["address"] = json!(address);
        row["link"] = json!(format!("figcraft://join?server={address}&file=f&token=synthetic-view-secret"));
        rows.push(row);
    }
    let prefs: Prefs = serde_json::from_value(json!({"joined_files":rows})).unwrap();
    assert_eq!(prefs.joined_files.len(), 4, "path case, protocol and port are separate namespaces");
    assert_eq!(prefs.joined_files[0].address, "wss://example.invalid:443/Alpha/");
    assert!(crate::joined::same_endpoint("WSS://EXAMPLE.invalid/Alpha", "wss://example.invalid:443/Alpha/"));
    assert!(!crate::joined::same_endpoint("wss://example.invalid/Alpha/", "wss://example.invalid/alpha/"));
    let (mut s, _) = session();
    s.prefs = prefs;
    let error = s.execute("collab.openJoined", &json!({"file":"f"})).unwrap_err().to_string();
    assert!(error.contains("several servers"));
    assert!(!error.contains("synthetic-view-secret"));
}

#[test]
fn joined_preferences_skip_bad_records_and_bound_history_without_losing_other_preferences() {
    let before =
        Prefs { user_name: "Synthetic reader".into(), number_wheel: true, accent: "#123456".into(), pixel_grid_size: 0.125, ..Prefs::default() };
    let old = serde_json::to_value(&before).unwrap();
    assert_eq!(serde_json::from_value::<Prefs>(old.clone()).unwrap(), before, "old preferences need no migration");
    for junk in
        [Value::Null, json!("bad"), json!([{"link":"not a link"}]), json!([{"link":LINK,"address":"ws://user:password@example.invalid/","file":"f"}])]
    {
        let mut data = old.clone();
        data["joined_files"] = junk;
        assert_eq!(serde_json::from_value::<Prefs>(data).unwrap(), before);
    }
    let mut rows = vec![record(), record()];
    for n in 0..500 {
        let mut row = record();
        row["file"] = json!(format!("f{n}"));
        row["link"] = json!(format!("figcraft://join?host=127.0.0.1:47900&file=f{n}&token=synthetic-view-secret"));
        rows.push(row);
    }
    let mut data = old;
    data["joined_files"] = json!(rows);
    let restored: Prefs = serde_json::from_value(data).unwrap();
    assert_eq!(restored.joined_files.len(), crate::joined::MAX_JOINED_FILES);
    let mut others = restored;
    others.joined_files.clear();
    assert_eq!(others, before);
}

#[test]
fn joined_private_links_never_appear_in_debug_metadata_errors_or_saved_documents() {
    let (mut s, script) = session();
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    complete(&mut s, &script, LINK, "Synthetic nonsecret file", Access::View);
    assert!(!format!("{:?}", s.prefs).contains("synthetic-view-secret"));
    assert!(!s.execute("collab.joinedFiles", &json!({})).unwrap().to_string().contains("synthetic-view-secret"));
    assert!(!s.execute("collab.openJoined", &json!({"file":"synthetic-view-secret"})).unwrap_err().to_string().contains("synthetic-view-secret"));
    assert!(!serde_json::to_string(s.doc().unwrap()).unwrap().contains("synthetic-view-secret"));
}

#[test]
fn joined_history_retains_a_capability_when_an_owner_card_hides_it() {
    let (mut s, script) = session();
    let owner = json!({"server":"ws://127.0.0.1:47900/", "file":"f", "name":"Owned", "owner":"synthetic-owner-secret", "edit":LINK});
    s.prefs = serde_json::from_value(json!({"online_files":[owner]})).unwrap();
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    complete(&mut s, &script, LINK, "Joined and owned", Access::View);
    assert_eq!(s.prefs.joined_files.len(), 1, "the browser must not lose its only durable reopen capability");
    assert_eq!(s.prefs.online_files[0].owner, "synthetic-owner-secret");
    assert!(!format!("{:?}", s.prefs).contains("synthetic-owner-secret"));
}

#[test]
fn joined_history_remembers_a_server_key_publication_after_its_document_arrives() {
    let (mut s, script) = session();
    s.execute("collab.publish", &json!({"server":"ws://127.0.0.1:47900/?key=synthetic-server-secret"})).unwrap();
    push(
        &script,
        vec![Event::Published(crate::live::Published {
            server: "ws://127.0.0.1:47900/".into(),
            file: "f".into(),
            name: "Keyed file".into(),
            edit: LINK.into(),
            view: String::new(),
            owner: None,
            expires: None,
        })],
    );
    s.live_poll();
    assert!(s.prefs.joined_files.is_empty() && s.prefs.online_files.is_empty());
    complete(&mut s, &script, LINK, "Keyed file", Access::View);
    assert_eq!(s.prefs.joined_files.len(), 1);
    assert!(s.prefs.online_files.is_empty(), "this is a reopen capability, not an owner token");
    assert!(!serde_json::to_string(&s.prefs).unwrap().contains("synthetic-server-secret"));
}

#[test]
fn joined_history_new_success_is_mru_and_evicts_the_oldest_at_its_limit() {
    let (mut s, script) = session();
    let mut rows = Vec::new();
    for n in 0..crate::joined::MAX_JOINED_FILES {
        let mut row = record();
        row["file"] = json!(format!("f{n}"));
        row["link"] = json!(format!("figcraft://join?host=127.0.0.1:47900&file=f{n}&token=synthetic-view-secret"));
        rows.push(row);
    }
    s.prefs = serde_json::from_value(json!({"joined_files":rows})).unwrap();
    s.execute("collab.join", &json!({"link":LINK})).unwrap();
    complete(&mut s, &script, LINK, "Newest", Access::View);
    assert_eq!(s.prefs.joined_files.len(), crate::joined::MAX_JOINED_FILES);
    assert_eq!(s.prefs.joined_files[0].file, "f");
    assert_eq!(s.prefs.joined_files[0].name, "Newest");
    assert!(s.prefs.joined_files.iter().all(|f| f.file != "f99"));
    let again = LINK.replace("file=f&", "file=f50&");
    s.execute("collab.join", &json!({"link":again})).unwrap();
    complete(&mut s, &script, &again, "Reopened", Access::Comment);
    assert_eq!(s.prefs.joined_files.len(), crate::joined::MAX_JOINED_FILES);
    assert_eq!(s.prefs.joined_files[0].file, "f50");
    assert_eq!(s.prefs.joined_files.iter().filter(|f| f.file == "f50").count(), 1);
    assert_eq!(s.prefs.joined_files[0].access, Access::Comment);
}

#[test]
fn joined_history_forget_survives_failure_before_a_late_same_session_welcome() {
    for already_synced in [false, true] {
        let (mut s, script) = session();
        s.prefs = serde_json::from_value(json!({"joined_files":[record()]})).unwrap();
        s.execute("collab.join", &json!({"link":LINK})).unwrap();
        if already_synced {
            complete(&mut s, &script, LINK, "Initially joined", Access::View);
        }
        s.execute("collab.forgetJoined", &json!({"file":"f"})).unwrap();
        push(&script, vec![Event::Status(crate::live::Status::Failed { reason: "Synthetic connection failure".into() })]);
        s.live_poll();
        complete(&mut s, &script, LINK, "Late same-session welcome", Access::View);
        assert!(s.prefs.joined_files.is_empty(), "a failed old session must retain Forget suppression (already synced: {already_synced})");
        s.execute("collab.join", &json!({"link":LINK})).unwrap();
        complete(&mut s, &script, LINK, "New explicit join", Access::View);
        assert_eq!(s.prefs.joined_files.len(), 1);
    }
}

#[test]
fn joined_history_accepts_the_existing_invite_parsers_longest_endpoint_shape() {
    let address = format!("wss://{}/{}/", "a".repeat(250), "b".repeat(254));
    let link = format!("figcraft://join?server={address}&file=f&token=synthetic-view-secret");
    let invite = figcraft_collab::invite::Invite::parse(&link).unwrap();
    assert!(invite.endpoint().len() > 512 && invite.endpoint().len() < 1024);
    let (mut s, script) = session();
    s.execute("collab.join", &json!({"link":link})).unwrap();
    complete(&mut s, &script, &link, "Long valid endpoint", Access::View);
    assert_eq!(s.prefs.joined_files.len(), 1, "an accepted invite must remain reopenable at the parser's endpoint limits");
    assert_eq!(s.prefs.joined_files[0].address, invite.endpoint());
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(serde_json::from_value::<Prefs>(saved).unwrap().joined_files.len(), 1);
}

#[test]
fn joined_canonical_maximum_paths_reopen_from_empty_home_after_direct_and_published_joins() {
    for trailing_slash in [false, true] {
        let path = if trailing_slash { format!("/{}/", "b".repeat(254)) } else { format!("/{}", "b".repeat(255)) };
        assert_eq!(path.len(), 256);
        let server = format!("wss://{}{path}", "a".repeat(250));
        let invite = figcraft_collab::invite::Invite::parse(&format!("{server}?file=f&token=synthetic-view-secret")).unwrap();
        let canonical = invite.link();
        assert_eq!(invite.server.path.len(), if trailing_slash { 256 } else { 257 });
        for source in ["direct", "published", "remembered"] {
            let (mut s, script) = session();
            s.docs.clear();
            s.active = 0;
            match source {
                "direct" => {
                    s.execute("collab.join", &json!({"link":invite.url()})).unwrap();
                }
                "published" => {
                    s.execute("file.new", &json!({})).unwrap();
                    s.execute("collab.publish", &json!({"server":format!("{}?key=synthetic-server-secret",invite.endpoint())})).unwrap();
                    push(
                        &script,
                        vec![Event::Published(crate::live::Published {
                            server: invite.endpoint(),
                            file: "f".into(),
                            name: "Long published file".into(),
                            edit: canonical.clone(),
                            view: String::new(),
                            owner: None,
                            expires: None,
                        })],
                    );
                    s.live_poll();
                }
                _ => {
                    let mut saved = record();
                    saved["address"] = json!(invite.endpoint());
                    saved["link"] = json!(canonical);
                    s.prefs = serde_json::from_value(json!({"joined_files":[saved]})).unwrap();
                    s.execute("collab.openJoined", &json!({"file":"f"})).unwrap();
                }
            }
            complete(&mut s, &script, &canonical, "Maximum canonical endpoint", Access::View);
            assert_eq!(s.prefs.joined_files.len(), 1, "{source}, trailing slash: {trailing_slash}");
            s.execute("collab.leave", &json!({})).unwrap();
            s.docs.clear();
            s.active = 0;
            let opened = s.execute("collab.openJoined", &json!({"file":"f","address":invite.endpoint()})).unwrap();
            assert_eq!(opened["alreadyOpen"], false);
            assert_eq!(script.lock().unwrap().joined.last(), Some(&canonical));
            complete(&mut s, &script, &canonical, "Reopened from empty Home", Access::View);
            assert_eq!(s.docs.len(), 1);
            assert_eq!(s.prefs.joined_files.len(), 1);
            assert!(s.active_read_only().is_some());
        }
    }
}
