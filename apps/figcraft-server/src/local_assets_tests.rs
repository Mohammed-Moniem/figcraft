//! Local artifact service contracts: persistence, consent, no design-session joins and exact counters.
use crate::store::{FILES, Store};
use crate::test_util::{Raw, TempDir, config, session, snapshot};
use figcraft_collab::message::{AssetActions, AssetUsage, PluginReply, PluginReq, ServerMsg};
use figcraft_collab_net::{Invite, NetLink};
use figcraft_engine::Session;
use figcraft_engine::live::LiveLink;
use figcraft_plugin::bundle::Bundle;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn analytics_baselines_survive_usage_eviction_restart_undo_redo_and_damaged_disk() {
    let dir = TempDir::new("asset-ledger");
    let cfg = config(dir.path()).store();
    let mut store = Store::open(dir.path(), cfg.clone()).unwrap();
    let (file, ..) = store.publish("Library", &snapshot("Library")).unwrap();
    store.publish_library(&file.id, &crate::hardening_tests::archive(0, 0)).unwrap();
    let report = |store: &mut Store, inserted, detached| {
        store
            .report_asset_actions(
                &file.id,
                AssetActions { file: "Original".into(), inserted: vec![("brand".into(), inserted)], detached: vec![("brand".into(), detached)] },
            )
            .unwrap()
    };
    report(&mut store, 7, 2);
    // More than the live usage cap, with newer timestamps, really evicts the old usage row.
    store.report_assets(&file.id, AssetUsage { file: "Original".into(), styles: vec![("brand".into(), 1)], variables: vec![] }).unwrap();
    for i in 0..=200 {
        let name = format!("Other {i}");
        store.report_usage(&file.id, figcraft_collab::message::Usage { file: name.clone(), components: vec![] }).unwrap();
        store.report_assets(&file.id, AssetUsage { file: name, styles: vec![("brand".into(), 1)], variables: vec![] }).unwrap();
    }
    assert!(!store.library_assets(&file.id).unwrap().iter().any(|r| r.file == "Original"), "the original live usage row was actually evicted");
    drop(store);
    let mut store = Store::open(dir.path(), cfg.clone()).unwrap();
    for (i, d) in [(6, 1), (7, 2), (8, 3), (8, 3)] {
        report(&mut store, i, d);
    }
    let rows = store.library_asset_actions(&file.id).unwrap();
    assert_eq!(rows.iter().map(|w| w.insertions).sum::<u32>(), 8);
    assert_eq!(rows.iter().map(|w| w.detachments).sum::<u32>(), 3);
    drop(store);
    let ledger = dir.path().join(FILES).join(&file.id).join(crate::disk::ASSET_LEDGER);
    std::fs::write(&ledger, b"not-json").unwrap();
    let mut store = Store::open(dir.path(), cfg).unwrap();
    let error = store
        .report_asset_actions(&file.id, AssetActions { file: "Original".into(), inserted: vec![("brand".into(), 9)], detached: vec![] })
        .unwrap_err();
    assert!(error.contains("refusing to recount"), "{error}");
    assert_eq!(std::fs::read(ledger).unwrap(), b"not-json");
}

#[test]
fn directory_mode_refuses_network_and_sharing_settings() {
    let dir = TempDir::new("local-directory-config");
    let mut cfg = config(dir.path());
    cfg.directory_only = true;
    assert!(cfg.check().is_ok());
    for at in ["0.0.0.0:47900", "[::]:47900", "192.0.2.1:47900"] {
        cfg.listen = at.into();
        assert!(cfg.check().is_err(), "{at}");
    }
    cfg.listen = "[::ffff:127.0.0.1]:0".into();
    assert!(cfg.check().is_ok());
    cfg.open_publish = true;
    assert!(cfg.check().is_err());
    cfg.open_publish = false;
    cfg.public_url = Some("wss://example.invalid/".into());
    assert!(cfg.check().is_err());
}

#[test]
fn local_directory_publishes_libraries_and_plugins_without_joining_and_restarts() {
    let dir = TempDir::new("local-directory-e2e");
    let mut cfg = config(dir.path());
    cfg.directory_only = true;
    let mut server = crate::start(&cfg).unwrap();
    let addr = server.addr();
    let secret = crate::test_util::key(dir.path());
    let endpoint = format!("ws://{addr}/");
    let mut publisher = session("Local publisher");
    let rect = publisher.execute("shape.rectangle", &json!({"width":40,"height":30})).unwrap();
    publisher.execute("style.create", &json!({"type":"paint","name":"Brand","from":rect["id"]})).unwrap();
    publisher.execute("shape.rectangle", &json!({"width":5,"height":5})).unwrap();
    let published = publisher
        .execute("command.batch", &json!({"commands":[{"command":"library.publishToServer","params":{"server":endpoint,"key":secret}}]}))
        .unwrap()[0]
        .clone();
    let before = publisher.doc().unwrap().nodes().count();
    publisher.execute("edit.repeat", &json!({})).unwrap();
    assert_eq!(publisher.doc().unwrap().nodes().count(), before + 1, "Repeat retains the previous ordinary edit, never the key-bearing publication");
    let wrapped = format!(" \t{endpoint}?key={secret}\n ");
    for params in [json!({"server":wrapped}), json!({"commands":[{"command":"library.publishToServer","params":{"server":wrapped}}]})] {
        let mut replay = session("Whitespace credentials");
        let shape = replay.execute("shape.rectangle", &json!({"width":5,"height":5})).unwrap();
        replay.execute("style.create", &json!({"type":"paint","name":"Credential fixture","from":shape["id"]})).unwrap();
        replay.execute("shape.rectangle", &json!({"width":5,"height":5})).unwrap();
        let command = if params.get("commands").is_some() { "command.batch" } else { "library.publishToServer" };
        replay.execute(command, &params).unwrap();
        let before = replay.doc().unwrap().nodes().count();
        replay.execute("edit.repeat", &json!({})).unwrap();
        assert_eq!(replay.doc().unwrap().nodes().count(), before + 1, "whitespace-wrapped keyed URLs cannot replace Repeat");
        assert!(!serde_json::to_string(&replay.journal).unwrap().contains(&secret));
    }
    assert!(!publisher.live().syncing(), "a directory publication never joins a design session");
    assert!(publisher.server_link().is_some());
    let mut consumer = session("Local consumer");
    let linked = consumer.execute("library.add", &json!({"path":published["view"]})).unwrap();
    let target = consumer.execute("shape.rectangle", &json!({"width":10,"height":10})).unwrap();
    consumer.execute("library.applyStyle", &json!({"library":linked["id"],"style":"Brand","ids":[target["id"]]})).unwrap();
    consumer.execute("edit.duplicate", &json!({"ids":[target["id"]]})).unwrap();
    consumer.execute("style.detach", &json!({"ids":[target["id"]],"which":"fill"})).unwrap();
    let start = Instant::now();
    loop {
        consumer.refresh_server_libraries();
        let u = publisher.execute("library.usage", &json!({})).unwrap();
        if u["styles"][0]["insertions"] == 2 && u["styles"][0]["detachments"] == 1 {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{u}");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!serde_json::to_string(&publisher.prefs).unwrap().contains(&secret));
    assert!(!serde_json::to_string(&publisher.journal).unwrap().contains(&secret));
    // Per-file capabilities persist in its actual saved archive; the reusable key does not.
    let bytes = figcraft_format::save_bytes(publisher.doc().unwrap(), None).unwrap();
    let (doc, _) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(!serde_json::to_string(&doc).unwrap().contains(&secret));
    assert!(doc.libraries.publication.is_some());
    // Actual wire rejects a design Hello even when the artifact edit token is correct.
    let invite = Invite::parse(published["edit"].as_str().unwrap()).unwrap();
    let mut raw = Raw::connect(addr);
    raw.send(&figcraft_collab::Client::hello(&invite.file, &invite.token, "No joining", ""));
    assert!(matches!(raw.next(),ServerMsg::Refused{reason} if reason.contains("libraries and plugins only")));
    let mut net = NetLink::new(Arc::new(|| {}));
    let manifest = crate::directory_tests::manifest("org.example.local", "1.0.0");
    let bundle = Bundle { manifest, wasm: crate::directory_tests::wasm(), assets: vec![] }.encode();
    let keyed = format!("{endpoint}?key={secret}");
    assert!(matches!(net.plugins(&keyed, PluginReq::Publish { bundle }).unwrap(), PluginReply::Published(_)));
    let listing = consumer.execute("plugin.directory.list", &json!({"server":endpoint})).unwrap();
    let sha = listing["plugins"][0]["sha256"].clone();
    assert!(consumer.execute("plugin.directory.install", &json!({"id":"org.example.local","server":endpoint})).is_err());
    let installed =
        consumer.execute("plugin.directory.install", &json!({"id":"org.example.local","server":endpoint,"sha256":sha,"confirm":true})).unwrap();
    assert_eq!(installed["permissions"]["access"], "write");
    assert!(net.plugins(&format!("{endpoint}?key=wrong"), PluginReq::Unpublish { id: "org.example.local".into(), version: None }).is_err());
    server.stop();
    cfg.listen = addr.to_string();
    let mut server = crate::start(&cfg).unwrap();
    assert!(matches!(net.plugins(&endpoint,PluginReq::List{query:String::new()}).unwrap(),PluginReply::List(v) if v.len()==1));
    let mut reopened = Session::with_document(doc);
    reopened.live.install(Box::new(NetLink::new(Arc::new(|| {}))));
    let u = reopened.execute("library.usage", &json!({})).unwrap();
    assert_eq!(u["styles"][0]["insertions"], 2);
    assert_eq!(u["styles"][0]["detachments"], 1);
    server.stop();
}

#[test]
fn native_plaintext_key_is_not_sent_to_a_redirect_target() {
    use std::io::{Read, Write};
    let redirect = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let at = redirect.local_addr().unwrap();
    let destination = target.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = redirect.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
            if socket.read(&mut byte).unwrap_or(0) == 0 {
                break;
            }
            request.push(byte[0]);
        }
        write!(socket, "HTTP/1.1 307 Temporary Redirect\r\nLocation: ws://{destination}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        request
    });
    let endpoint = figcraft_collab_net::Endpoint::plain(at.to_string());
    let result = figcraft_collab_net::manage::plugins(
        &endpoint,
        "redirect-test-private-key",
        PluginReq::List { query: String::new() },
        &figcraft_collab_net::NetLimits::default(),
        Duration::from_secs(2),
    );
    assert!(result.is_err());
    let handshake = server.join().unwrap();
    assert!(!String::from_utf8_lossy(&handshake).contains("redirect-test-private-key"));
    assert!(matches!(target.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn full_or_unwritable_ledger_refuses_new_actions_without_losing_existing_totals() {
    let dir = TempDir::new("asset-ledger-bounds");
    let cfg = config(dir.path()).store();
    let mut store = Store::open(dir.path(), cfg.clone()).unwrap();
    let (file, ..) = store.publish("Library", &snapshot("Library")).unwrap();
    store.publish_library(&file.id, &crate::hardening_tests::archive(0, 0)).unwrap();
    drop(store);
    let folder = dir.path().join(FILES).join(&file.id);
    let ledger = folder.join(crate::disk::ASSET_LEDGER);
    let reporters = std::collections::BTreeMap::from_iter((0..1024).map(|i| (format!("Reporter {i}"), vec![("brand", 1, 0)])));
    std::fs::write(&ledger, serde_json::to_vec(&json!({"reporters":reporters,"weeks":[]})).unwrap()).unwrap();
    let mut store = Store::open(dir.path(), cfg).unwrap();
    let report = |name: &str, n| AssetActions { file: name.into(), inserted: vec![("brand".into(), n)], detached: vec![] };
    let error = store.report_asset_actions(&file.id, report("Overflow", 9)).unwrap_err();
    assert!(error.contains("capacity"));
    store.report_asset_actions(&file.id, report("Reporter 0", 2)).unwrap();
    assert_eq!(store.library_asset_actions(&file.id).unwrap().iter().map(|r| r.insertions).sum::<u32>(), 1);
    // A deterministic filesystem failure represents an unwritable ledger; acknowledgement must fail.
    let bytes = std::fs::read(&ledger).unwrap();
    std::fs::remove_file(&ledger).unwrap();
    std::fs::create_dir(&ledger).unwrap();
    assert!(store.report_asset_actions(&file.id, report("Reporter 0", 3)).is_err());
    assert_eq!(store.library_asset_actions(&file.id).unwrap().iter().map(|r| r.insertions).sum::<u32>(), 1);
    std::fs::remove_dir(&ledger).unwrap();
    std::fs::write(&ledger, bytes).unwrap();
}
