use super::*;
use crate::plugins_tests::install;
use crate::widgets_tests::{W, widget_manifest};

fn setup() -> (Session, NodeId) {
    let mut w = W::default();
    w.imports = r#"(import "figcraft" "widget_timer" (func $timer (param i32) (result i32)))"#.into();
    let stop = w.has(r#""target":"stop""#);
    let draw = w.render(&json!({"root":{"type":"frame","children":[{"type":"rectangle","target":"stop","width":10,"height":10}]}}));
    let wasm =
        w.build(&format!("(if {stop} (then (drop(call $timer(i32.const 0))) (return(i32.const 0)))) {draw} (drop(call $timer(i32.const 100)))"));
    let mut s = Session::new();
    let mut m = widget_manifest("org.test.timer-owner", "read");
    m["api"] = json!(6);
    install(&mut s, m, &wasm);
    let out = s.execute("widget.insert", &json!({"plugin":"org.test.timer-owner"})).unwrap();
    (s, NodeId(out["id"].as_u64().unwrap()))
}

#[test]
fn cancelled_timer_removes_stamp_and_failed_outer_group_restores_ownership() {
    let (mut s, w) = setup();
    let before = s.ds().unwrap().widget_timers[0].clone();
    let doc = s.doc().unwrap().clone();
    let result: Result<()> = s.grouped("failed outer", |s| {
        s.execute("widget.click", &json!({"id":w,"target":"stop"}))?;
        Err(bad("test", "rollback"))
    });
    assert!(result.is_err());
    let ds = s.ds().unwrap();
    assert_eq!(*ds.doc, doc);
    assert_eq!(ds.widget_timers.len(), 1);
    assert_eq!(ds.widget_timers[0].stamp, before.stamp);
    assert_eq!(ds.widget_timers[0].due, before.due);
    s.execute("widget.click", &json!({"id":w,"target":"stop"})).unwrap();
    assert!(next_timer(&s).is_none());
    assert!(!s.doc().unwrap().node(w).unwrap().plugin_data.contains_key(TIMER));
}

#[test]
fn timer_owners_do_not_depend_on_a_clock_and_entropy_failure_keeps_the_previous_owner() {
    let (mut s, w) = setup();
    let before = s.ds().unwrap().widget_timers[0].clone();
    let doc = s.doc().unwrap().clone();
    assert!(arm_using(&mut s, w, 100, || Err(bad("test", "entropy unavailable"))).is_err());
    assert_eq!(*s.doc().unwrap(), doc);
    assert_eq!(s.ds().unwrap().widget_timers[0].stamp, before.stamp);
    assert_eq!(s.ds().unwrap().widget_timers[0].due, before.due);
    let owners = (0..256).map(|_| stamp().unwrap()).collect::<std::collections::HashSet<_>>();
    assert_eq!(owners.len(), 256);
    assert!(owners.iter().all(|s| s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())));
}

#[test]
fn timer_failure_is_consumed_once_and_other_documents_do_not_fire_it() {
    let (mut s, w) = setup();
    s.ds_mut().unwrap().widget_timers[0].due = Instant::now();
    s.execute("file.new", &json!({"name":"Foreground"})).unwrap();
    assert!(next_timer(&s).is_none());
    assert!(run_timers(&mut s).is_empty());
    s.active = 0;
    assert!(next_timer(&s).is_some());
    s.prefs.plugins.clear();
    assert_eq!(run_timers(&mut s).len(), 1);
    assert!(run_timers(&mut s).is_empty());
    assert!(next_timer(&s).is_none());
    assert!(s.doc().unwrap().contains(w));
}

#[test]
fn saved_stamps_do_not_revive_and_timers_are_bounded_per_document_and_frame() {
    let (mut s, w) = setup();
    let restored = Session::with_document(s.doc().unwrap().clone());
    assert!(next_timer(&restored).is_none());
    assert!(arm(&mut s, w, 99).is_err());
    assert!(arm(&mut s, w, 3_600_001).is_err());
    let page = s.page().unwrap();
    let mut ids = vec![w];
    for _ in 1..MAX_TIMERS {
        let doc = s.ds_mut().unwrap().doc_mut();
        let id = doc.alloc_id();
        let mut n = Node::new(id, NodeKind::Frame, "timer", 10.0, 10.0);
        n.plugin_data.insert(PLUGIN.into(), "org.test.timer-owner".into());
        doc.insert(n, page, None).unwrap();
        ids.push(id);
        arm(&mut s, id, 100).unwrap();
    }
    let doc = s.ds_mut().unwrap().doc_mut();
    let extra = doc.alloc_id();
    let mut n = Node::new(extra, NodeKind::Frame, "extra", 10.0, 10.0);
    n.plugin_data.insert(PLUGIN.into(), "org.test.timer-owner".into());
    doc.insert(n, page, None).unwrap();
    assert!(arm(&mut s, extra, 100).is_err());
    assert_eq!(s.ds().unwrap().widget_timers.len(), MAX_TIMERS);
    for t in &mut s.ds_mut().unwrap().widget_timers {
        t.due = Instant::now();
    }
    s.prefs.plugins.clear();
    assert_eq!(run_timers(&mut s).len(), MAX_TICKS);
    assert_eq!(s.ds().unwrap().widget_timers.len(), MAX_TIMERS - MAX_TICKS);
}
