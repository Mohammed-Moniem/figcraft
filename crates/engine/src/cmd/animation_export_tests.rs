use crate::Session;
use figcraft_doc::NodeKind;
use serde_json::json;
use std::sync::Arc;

#[test]
fn animation_export_command_produces_a_gif_without_editing_the_document() {
    let mut s = Session::new();
    let page = s.page().unwrap();
    let frame = s.doc_mut().unwrap().create(page, NodeKind::Frame, "Export", 0.0, 0.0, 32.0, 24.0).unwrap();
    let before = serde_json::to_value(s.doc().unwrap()).unwrap();
    let result = s.execute("motion.export", &json!({"id":frame, "duration":200, "fps":10, "format":"gif"})).unwrap();
    assert_eq!(result["format"], "gif");
    assert_eq!(result["frames"], 2);
    assert!(result["data"].as_str().is_some_and(|v| v.starts_with("R0lGOD")));
    assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
}

#[test]
fn animation_export_refuses_invalid_options_and_does_not_overwrite_existing_output() {
    let mut s = Session::new();
    let page = s.page().unwrap();
    let frame = s.doc_mut().unwrap().create(page, NodeKind::Frame, "Export", 0.0, 0.0, 16.0, 12.0).unwrap();
    let file = std::env::temp_dir().join(format!("figcraft-motion-invalid-{}.gif", std::process::id()));
    std::fs::write(&file, b"previous export").unwrap();
    let before = serde_json::to_value(s.doc().unwrap()).unwrap();
    for patch in [
        json!({"format":"mp4"}),
        json!({"fps":0}),
        json!({"fps":101}),
        json!({"fps":2.5}),
        json!({"duration":-1}),
        json!({"duration":0}),
        json!({"loop":"yes"}),
        json!({"scale":0}),
        json!({"scale":"2"}),
        json!({"id":page}),
    ] {
        let mut params = json!({"id":frame,"path":file,"duration":200,"fps":10});
        params.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        assert!(s.execute("motion.export", &params).is_err(), "{patch}");
        assert_eq!(std::fs::read(&file).unwrap(), b"previous export");
        assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
    }
    std::fs::remove_file(file).unwrap();
}

#[test]
fn animation_export_obeys_automation_write_roots() {
    use crate::automation::file_access::{Roots, confined};
    let dir = std::env::temp_dir().join(format!("figcraft-motion-roots-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("allowed")).unwrap();
    let roots = Arc::new(Roots::new(&[], &[dir.join("allowed").to_string_lossy().into_owned()]).unwrap());
    let mut s = Session::new();
    let page = s.page().unwrap();
    let frame = s.doc_mut().unwrap().create(page, NodeKind::Frame, "Export", 0.0, 0.0, 16.0, 12.0).unwrap();
    confined(Some(&roots), || {
        let params = |path| json!({"id":frame,"duration":200,"fps":10,"path":path});
        let error = s.execute("motion.export", &params(dir.join("outside.gif"))).unwrap_err().to_string();
        assert!(error.contains("automation path rejected"), "{error}");
        let result = s.execute("motion.export", &params(dir.join("allowed/output.gif"))).unwrap();
        assert_eq!(result["frames"], 2);
    });
    assert!(!dir.join("outside.gif").exists());
    assert!(std::fs::read(dir.join("allowed/output.gif")).unwrap().starts_with(b"GIF89a"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn animation_export_uses_page_timing_and_does_not_insert_an_undo_step() {
    let mut s = Session::new();
    let frame = s.execute("frame.create", &json!({"x":0,"y":0,"width":16,"height":12})).unwrap()["id"].clone();
    s.execute("motion.timeline", &json!({"duration":300,"fps":10,"loop":true})).unwrap();
    let before_selection = s.selection();
    let result = s.execute("motion.export", &json!({"id":frame})).unwrap();
    assert_eq!(result["frames"], 3);
    assert_eq!(result["duration"], 300);
    assert_eq!(result["loop"], true);
    assert_eq!(s.selection(), before_selection);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().timelines.is_empty(), "the previous edit is undone, not the export");
}
