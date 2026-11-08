use figcraft_engine::Session;
use serde_json::json;

use crate::import_report::{MAX_NOTE_CHARS, MAX_NOTES, Report, note_text};
use crate::{FigApp, NoServices};

#[test]
fn import_results_with_warnings_become_reports_with_counts() {
    let result =
        json!({"warnings": ["layers without a type were skipped"], "summary": {"name": "Synthetic design", "pages": 2, "layers": 12, "images": 3}});
    let report = Report::from_result("file.import", &json!({}), &result).unwrap();
    assert_eq!(report.name, "Synthetic design");
    assert_eq!((report.pages, report.layers, report.images), (Some(2), Some(12), Some(3)));
    assert_eq!(report.notes, ["layers without a type were skipped"]);
    assert_eq!(report.note_count, 1);
}

#[test]
fn only_import_commands_with_warnings_create_reports() {
    // A warning-free `.fig` or REST open still has a summary to show.
    let report = Report::from_result("file.open", &json!({}), &json!({"warnings": [], "summary": {"name": "Local", "pages": 1}})).unwrap();
    assert!(!report.appended && report.notes.is_empty());
    // A native file's repairs have their own message; an SVG without notes has no `warnings`.
    assert!(Report::from_result("file.open", &json!({}), &json!({"repairs": ["Repaired parent"]})).is_none());
    assert!(Report::from_result("file.open", &json!({}), &json!({"index": 0})).is_none());
    assert!(Report::from_result("file.export", &json!({}), &json!({"warnings": ["Export warning"]})).is_none());
    assert!(Report::from_result("shape.rectangle", &json!({}), &json!({})).is_none());
    // File ▸ Open recent opens by path and reports like File ▸ Open.
    let recent = json!({"warnings": ["layers without a type were skipped"], "summary": {"name": "Recent", "pages": 1}});
    assert!(Report::from_result("file.openRecent", &json!({"path": "Recent.json"}), &recent).is_some());
    assert!(Report::from_result("file.openRecent", &json!({"path": "Clean.svg"}), &json!({"warnings": []})).is_none());
}

#[test]
fn svg_placements_are_not_reported_as_appended_pages() {
    assert!(Report::from_result("file.import", &json!({}), &json!({"id": 7})).is_none());
    let report = Report::from_result("file.import", &json!({}), &json!({"id": 7, "warnings": ["Filter skipped"]})).unwrap();
    assert!(!report.appended);
    assert!(Report::from_result("file.import", &json!({}), &json!({"pages": [1], "summary": {"pages": 1}, "warnings": []})).unwrap().appended);
}

#[test]
fn report_retention_is_bounded_and_truncates_only_at_character_boundaries() {
    let long_note = "🦎".repeat(MAX_NOTE_CHARS + 20);
    let notes: Vec<_> = std::iter::repeat_n(long_note, MAX_NOTES + 37).collect();
    let report = Report::from_result("file.open", &json!({}), &json!({"warnings": notes})).unwrap();
    assert_eq!(report.note_count, MAX_NOTES + 37);
    assert_eq!(report.notes.len(), MAX_NOTES);
    assert!(report.notes.iter().all(|note| note.chars().count() == MAX_NOTE_CHARS + 1 && note.ends_with('…')));
}

#[test]
fn note_text_keeps_the_repeat_suffix_and_unknown_notes() {
    let base = "instances of components that aren't in the file were imported as frames";
    let repeated = note_text(&format!("{base} (×122)"));
    assert_eq!(repeated, "Instances of components that aren't in the file were imported as frames (×122)");
    assert_eq!(note_text("some unknown note"), "some unknown note");
    assert_eq!(note_text("weird (×x)"), "weird (×x)");
    assert_eq!(note_text("weird (×12) tail"), "weird (×12) tail");
}

#[test]
fn vector_style_notes_are_known_import_notes() {
    for note in [
        "some vector points refer to point settings missing from the file; they use the layer's joins and plain corners",
        "vector points with unknown stroke joins use the layer join",
        "point stroke joins were preserved; dashed or variable-width strokes render with the layer join",
    ] {
        let shown = note_text(note);
        assert!(shown.chars().next().is_some_and(char::is_uppercase), "{shown}");
        assert_eq!(shown[shown.chars().next().unwrap().len_utf8()..], note[note.chars().next().unwrap().len_utf8()..]);
    }
}

#[test]
fn opening_a_rest_json_file_reports_what_was_skipped() {
    let path = std::env::temp_dir().join(format!("figcraft-import-report-{}.json", std::process::id()));
    let file = json!({"document": {"id": "0:0", "name": "Synthetic", "type": "DOCUMENT", "children": [
        {"id": "0:1", "name": "Page 1", "type": "CANVAS", "children": [
            {"id": "1:1", "name": "Card", "type": "FRAME", "size": {"x": 200, "y": 100}, "relativeTransform": [[1, 0, 10], [0, 1, 20]],
                "children": [{"id": "1:2", "name": "No type"}]}]}]}});
    std::fs::write(&path, file.to_string()).unwrap();
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    // A programmatic open (control channel, MCP) never opens the window.
    assert!(app.run("file.open", json!({"path": path.to_string_lossy()})).is_some());
    assert!(app.ui.import_report.is_none());
    let opened = app.run_user("file.open", json!({"path": path.to_string_lossy()}));
    std::fs::remove_file(&path).unwrap();
    assert!(opened.is_some());
    let report = app.ui.import_report.as_ref().expect("a report for the REST import");
    assert!(!report.appended);
    assert_eq!(report.pages, Some(1));
    assert!(report.layers.is_some_and(|n| n >= 1));
    assert!(report.notes.iter().any(|n| n == "layers without a type were skipped"), "{:?}", report.notes);
    assert_eq!(app.inspect()["importReport"]["warningCount"], report.note_count);
}

#[test]
fn escape_closes_the_import_report_like_other_simple_dialogs() {
    let mut u = crate::layers_drag_tests::Ui::new(Session::new());
    let mut report = Report::from_result("file.open", &json!({}), &json!({"warnings": ["Synthetic note"], "summary": {"name": "Synthetic design"}}));
    report.as_mut().unwrap().uid = u.app.session.ds().unwrap().uid;
    u.app.ui.import_report = report;
    u.frame(vec![]);
    let escape =
        vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }];
    u.frame(escape);
    assert!(u.app.ui.import_report.is_none(), "Escape closes the report");
}

pub(crate) fn shown_texts(shapes: Vec<egui::epaint::ClippedShape>) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

#[test]
fn a_clean_svg_open_has_no_report_and_a_placed_svg_says_where_it_went() {
    let svg_open = Report::from_result("file.open", &json!({"path": "icons/Logo.SVG"}), &json!({"warnings": []}));
    assert!(svg_open.is_none(), "a clean SVG opened with file.open has nothing to report");
    let noted = Report::from_result("file.open", &json!({"path": "Logo.svg"}), &json!({"warnings": ["Filter skipped"]})).unwrap();
    assert_eq!(noted.name, "Logo.svg");

    let placed = Report::from_result("file.import", &json!({"path": "dir/Logo.svg"}), &json!({"id": 7, "warnings": ["Filter skipped"]})).unwrap();
    assert!(placed.placed && !placed.appended);
    assert_eq!(placed.name, "Logo.svg");
    let mut u = crate::layers_drag_tests::Ui::new(Session::new());
    let mut placed = placed;
    placed.uid = u.app.session.ds().unwrap().uid;
    u.app.ui.import_report = Some(placed);
    let texts = shown_texts({
        u.frame(vec![]);
        u.frame_output(vec![]).shapes
    });
    assert!(texts.iter().any(|t| t == "Placed on the current page"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Opened as a new document"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Logo.svg"), "{texts:?}");
}

#[test]
fn the_report_goes_when_its_document_closes_and_counts_omitted_notes_in_the_singular() {
    let mut u = crate::layers_drag_tests::Ui::new(Session::new());
    let mut report = Report::from_result("file.open", &json!({}), &json!({"warnings": ["One note"], "summary": {"name": "Synthetic"}})).unwrap();
    report.uid = u.app.session.ds().unwrap().uid;
    report.note_count = 2;
    u.app.ui.import_report = Some(report);
    let texts = shown_texts({
        u.frame(vec![]);
        u.frame_output(vec![]).shapes
    });
    assert!(texts.iter().any(|t| t == "1 more note omitted from this report"), "{texts:?}");

    u.app.session.execute("file.new", &json!({"name": "Other"})).unwrap();
    u.frame(vec![]);
    assert!(u.app.ui.import_report.is_some(), "another tab was added; the report's document is still open");
    u.app.session.execute("file.close", &json!({"index": 0, "force": true})).unwrap();
    u.frame(vec![]);
    assert!(u.app.ui.import_report.is_none(), "the report left with its document");
}

#[test]
fn a_dropped_file_shows_its_report_once_it_opens() {
    let path = std::env::temp_dir().join(format!("figcraft-import-report-drop-{}.json", std::process::id()));
    let file = json!({"document": {"id": "0:0", "name": "Dropped", "type": "DOCUMENT", "children": [
        {"id": "0:1", "name": "Page 1", "type": "CANVAS", "children": [{"id": "1:1", "name": "No type"}]}]}});
    std::fs::write(&path, file.to_string()).unwrap();
    let mut u = crate::layers_drag_tests::Ui::new(Session::new());
    u.app.ui.pending_opens.push(crate::pending_open::PendingOpen {
        label: "Dropped.json".into(),
        params: json!({"path": path.to_string_lossy()}),
        command: "file.open",
        target: None,
        read: None,
    });
    for _ in 0..6 {
        u.frame(vec![]);
    }
    std::fs::remove_file(&path).unwrap();
    let report = u.app.ui.import_report.as_ref().expect("the dropped file's report");
    assert_eq!(report.uid, u.app.session.ds().unwrap().uid);
}

#[test]
fn a_file_opened_from_base64_data_has_a_summary_too() {
    use base64::Engine as _;
    let file = json!({"document": {"id": "0:0", "name": "From data", "type": "DOCUMENT", "children": [
        {"id": "0:1", "name": "Page 1", "type": "CANVAS", "children": [{"id": "1:1", "name": "No type"}]}]}});
    let data = base64::engine::general_purpose::STANDARD.encode(file.to_string());
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    let opened = app.run_user("file.open", json!({"data": data, "name": "Dropped.json"})).unwrap();
    assert_eq!(opened["summary"]["pages"], 1);
    let report = app.ui.import_report.as_ref().expect("a report for the REST import");
    assert_eq!(report.pages, Some(1));
    assert!(report.notes.iter().any(|n| n == "layers without a type were skipped"), "{:?}", report.notes);
    // Shown as a sentence, worked out once when the report opens.
    assert!(report.shown.iter().any(|n| n == "Layers without a type were skipped"), "{:?}", report.shown);
}

#[test]
fn a_paste_that_left_something_out_is_reported_like_an_import() {
    let note = "image fills whose image isn't in the file show a missing-image placeholder and keep its name";
    let result = json!({"ids": [], "warnings": [note], "missingImages": ["Hero"]});
    let report = Report::from_result("edit.pasteDesignLayers", &json!({"html": "<span></span>"}), &result).unwrap();
    assert_eq!(report.name, "Pasted design layers");
    assert!(report.placed && !report.appended, "placed on the current page");
    assert_eq!(report.notes.len(), 1);
    // A paste that left nothing out opens no report.
    assert!(Report::from_result("edit.pasteDesignLayers", &json!({}), &json!({"ids": [], "warnings": []})).is_none());
}

/// Original synthetic content, encoded through the same ZIP/kiwi path as a desktop `.fig`.
fn reveal_import_fixture() -> Vec<u8> {
    use figcraft_doc::proto::{Action, Easing, Flow, Interaction, Transition, Trigger};
    use figcraft_doc::{Document, NodeKind};
    let mut doc = Document::new("Import reveal fixture");
    let empty = doc.pages[0];
    doc.node_mut(empty).unwrap().name = "Empty cover".into();
    let divider = doc.add_page("- Section");
    assert!(doc.is_divider(divider));
    let design = doc.add_page("Design");
    let next = doc.add_page("Next");
    let frame = |doc: &mut Document, page, name| doc.create(page, NodeKind::Frame, name, 10000.0, 20000.0, 400.0, 300.0).unwrap();
    let start = frame(&mut doc, design, "Start");
    let dest = frame(&mut doc, next, "Destination");
    doc.node_mut(start).unwrap().interactions.push(Interaction {
        trigger: Trigger::OnClick,
        actions: vec![Action::Navigate { dest, preserve_scroll: false }],
        transition: Transition::Instant,
        duration_ms: 0,
        easing: Easing::default(),
        delay_ms: 0,
    });
    if let NodeKind::Page { flows, .. } = &mut doc.node_mut(design).unwrap().kind {
        flows.push(Flow { name: "Synthetic flow".into(), start, description: String::new() });
    }
    figcraft_format::fig::export(&doc, None).unwrap().0
}

fn assert_import_revealed(app: &FigApp, original: figcraft_doc::NodeId, original_layer: figcraft_doc::NodeId) {
    use figcraft_doc::NodeKind;
    use figcraft_doc::proto::Action;
    let doc = app.session.doc().unwrap();
    let active = app.session.page().unwrap();
    assert_ne!(active, original, "the appended design must be visible immediately");
    assert_eq!(doc.node(active).unwrap().name, "Design");
    assert!(doc.node(original).unwrap().children.contains(&original_layer));
    assert_eq!(doc.pages.len(), 5);
    let start = doc.nodes().find(|n| n.name == "Start").unwrap();
    let dest = doc.nodes().find(|n| n.name == "Destination").unwrap();
    assert_eq!(start.interactions[0].actions, [Action::Navigate { dest: dest.id, preserve_scroll: false }]);
    let NodeKind::Page { flows, .. } = &doc.node(active).unwrap().kind else { panic!() };
    assert_eq!(flows[0].start, start.id);
    let ds = app.session.ds().unwrap();
    assert!(ds.selection.is_empty());
    assert!((ds.view.cx - 10200.0).abs() < 1.0 && (ds.view.cy - 20150.0).abs() < 1.0, "the imported design is fitted");
    assert!(ds.view.zoom > 0.0 && ds.view.zoom < 1.0, "fitting reduces the deliberately oversized zoom");
    let bounds = doc.page_content_bounds(active).unwrap();
    let screen_x = |x| (x - ds.view.cx) * ds.view.zoom + ds.view.vw / 2.0;
    let screen_y = |y| (y - ds.view.cy) * ds.view.zoom + ds.view.vh / 2.0;
    assert!(screen_x(bounds.x0) >= 0.0 && screen_x(bounds.x1) <= ds.view.vw, "all imported content fits horizontally");
    assert!(screen_y(bounds.y0) >= 0.0 && screen_y(bounds.y1) <= ds.view.vh, "all imported content fits vertically");
    assert!(app.ui.import_report.as_ref().unwrap().appended);
    assert_eq!(app.ui.import_report.as_ref().unwrap().uid, ds.uid);
    if !cfg!(target_arch = "wasm32") {
        assert!(app.ui.font_checks.contains(&ds.uid), "font checking remains queued behind the import report");
        assert!(app.ui.missing_fonts.is_none());
    }
}

fn reveal_app() -> (FigApp, figcraft_doc::NodeId, figcraft_doc::NodeId) {
    let mut app = FigApp::new(Session::new(), Box::new(NoServices));
    let original = app.session.page().unwrap();
    let original_layer = app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 20, "height": 30})).unwrap()["id"].as_u64().unwrap();
    app.run("view.set", json!({"zoom": 8, "cx": 0, "cy": 0, "vw": 320, "vh": 240})).unwrap();
    (app, original, figcraft_doc::NodeId(original_layer))
}

#[test]
fn user_fig_import_from_contents_reveals_first_populated_page_and_preserves_flow_targets() {
    use base64::Engine as _;
    let (mut app, original, layer) = reveal_app();
    let data = base64::engine::general_purpose::STANDARD.encode(reveal_import_fixture());
    assert!(app.run_user("file.import", json!({"name": "Synthetic.fig", "data": data})).is_some());
    assert_import_revealed(&app, original, layer);
}

#[test]
fn user_fig_import_from_path_reveals_first_populated_page() {
    let path = std::env::temp_dir().join(format!("figcraft-import-reveal-{}.fig", std::process::id()));
    std::fs::write(&path, reveal_import_fixture()).unwrap();
    let (mut app, original, layer) = reveal_app();
    let result = app.run_user("file.import", json!({"path": path.to_string_lossy()}));
    std::fs::remove_file(path).unwrap();
    assert!(result.is_some());
    assert_import_revealed(&app, original, layer);
}

#[test]
fn programmatic_import_preserves_current_page_and_failed_user_import_does_not_switch() {
    use base64::Engine as _;
    let (mut app, original, layer) = reveal_app();
    let data = base64::engine::general_purpose::STANDARD.encode(reveal_import_fixture());
    assert!(app.run("file.import", json!({"name": "Synthetic.fig", "data": data})).is_some());
    assert_eq!(app.session.page().unwrap(), original);
    assert_eq!(app.session.ds().unwrap().selection, [layer]);
    assert!(app.ui.import_report.is_none());
    let pages = app.session.doc().unwrap().pages.clone();
    assert!(app.run_user("file.import", json!({"name": "Broken.fig", "data": "broken"})).is_none());
    assert_eq!(app.session.page().unwrap(), original);
    assert_eq!(app.session.doc().unwrap().pages, pages);
}

#[test]
fn user_svg_placement_stays_on_the_current_page() {
    use base64::Engine as _;
    let (mut app, original, _) = reveal_app();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="30"><rect width="20" height="30" fill="#ff0000"/></svg>"##;
    let data = base64::engine::general_purpose::STANDARD.encode(svg);
    let result = app.run_user("file.import", json!({"name": "Synthetic.svg", "data": data})).unwrap();
    assert!(result.get("pages").is_none());
    assert_eq!(app.session.page().unwrap(), original);
    assert_eq!(app.session.doc().unwrap().pages.len(), 1);
}

#[test]
fn user_empty_document_import_reveals_a_valid_empty_page() {
    use base64::Engine as _;
    let doc = figcraft_doc::Document::new("Empty imported document");
    let bytes = figcraft_format::fig::export(&doc, None).unwrap().0;
    let (mut app, original, _) = reveal_app();
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let result = app.run_user("file.import", json!({"name": "Empty.fig", "data": data})).unwrap();
    let appended = figcraft_engine::params::id_of(&result["pages"][0]).unwrap();
    assert_ne!(appended, original);
    assert_eq!(app.session.page().unwrap(), appended);
}

enum RevealPick {
    Path(std::path::PathBuf),
    Contents(Vec<u8>),
}

impl crate::Services for RevealPick {
    fn pick_open(&mut self, _: &str, _: &[&str]) -> crate::DialogFuture {
        let Self::Path(path) = self else { panic!("contents providers must not be asked for a filesystem path") };
        Box::pin(std::future::ready(Some(path.clone())))
    }

    fn pick_open_contents(&mut self, _: &str, _: &[&str]) -> Option<crate::ContentsFuture> {
        let Self::Contents(bytes) = self else { return None };
        Some(Box::pin(std::future::ready(Some(Ok(("Synthetic.fig".into(), bytes.clone()))))))
    }
}

fn assert_dialog_reveal(provider: RevealPick) {
    let (mut app, original, layer) = reveal_app();
    app.services = Box::new(provider);
    let target = app.session.ds().unwrap().uid;
    app.import_dialog(false);
    assert_eq!(app.session.doc().unwrap().pages.len(), 1, "no import before the picker is polled");
    assert!(app.ui.import_report.is_none());
    // The asynchronous continuation must import into the dialog's original document even after
    // the user switches tabs, then restore the tab they chose while waiting.
    app.run("file.new", json!({"name": "Other document"})).unwrap();
    let other = app.session.ds().unwrap().uid;
    app.poll_dialog(&egui::Context::default());
    assert!(app.pending_dialog.is_none());
    assert_eq!(app.session.ds().unwrap().uid, other);
    assert_eq!(app.session.doc().unwrap().pages.len(), 1, "the other document is untouched");
    assert_eq!(app.session.doc().unwrap().nodes().filter(|node| node.name == "Start").count(), 0);
    app.session.active = app.session.docs.iter().position(|ds| ds.uid == target).unwrap();
    assert_import_revealed(&app, original, layer);
}

#[test]
fn async_import_dialog_reveals_native_path_content_in_its_original_document() {
    let path = std::env::temp_dir().join(format!("figcraft-dialog-reveal-{}.fig", std::process::id()));
    std::fs::write(&path, reveal_import_fixture()).unwrap();
    assert_dialog_reveal(RevealPick::Path(path.clone()));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn async_import_dialog_reveals_provider_contents_in_its_original_document() {
    assert_dialog_reveal(RevealPick::Contents(reveal_import_fixture()));
}

#[test]
fn user_fig_import_is_one_undo_step_and_redo_preserves_cross_page_flow_targets() {
    use base64::Engine as _;
    use figcraft_doc::NodeKind;
    use figcraft_doc::proto::Action;
    let (mut app, original, layer) = reveal_app();
    let before = app.session.ds().unwrap().doc.clone();
    let undo_count = app.session.ds().unwrap().history.undo.len();
    let data = base64::engine::general_purpose::STANDARD.encode(reveal_import_fixture());
    app.run_user("file.import", json!({"name": "Synthetic.fig", "data": data})).unwrap();
    assert_import_revealed(&app, original, layer);
    assert_eq!(app.session.ds().unwrap().history.undo.len(), undo_count + 1, "revealing adds no separate Undo step");
    let imported_pages = app.session.doc().unwrap().pages.clone();
    let imported_nodes = app.session.doc().unwrap().nodes().count();

    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.session.doc().unwrap(), before.as_ref(), "one Undo restores all original content and removes every appended page");
    assert_eq!(app.session.page().unwrap(), original);
    assert!(app.session.doc().unwrap().node(layer).is_some());
    assert_eq!(app.session.ds().unwrap().history.undo.len(), undo_count);

    app.run("edit.redo", json!({})).unwrap();
    let doc = app.session.doc().unwrap();
    assert_eq!(doc.pages, imported_pages);
    assert_eq!(doc.nodes().count(), imported_nodes);
    assert!(doc.node(original).unwrap().children.contains(&layer));
    let start = doc.nodes().find(|node| node.name == "Start").unwrap();
    let dest = doc.nodes().find(|node| node.name == "Destination").unwrap();
    assert_ne!(doc.page_of(start.id), doc.page_of(dest.id));
    assert_eq!(start.interactions[0].actions, [Action::Navigate { dest: dest.id, preserve_scroll: false }]);
    let design = doc.page_of(start.id).unwrap();
    let NodeKind::Page { flows, .. } = &doc.node(design).unwrap().kind else { panic!() };
    assert_eq!(flows[0].start, start.id);
    assert!(doc.pages.contains(&app.session.page().unwrap()), "Redo keeps a valid active page");
    assert!(app.session.ds().unwrap().selection.iter().all(|id| doc.contains(*id)), "Redo keeps a valid selection");
    assert_eq!(app.session.ds().unwrap().history.undo.len(), undo_count + 1);
}
