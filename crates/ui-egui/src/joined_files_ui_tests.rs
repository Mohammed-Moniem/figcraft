//! Home rows exercise the same private-capability commands as control/MCP callers.

use crate::live_tests::{Harness, has_text, text_center, texts};
use figcraft_engine::live::{Access, HostOptions, LiveLink, Presence, SessionInfo};
use serde_json::json;
use std::sync::{Arc, Mutex};

const LINK: &str = "figcraft://join?host=127.0.0.1:47900&file=f&token=synthetic-view-secret";
const NAME: &str = "Stored shared drawing";

struct Joins(Arc<Mutex<Vec<String>>>);
impl LiveLink for Joins {
    fn host(&mut self, _: u64, _: HostOptions) -> Result<SessionInfo, String> {
        Err("not in this test".into())
    }
    fn join(&mut self, _: u64, link: &str, _: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(link.into());
        Ok(())
    }
    fn leave(&mut self, _: u64) {}
    fn presence(&mut self, _: u64, _: &Presence) {}
    fn send(&mut self, _: u64, _: figcraft_engine::figcraft_collab::message::ClientMsg) {}
    fn poll(&mut self) -> Vec<(u64, figcraft_engine::live::Event)> {
        vec![]
    }
}

fn harness() -> (Harness, Arc<Mutex<Vec<String>>>) {
    let (mut h, _) = Harness::new();
    let joins = Arc::new(Mutex::new(Vec::new()));
    h.app.session.live.install(Box::new(Joins(joins.clone())));
    h.app.session.prefs = serde_json::from_value(json!({"publish_server":"", "joined_files":[{
        "address":"ws://127.0.0.1:47900/", "file":"f", "name":NAME, "link":LINK, "access":"view", "opened":1_791_800_000u64
    }]}))
    .unwrap();
    h.app.ui.home = true;
    h.app.ui.home_view.section = crate::home::Section::Online;
    h.frames(4);
    (h, joins)
}

#[test]
fn joined_home_row_shows_safe_metadata_and_opens_the_exact_saved_view_link() {
    let (mut h, joins) = harness();
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "Joined files") && has_text(&shapes, NAME));
    assert!(texts(&shapes).iter().any(|(text, _, _)| text.contains("ws://127.0.0.1:47900/") && text.contains("View only")));
    assert!(!format!("{shapes:?}").contains("synthetic-view-secret"));
    assert!(!h.app.inspect().to_string().contains("synthetic-view-secret"));
    assert!(joins.lock().unwrap().is_empty(), "drawing saved history must not connect");
    h.click(text_center(&shapes, NAME));
    assert_eq!(*joins.lock().unwrap(), [LINK]);
    assert!(h.app.ui.live.home_join.is_some());
    let first = h.app.ui.live.home_join;
    let pending = h.frames(1);
    h.click(text_center(&pending, NAME));
    assert_eq!(*joins.lock().unwrap(), [LINK], "repeated Open while connecting must not start a second session");
    assert_eq!(h.app.ui.live.home_join, first);
    let shapes = h.frames(2);
    assert!(has_text(&shapes, "Joining…"));
    assert_eq!(h.app.session.prefs.publish_server, "");
    assert_eq!(h.app.session.prefs.joined_files[0].access, Access::View);
}

#[test]
fn joined_home_menu_forgets_only_local_history_and_keeps_the_open_document() {
    let (mut h, joins) = harness();
    let shapes = h.frames(1);
    let before = serde_json::to_value(h.app.session.doc().unwrap()).unwrap();
    let history = h.app.session.ds().unwrap().history.undo.len();
    h.click(egui::pos2(280.0 + 720.0 - 18.0, text_center(&shapes, NAME).y));
    let shapes = h.frames(3);
    assert!(has_text(&shapes, "Forget on this computer"));
    h.click(text_center(&shapes, "Forget on this computer"));
    let shapes = h.frames(3);
    assert!(!has_text(&shapes, NAME));
    assert!(h.app.session.prefs.joined_files.is_empty());
    assert_eq!(serde_json::to_value(h.app.session.doc().unwrap()).unwrap(), before);
    assert_eq!(h.app.session.ds().unwrap().history.undo.len(), history);
    assert!(joins.lock().unwrap().is_empty());
}

#[test]
fn joined_home_hides_only_the_same_owned_endpoint_and_keeps_durable_browser_capabilities() {
    let (mut h, _) = harness();
    let owned = json!({"server":"ws://127.0.0.1:47900/", "file":"f", "name":"Owner card", "owner":"synthetic-owner-secret", "edit":LINK});
    h.app.session.prefs.online_files = serde_json::from_value(json!([owned])).unwrap();
    assert!(crate::home::joined::visible(&h.app.session).is_empty());
    assert_eq!(h.app.session.prefs.joined_files.len(), 1, "presentation does not delete private persisted history");
    h.app.session.prefs.online_files[0].server = "wss://127.0.0.1:47900/".into();
    assert_eq!(crate::home::joined::visible(&h.app.session).len(), 1, "different protocols are not deduplicated");
}

#[test]
fn browser_preference_errors_explain_failed_durability_without_echoing_payloads() {
    let reading = crate::browser_preferences_error(true);
    let writing = crate::browser_preferences_error(false);
    assert!(reading.contains("Couldn't read") && writing.contains("lost after reload"));
    assert!(!reading.contains("synthetic-view-secret") && !writing.contains("synthetic-view-secret"));
}

struct Kept(Arc<Mutex<Vec<figcraft_engine::Prefs>>>);
impl crate::Services for Kept {
    fn save_prefs(&mut self, prefs: &figcraft_engine::Prefs) {
        self.0.lock().unwrap().push(prefs.clone());
    }
}

#[test]
fn joined_success_and_forget_are_kept_by_logic_without_a_painted_frame() {
    use figcraft_engine::figcraft_collab::message::{ClientMsg, ServerMsg};
    use figcraft_engine::figcraft_collab::{Client, Limits, Outgoing, Server};
    use figcraft_engine::live::{Event, Role};
    let (mut h, _) = Harness::new();
    let kept = Arc::new(Mutex::new(Vec::new()));
    h.app.services = Box::new(Kept(kept.clone()));
    h.app.background_tick(&h.ctx);
    kept.lock().unwrap().clear();
    h.app.session.execute("collab.join", &json!({"link":LINK})).unwrap();
    let mut server = Server::new(figcraft_doc::Document::new("Hidden shared file"), "", Limits::default());
    let ClientMsg::Hello(hello) = Client::hello("f", "", "Synthetic visitor", "") else { panic!("no hello") };
    let (id, replies) = server.join_as(hello, Access::View).unwrap();
    let sync = replies
        .into_iter()
        .find_map(|reply| match reply {
            Outgoing::To(_, welcome @ ServerMsg::Welcome { .. }) => Some(welcome),
            _ => None,
        })
        .unwrap();
    h.send(vec![Event::Welcome {
        info: SessionInfo {
            role: Role::Guest,
            me: id,
            name: "Synthetic visitor".into(),
            color: "#14AE5C".into(),
            file_name: String::new(),
            pages: vec![],
            link: LINK.into(),
            url: "ws://127.0.0.1:47900/?file=f&token=synthetic-view-secret".into(),
            address: "127.0.0.1:47900".into(),
            network: false,
            access: Access::View,
        },
        peers: vec![],
    }]);
    h.app.background_tick(&h.ctx);
    assert!(kept.lock().unwrap().is_empty(), "a transport welcome alone must not save a join");
    h.send(vec![Event::Sync(sync)]);
    h.app.background_tick(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 1, "hidden browser logic must persist the completed document join");
    assert_eq!(kept.lock().unwrap()[0].joined_files[0].name, "Hidden shared file");
    assert!(!h.app.session.live.unsaved);
    h.app.session.execute("collab.forgetJoined", &json!({"file":"f"})).unwrap();
    h.app.background_tick(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 2);
    assert!(kept.lock().unwrap()[1].joined_files.is_empty());
    h.app.background_tick(&h.ctx);
    assert_eq!(kept.lock().unwrap().len(), 2, "no repeated write after flags were consumed");
}
