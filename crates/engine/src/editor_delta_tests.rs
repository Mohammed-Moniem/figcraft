//! Behavioral boundaries of the adapted editor PRs, exercised through the shared command API.
use crate::Session;
use figcraft_color::Color;
use figcraft_doc::NodeId;
use figcraft_doc::anim::motion::{self, AnimProperty, AnimValue, Clip, ClipKind, ClipMode, Ease, NodeMotion};
use serde_json::{Value, json};

fn run(s: &mut Session, c: &str, p: Value) -> Value {
    s.execute(c, &p).unwrap_or_else(|e| panic!("{c}: {e}"))
}
fn rect(s: &mut Session) -> NodeId {
    NodeId(run(s, "shape.rectangle", json!({"x":0,"y":0,"width":100,"height":80}))["id"].as_u64().unwrap())
}

#[test]
fn motion_commands_move_ease_hide_delete_and_undo_keys() {
    let mut s = Session::new();
    let id = rect(&mut s);
    for (time, value) in [(0, 1.0), (500, 0.5), (1000, 0.0)] {
        run(&mut s, "motion.keyframe", json!({"ids":[id],"property":"Opacity","time":time,"value":AnimValue::Scalar(value)}));
    }
    let moved = run(&mut s, "motion.keyframeMove", json!({"node":id,"property":"Opacity","time":500,"to":2000}));
    assert_eq!(moved["time"], 999, "a key cannot overwrite its neighbor");
    run(&mut s, "motion.keyframeEase", json!({"node":id,"property":"Opacity","time":0,"ease":Ease::Hold}));
    assert_eq!(motion::evaluate(s.doc().unwrap(), s.page().unwrap(), 250.0).node(id).unwrap().opacity, 1.0);
    run(&mut s, "motion.track", json!({"node":id,"property":"Opacity","visible":false}));
    assert!(!s.doc().unwrap().node(id).unwrap().motion.track(AnimProperty::Opacity).unwrap().visible);
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().node(id).unwrap().motion.track(AnimProperty::Opacity).unwrap().visible);
    assert_eq!(run(&mut s, "motion.keyframeDelete", json!({"node":id,"property":"Opacity","time":999}))["removed"], true);
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().node(id).unwrap().motion.track(AnimProperty::Opacity).unwrap().key_at(999).is_some());
}

#[test]
fn motion_clips_bulk_replace_and_native_file_roundtrip() {
    let mut s = Session::new();
    let id = rect(&mut s);
    for kind in ClipKind::ALL {
        run(&mut s, "motion.clip", json!({"ids":[id],"kind":kind.name(),"mode":"custom","time":200}));
    }
    let mut clip = s.doc().unwrap().node(id).unwrap().motion.clips[0].clone();
    clip.delay_ms = 400;
    clip.duration_ms = 1200;
    clip.visible = false;
    run(&mut s, "motion.clipUpdate", json!({"node":id,"id":clip.id,"clip":clip}));
    assert_eq!(s.doc().unwrap().node(id).unwrap().motion.clips[0].delay_ms, 400);
    run(&mut s, "motion.timeline", json!({"duration":6000,"fps":24,"loop":true}));
    run(&mut s, "motion.keyframe", json!({"ids":[id],"property":"Position","time":100,"value":AnimValue::Vec2(figcraft_geom::Vec2::new(12.0,34.0))}));
    let saved = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (loaded, _) = figcraft_format::load_bytes(&saved).unwrap();
    assert_eq!(loaded.node(id).unwrap().motion, s.doc().unwrap().node(id).unwrap().motion);
    assert_eq!(loaded.timelines, s.doc().unwrap().timelines);
    let motion = loaded.node(id).unwrap().motion.clone();
    run(&mut s, "motion.replace", json!({"node":id,"motion":NodeMotion::default()}));
    assert!(s.doc().unwrap().node(id).unwrap().motion.is_empty());
    run(&mut s, "motion.replace", json!({"node":id,"motion":motion}));
    run(&mut s, "motion.clipDelete", json!({"node":id,"id":clip.id}));
    assert_eq!(s.doc().unwrap().node(id).unwrap().motion.clips.len(), ClipKind::ALL.len() - 1);
}

#[test]
fn malformed_motion_optional_values_and_wrong_pages_fail_without_effects() {
    let mut s = Session::new();
    let id = rect(&mut s);
    run(&mut s, "motion.keyframe", json!({"ids":[id],"property":"Opacity","value":AnimValue::Scalar(0.5)}));
    for (cmd, p) in [
        ("motion.keyframe", json!({"ids":[id],"property":"Opacity","time":"later"})),
        ("motion.keyframe", json!({"ids":[id],"property":"Opacity","all":"yes"})),
        ("motion.keyframe", json!({"ids":[id],"property":"Position","value":AnimValue::Scalar(4.0)})),
        ("motion.clip", json!({"ids":[id],"kind":"Opacity","mode":true})),
        ("motion.clip", json!({"ids":[id],"kind":"Opacity","time":[]})),
        ("motion.track", json!({"node":id,"property":"Opacity","visible":"false"})),
        ("motion.timeline", json!({"page":id,"duration":4000})),
        ("motion.timeline", json!({"page":999999,"duration":4000})),
        ("motion.timeline", json!({"duration":"4000"})),
        ("motion.timeline", json!({"fps":"24"})),
        ("motion.timeline", json!({"loop":1})),
        ("motion.clipDelete", json!({"node":id,"id":0.5})),
    ] {
        let before = s.doc().unwrap().clone();
        let undo = s.undo_depth();
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
        assert_eq!(s.doc().unwrap(), &before, "{cmd} changed the document");
        assert_eq!(s.undo_depth(), undo);
    }
}

#[test]
fn clip_ids_remain_unique_when_a_file_contains_maximum_ids() {
    let mut clip = Clip::new(ClipKind::Opacity, ClipMode::In);
    clip.id = u32::MAX;
    let m = NodeMotion { clips: vec![clip.clone(), clip.clone(), clip], ..Default::default() };
    let m: NodeMotion = serde_json::from_value(json!(m)).unwrap();
    let ids: std::collections::BTreeSet<_> = m.clips.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), 3);
    assert!(!ids.contains(&m.next_clip_id()));
}

#[test]
fn highlights_are_saved_duplicated_undoable_and_document_scoped() {
    let mut s = Session::new();
    let id = rect(&mut s);
    let blue = Color::from_hex_or_name("#3B82F6").unwrap();
    run(&mut s, "layer.setHighlight", json!({"ids":[id],"color":"#3B82F6"}));
    assert_eq!(s.doc().unwrap().node(id).unwrap().highlight_color, Some(blue));
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (d, _) = figcraft_format::load_bytes(&bytes).unwrap();
    assert_eq!(d.node(id).unwrap().highlight_color, Some(blue));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().node(id).unwrap().highlight_color, None);
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.duplicate", json!({"ids":[id]}));
    let copy = s.selection()[0];
    assert_ne!(copy, id);
    assert_eq!(s.doc().unwrap().node(copy).unwrap().highlight_color, Some(blue));
    run(&mut s, "file.new", json!({}));
    assert!(s.doc().unwrap().nodes().all(|n| n.highlight_color.is_none()));
}

#[test]
fn highlight_main_scope_has_no_hidden_global_or_future_layer_effect() {
    let mut s = Session::new();
    let child = rect(&mut s);
    let frame = NodeId(run(&mut s, "object.frameSelection", json!({"ids":[child]}))["id"].as_u64().unwrap());
    let other = rect(&mut s);
    run(&mut s, "layer.setHighlight", json!({"allMain":true,"color":"#EF4444"}));
    assert!(s.doc().unwrap().node(frame).unwrap().highlight_color.is_some());
    for id in [child, other] {
        assert!(s.doc().unwrap().node(id).unwrap().highlight_color.is_none());
    }
    run(&mut s, "layer.setHighlight", json!({"ids":[child],"color":"blue"}));
    run(&mut s, "layer.resetHighlight", json!({"allMain":true}));
    assert!(s.doc().unwrap().node(frame).unwrap().highlight_color.is_none());
    assert!(s.doc().unwrap().node(child).unwrap().highlight_color.is_some());
    let later = NodeId(run(&mut s, "frame.create", json!({"x":300,"y":0,"width":100,"height":100}))["id"].as_u64().unwrap());
    assert!(s.doc().unwrap().node(later).unwrap().highlight_color.is_none());
}

#[test]
fn refused_highlights_and_animation_leave_no_history_or_mutation() {
    let mut s = Session::new();
    let id = rect(&mut s);
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth();
    s.ds_mut().unwrap().start_preview("v1", "Version", 0, std::sync::Arc::new(before.clone()));
    for (cmd, p) in [("layer.setHighlight", json!({"ids":[id],"color":"red"})), ("motion.clip", json!({"ids":[id],"kind":"Opacity"}))] {
        assert!(s.execute(cmd, &p).is_err());
    }
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), undo);
}

#[test]
fn malformed_highlights_are_atomic_and_defaults_remain_omitted() {
    let mut s = Session::new();
    let id = rect(&mut s);
    for p in [
        json!({"ids":[id],"color":"not a color"}),
        json!({"ids":[id],"color":true}),
        json!({"ids":[id],"allMain":true,"color":"blue"}),
        json!({"allMain":"true","color":"blue"}),
        json!({"ids":[id,999999],"color":"blue"}),
    ] {
        let before = s.doc().unwrap().clone();
        assert!(s.execute("layer.setHighlight", &p).is_err());
        assert_eq!(s.doc().unwrap(), &before);
    }
    assert!(!serde_json::to_string(s.doc().unwrap()).unwrap().contains("highlight_color"));
}

#[test]
fn a_table_corner_resize_preserves_fixed_columns_and_a_usable_last_cell() {
    let mut s = Session::new();
    run(&mut s, "file.newBoard", json!({}));
    let made = run(&mut s, "board.table", json!({"rows":2,"columns":3,"x":0,"y":0}));
    let table = NodeId(made["id"].as_u64().unwrap());
    run(&mut s, "board.tableEdit", json!({"id":table,"action":"resizeColumn","index":0,"size":300}));
    run(&mut s, "node.set", json!({"ids":[table],"props":{"width":10}}));
    let d = s.doc().unwrap();
    assert!(d.node(table).unwrap().width >= 500.0);
    let row = d.node(table).unwrap().children.iter().find(|id| !d.node(**id).unwrap().child_layout.absolute).unwrap();
    let cells = &d.node(*row).unwrap().children;
    assert_eq!(d.node(cells[0]).unwrap().width, 300.0);
    assert_eq!(d.node(cells[1]).unwrap().width, 160.0);
    assert!(d.node(cells[2]).unwrap().width >= 40.0);
    let saved = figcraft_format::save_bytes(d, None).unwrap();
    let (loaded, _) = figcraft_format::load_bytes(&saved).unwrap();
    assert_eq!(loaded.node(table).unwrap().width, d.node(table).unwrap().width);
}

#[test]
fn leaving_a_table_discards_the_old_active_cell() {
    use crate::tools::{PointerEvent, PointerKind};
    let mut s = Session::new();
    run(&mut s, "file.newBoard", json!({}));
    let made = run(&mut s, "board.table", json!({"rows":2,"columns":3,"x":0,"y":0}));
    let table = NodeId(made["id"].as_u64().unwrap());
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(&PointerEvent::at(kind, 40.0, 20.0, 1.0)).unwrap();
    }
    assert!(crate::board_table::active_cell(&s).is_some());
    s.set_selection(vec![]);
    s.set_selection(vec![table]);
    assert!(crate::board_table::active_cell(&s).is_none());
}

#[test]
fn node_inspection_exposes_saved_motion_and_highlights_with_field_filtering() {
    let mut s = Session::new();
    let id = rect(&mut s);
    run(&mut s, "layer.setHighlight", json!({"ids":[id],"color":"#3B82F6"}));
    run(&mut s, "motion.keyframe", json!({"ids":[id],"property":"Opacity","value":AnimValue::Scalar(0.3),"time":250}));
    let full = run(&mut s, "node.get", json!({"id":id}));
    assert_eq!(full["highlightColor"], "#3B82F6");
    assert_eq!(full["motion"]["tracks"][0]["keys"][0]["time_ms"], 250);
    let only = run(&mut s, "node.get", json!({"id":id,"fields":["motion","highlightColor"]}));
    assert_eq!(only["motion"], full["motion"]);
    assert_eq!(only["highlightColor"], full["highlightColor"]);
    assert!(only.get("width").is_none());
    let before = s.doc().unwrap().node(id).unwrap().motion.clone();
    run(&mut s, "motion.replace", json!({"node":id,"motion":only["motion"]}));
    assert_eq!(s.doc().unwrap().node(id).unwrap().motion, before);
}

#[test]
fn a_table_cell_does_not_return_after_changing_pages_or_files() {
    use crate::tools::{PointerEvent, PointerKind};
    let mut s = Session::new();
    run(&mut s, "file.newBoard", json!({}));
    let page = s.page().unwrap();
    let made = run(&mut s, "board.table", json!({"rows":2,"columns":3,"x":0,"y":0}));
    let table = NodeId(made["id"].as_u64().unwrap());
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(&PointerEvent::at(kind, 40.0, 20.0, 1.0)).unwrap();
    }
    assert!(crate::board_table::active_cell(&s).is_some());
    run(&mut s, "page.add", json!({"name":"Other"}));
    run(&mut s, "page.select", json!({"id":page}));
    s.set_selection(vec![table]);
    assert!(crate::board_table::active_cell(&s).is_none());
    for kind in [PointerKind::Down, PointerKind::Up] {
        s.pointer(&PointerEvent::at(kind, 40.0, 20.0, 1.0)).unwrap();
    }
    assert!(crate::board_table::active_cell(&s).is_some());
    run(&mut s, "file.newBoard", json!({}));
    run(&mut s, "board.table", json!({"rows":2,"columns":3,"x":0,"y":0}));
    assert!(crate::board_table::active_cell(&s).is_none());
}
