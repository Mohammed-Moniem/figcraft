//! Widgets in a presented prototype (plugin API 4): the part under the pointer shows its hover
//! style, a timer runs on the player's clock, and text is typed into a field over the widget's
//! text, all on the prototype's own copy: the file, its history and its timers stay as they are.

use egui::{Event, Key, PointerButton, Pos2};
use figcraft_doc::NodeId;
use serde_json::json;

use crate::present_tests::Ui as Player;
use crate::widget_ui_tests::{NOTE, fill, key, part, session, state};

#[test]
fn a_presented_widget_takes_hover_typing_and_timers_on_its_own_copy() {
    let mut s = session();
    let a = NodeId(s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 300, "height": 300, "name": "A"})).unwrap()["id"].as_u64().unwrap());
    let w = NodeId(s.execute("widget.insert", &json!({"plugin": NOTE, "x": 40, "y": 40})).unwrap()["id"].as_u64().unwrap());
    s.execute("node.reparent", &json!({"ids": [w], "parent": a})).unwrap();
    let mut u = Player::new(s);
    u.run("proto.present", json!({"start": a}));
    u.frames(2, Vec::new);
    let shown = |u: &Player, name: &str| {
        let p = u.app.ui.present.as_ref().unwrap();
        let (doc, (_, place, _)) = (p.doc.as_ref().unwrap(), p.shown.unwrap());
        (part(doc, w, name), place.rect(doc.world_bounds(part(doc, w, name)).unwrap()))
    };
    let player = |u: &Player| state(u.app.ui.present.as_ref().unwrap().doc.as_ref().unwrap(), w);
    let press = |u: &mut Player, name: &str| {
        let at: Pos2 = shown(u, name).1.center();
        let b = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        u.frame(vec![Event::PointerMoved(at)]);
        u.frame(vec![b(true)]);
        u.frame(vec![b(false)]);
        u.frames(2, Vec::new);
    };
    // Hover: the part shows its style in the prototype; nobody is called.
    let (go, at) = shown(&u, "Go");
    u.frame(vec![Event::PointerMoved(at.center())]);
    u.frame(vec![]);
    assert_eq!(fill(u.app.ui.present.as_ref().unwrap().doc.as_ref().unwrap(), go), "#00FF00");
    assert_eq!(player(&u), r#"{"n":1}"#);
    u.frame(vec![Event::PointerMoved(at.center() + egui::vec2(0.0, 200.0))]);
    u.frame(vec![]);
    assert_eq!(fill(u.app.ui.present.as_ref().unwrap().doc.as_ref().unwrap(), go), "#0D99FF");
    // A timer runs on the player's clock: asked for in 100 ms, it fires a few frames later.
    press(&mut u, "Soon");
    assert_eq!(player(&u), r#"{"n":5}"#);
    u.frames(12, Vec::new);
    assert_eq!(player(&u), r#"{"n":3}"#, "the timer fired in the prototype");
    // Typing: a click on the text opens a field over it; Enter commits to the plugin.
    press(&mut u, "Title");
    u.frame(vec![Event::Text("Hey".into())]);
    u.frame(key(Key::Enter));
    u.frames(3, Vec::new);
    assert_eq!(player(&u), r#"{"n":4}"#, "the typed text reached the plugin");
    // The file, its history and its timers are as they were, and restarting starts over.
    let ds = u.app.session.ds().unwrap();
    assert_eq!((state(&ds.doc, w), ds.history.undo.len()), (r#"{"n":1}"#.to_string(), 3));
    assert!(figcraft_engine::widgets::next_timer(&u.app.session).is_none() && u.app.ui.toast.is_none(), "{:?}", u.app.ui.toast);
    press(&mut u, "Soon");
    u.frame(key(Key::R));
    u.frames(12, Vec::new);
    assert_eq!(player(&u), r#"{"n":1}"#, "restarting forgets the prototype's timers");
}

#[test]
fn a_click_that_rearms_a_due_player_timer_does_not_fire_the_old_owner_afterwards() {
    let mut s = session();
    let a = NodeId(s.execute("frame.create", &json!({"width":300,"height":300})).unwrap()["id"].as_u64().unwrap());
    let w = NodeId(s.execute("widget.insert", &json!({"plugin":NOTE})).unwrap()["id"].as_u64().unwrap());
    s.execute("node.reparent", &json!({"ids":[w],"parent":a})).unwrap();
    let mut u = Player::new(s);
    u.run("proto.present", json!({"start":a}));
    u.frames(2, Vec::new);
    let p = u.app.ui.present.as_mut().unwrap();
    p.widget_events.push(("widget.click", json!({"id":w,"target":"soon"})));
    assert!(p.run_widgets(&u.app.session, 10.0).is_none());
    assert_eq!(p.widget_timers.len(), 1);
    p.widget_events.push(("widget.click", json!({"id":w,"target":"go"})));
    assert!(p.run_widgets(&u.app.session, 10.2).is_none());
    assert_eq!(state(p.design.as_ref().unwrap(), w), r#"{"n":2}"#);
    assert_eq!(p.widget_timers.len(), 1);
    let replacement = p.widget_timers[0];
    assert_eq!(replacement.0, w);
    assert!((70.1..=70.2).contains(&replacement.1), "the new 60-second timer remains; wall time elapsed inside the real callback is deducted");
    let title = part(p.design.as_ref().unwrap(), w, "Title");
    p.widget_input = Some((title, false));
    assert!(p.run_widgets(&u.app.session, 80.0).is_none());
    assert_eq!(state(p.design.as_ref().unwrap(), w), r#"{"n":2}"#, "due ticks wait while the player owns a text field");
    assert_eq!(p.widget_timers, vec![replacement]);
    p.widget_input = None;
    assert!(p.run_widgets(&u.app.session, 80.0).is_none());
    assert_eq!(state(p.design.as_ref().unwrap(), w), r#"{"n":3}"#);
    assert!(p.widget_timers.is_empty());
    assert_eq!(state(u.app.session.doc().unwrap(), w), r#"{"n":1}"#);
}
