//! The page colour of a new file follows the interface's theme when it is made (dark: #1E1E1E,
//! light: #F5F5F5) and is the page's own from then on; an added page takes the colour of the page
//! it was added from. A session without an interface (CLI, MCP, exports) has no theme and keeps the
//! light default. Text drawn with the Text tool is white on a dark page and black on a light one.

use figcraft_color::{Color, Paint};
use figcraft_doc::{NodeId, NodeKind};
use serde_json::{Value, json};

use crate::Session;
use crate::params::id_of;
use crate::tools::{PointerEvent, PointerKind};

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn background(s: &Session, page: NodeId) -> Color {
    match s.doc().unwrap().node(page).unwrap().kind {
        NodeKind::Page { background, .. } => background,
        _ => panic!("{page} is not a page"),
    }
}

fn backgrounds(s: &Session) -> Vec<String> {
    s.doc().unwrap().pages.iter().map(|p| background(s, *p).to_css_hex().to_uppercase()).collect()
}

/// Click with the Text tool at (x, y) on the current page; the new text layer.
fn draw_text(s: &mut Session, x: f64, y: f64) -> NodeId {
    ex(s, "tool.text", json!({}));
    s.pointer(&PointerEvent::at(PointerKind::Down, x, y, 1.0)).unwrap();
    s.pointer(&PointerEvent::at(PointerKind::Up, x, y, 1.0)).unwrap();
    let id = s.selection()[0];
    ex(s, "text.edit.insert", json!({"text": "Hi"}));
    ex(s, "text.edit.end", json!({}));
    id
}

#[test]
fn new_files_take_the_theme_they_are_made_in_and_added_pages_follow() {
    // (theme the interface reported, colour of a new file's page and of a page added to it)
    for (dark, want) in [(None, "#F5F5F5"), (Some(false), "#F5F5F5"), (Some(true), "#1E1E1E")] {
        let mut s = Session::empty();
        if let Some(d) = dark {
            s.set_ui_dark(d);
        }
        ex(&mut s, "file.new", json!({}));
        ex(&mut s, "page.add", json!({}));
        assert_eq!(backgrounds(&s), [want, want], "theme {dark:?}");
        // One undo step for the page, with its colour.
        ex(&mut s, "edit.undo", json!({}));
        ex(&mut s, "edit.redo", json!({}));
        assert_eq!(backgrounds(&s), [want, want], "theme {dark:?} after undo and redo");
    }
}

#[test]
fn a_change_of_theme_leaves_existing_pages_alone() {
    let mut s = Session::empty();
    s.set_ui_dark(true);
    ex(&mut s, "file.new", json!({}));
    s.set_ui_dark(false);
    ex(&mut s, "page.add", json!({}));
    assert_eq!(backgrounds(&s), ["#1E1E1E", "#1E1E1E"], "an added page follows its page, not the theme");
    ex(&mut s, "page.setBackground", json!({"color": "#F5F5F5"}));
    s.set_ui_dark(true);
    assert_eq!(backgrounds(&s), ["#1E1E1E", "#F5F5F5"], "each page keeps its colour");
    // A copy of a page is that page's colour, whatever the theme.
    let first = s.doc().unwrap().pages[0];
    s.set_ui_dark(false);
    let copy = id_of(&ex(&mut s, "page.duplicate", json!({"id": first}))["id"]).unwrap();
    assert_eq!(background(&s, copy), Color::DARK_PAGE);
}

#[test]
fn an_added_page_takes_the_current_pages_colour() {
    // Whatever the theme, and headless too: the page it was added from decides.
    for dark in [None, Some(false), Some(true)] {
        let mut s = Session::empty();
        if let Some(d) = dark {
            s.set_ui_dark(d);
        }
        ex(&mut s, "file.new", json!({}));
        ex(&mut s, "page.setBackground", json!({"color": "#336699"}));
        ex(&mut s, "page.add", json!({}));
        assert_eq!(backgrounds(&s), ["#336699", "#336699"], "theme {dark:?}");
        // From the second page, now current, after giving it its own colour.
        ex(&mut s, "page.setBackground", json!({"color": "#FFE08A"}));
        ex(&mut s, "page.add", json!({"name": "Third"}));
        assert_eq!(backgrounds(&s), ["#336699", "#FFE08A", "#FFE08A"], "theme {dark:?}");
        // One undo step, the colour with it.
        ex(&mut s, "edit.undo", json!({}));
        assert_eq!(backgrounds(&s), ["#336699", "#FFE08A"], "theme {dark:?} after undo");
        ex(&mut s, "edit.redo", json!({}));
        assert_eq!(backgrounds(&s), ["#336699", "#FFE08A", "#FFE08A"], "theme {dark:?} after redo");
    }
    // From a page other than the first: go there, then add.
    let mut s = Session::new();
    s.set_ui_dark(true);
    ex(&mut s, "page.add", json!({}));
    ex(&mut s, "page.setBackground", json!({"color": "#0B1F3A"}));
    ex(&mut s, "page.select", json!({"index": 0}));
    ex(&mut s, "page.add", json!({}));
    assert_eq!(backgrounds(&s), ["#1E1E1E", "#0B1F3A", "#1E1E1E"], "the current page decides, not the last");
}

#[test]
fn added_page_colours_survive_save_and_reopen() {
    let dir = std::env::temp_dir().join(format!("figcraft-added-pages-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pages.figcraft");
    let mut s = Session::new();
    ex(&mut s, "page.setBackground", json!({"color": "#336699"}));
    ex(&mut s, "page.add", json!({}));
    ex(&mut s, "file.saveAs", json!({"path": path.to_string_lossy()}));
    let mut s = Session::empty();
    s.set_ui_dark(true);
    ex(&mut s, "file.open", json!({"path": path.to_string_lossy()}));
    assert_eq!(backgrounds(&s), ["#336699", "#336699"]);
    // And a page added to the reopened file in the dark theme follows the file, not the theme.
    ex(&mut s, "page.add", json!({}));
    assert_eq!(backgrounds(&s), ["#336699", "#336699", "#336699"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_the_app_opened_with_takes_the_theme_once_it_is_known() {
    // Made before the interface knew its theme, nothing done to it yet.
    let mut s = Session::new();
    s.set_ui_dark(true);
    assert_eq!(backgrounds(&s), ["#1E1E1E"]);
    assert!(!s.ds().unwrap().is_dirty(), "still a file without changes");
    assert!(s.ds().unwrap().history.undo.is_empty());
    // A file with something in it, or one whose page colour was chosen, is left as it is.
    let mut s = Session::new();
    ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10}));
    s.set_ui_dark(true);
    assert_eq!(backgrounds(&s), ["#F5F5F5"]);
    let mut s = Session::new();
    ex(&mut s, "page.setBackground", json!({"color": "#336699"}));
    s.set_ui_dark(true);
    assert_eq!(backgrounds(&s), ["#336699"]);
    // Later changes of theme never repaint a page.
    let mut s = Session::new();
    s.set_ui_dark(false);
    s.set_ui_dark(true);
    assert_eq!(backgrounds(&s), ["#F5F5F5"]);
}

#[test]
fn opened_files_keep_their_page_colour() {
    let dir = std::env::temp_dir().join(format!("figcraft-theme-pages-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("light.figcraft");
    let mut s = Session::new();
    ex(&mut s, "file.saveAs", json!({"path": path.to_string_lossy()}));
    let mut s = Session::empty();
    s.set_ui_dark(true);
    ex(&mut s, "file.open", json!({"path": path.to_string_lossy()}));
    assert_eq!(backgrounds(&s), ["#F5F5F5"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_drawn_on_a_dark_page_is_white_and_on_a_light_page_black() {
    // (page colour, the new text's fill): by the page, not the theme.
    for (page, want) in [("#1E1E1E", Color::WHITE), ("#F5F5F5", Color::BLACK), ("#0B1F3A", Color::WHITE), ("#FFE08A", Color::BLACK)] {
        let mut s = Session::new();
        ex(&mut s, "page.setBackground", json!({"color": page}));
        let t = draw_text(&mut s, 40.0, 40.0);
        assert_eq!(s.doc().unwrap().node(t).unwrap().fills, vec![Paint::solid(want)], "text drawn on {page}");
    }
    // In a (white) frame on a dark page, readable on the frame (#362); so is text a command makes
    // in the app (`dark_page_text_tests`).
    let mut s = Session::empty();
    s.set_ui_dark(true);
    ex(&mut s, "file.new", json!({}));
    ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 200}));
    let t = draw_text(&mut s, 50.0, 50.0);
    assert_eq!(s.doc().unwrap().node(t).unwrap().fills, vec![Paint::solid(Color::BLACK)]);
    let made = id_of(&ex(&mut s, "text.create", json!({"text": "Hi", "x": 0, "y": 300}))["id"]).unwrap();
    assert_eq!(s.doc().unwrap().node(made).unwrap().fills, vec![Paint::solid(Color::WHITE)]);
}

/// `page.duplicate` copies the page's flows (pointing at the copied frames), prototype settings
/// and guides.
#[test]
fn a_copy_of_a_page_keeps_its_flows_prototype_settings_and_guides() {
    let mut s = Session::new();
    ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 100, "height": 100}));
    let start = ex(&mut s, "frame.create", json!({"x": 200, "y": 0, "width": 100, "height": 100}))["id"].clone();
    ex(&mut s, "proto.flow.add", json!({"start": start, "name": "Checkout"}));
    ex(&mut s, "proto.settings", json!({"device": "iPhone 16", "responsive": true}));
    ex(&mut s, "guide.add", json!({"vertical": true, "offset": 25}));
    ex(&mut s, "select.none", json!({}));
    let source = s.page().unwrap();
    let copy = id_of(&ex(&mut s, "page.duplicate", json!({}))["id"]).unwrap();
    let frames = s.doc().unwrap().node(copy).unwrap().children.clone();
    let state = |s: &Session, id: NodeId| {
        let n = s.doc().unwrap().node(id).unwrap();
        match &n.kind {
            NodeKind::Page { flows, prototype, .. } => (flows.clone(), prototype.clone(), n.guides.clone()),
            _ => panic!("expected a page"),
        }
    };
    let (flows, prototype, guides) = state(&s, copy);
    let (src_flows, src_prototype, src_guides) = state(&s, source);
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].name, "Checkout");
    assert_eq!(flows[0].start, frames[1], "the flow starts at the copied frame");
    assert_eq!(src_flows[0].start, id_of(&start).unwrap(), "the source keeps its own");
    assert_eq!(prototype, src_prototype);
    assert_eq!(prototype.device, "iPhone 16");
    assert!(prototype.responsive);
    assert_eq!(guides, src_guides);
    assert_eq!(guides.len(), 1);
    assert_eq!(ex(&mut s, "proto.present", json!({}))["start"].as_u64(), Some(frames[1].0));
}

#[test]
fn a_copy_of_a_page_keeps_a_flow_that_starts_in_a_section() {
    let mut s = Session::new();
    let start = id_of(&ex(&mut s, "frame.create", json!({"x": 0, "y": 0, "width": 100, "height": 100}))["id"]).unwrap();
    ex(&mut s, "object.wrapInSection", json!({"ids": [start]}));
    ex(&mut s, "proto.flow.add", json!({"start": start, "name": "Onboarding"}));
    let copy = id_of(&ex(&mut s, "page.duplicate", json!({}))["id"]).unwrap();
    let doc = s.doc().unwrap();
    let section = doc.node(copy).unwrap().children[0];
    let frame = doc.node(section).unwrap().children[0];
    match &doc.node(copy).unwrap().kind {
        NodeKind::Page { flows, .. } => assert_eq!(flows.iter().map(|f| (f.name.as_str(), f.start)).collect::<Vec<_>>(), vec![("Onboarding", frame)]),
        _ => panic!("expected a page"),
    }
}

#[test]
fn theme_background_native_data_authority_is_preserved_before_first_ui_theme() {
    use base64::Engine as _;
    for color in ["#F5F5F5", "#1E1E1E", "#336699"] {
        let mut original = Session::new();
        ex(&mut original, "page.setBackground", json!({"color":color}));
        let bytes = figcraft_format::save_bytes(original.doc().unwrap(), None).unwrap();
        let mut opened = Session::empty();
        ex(&mut opened, "file.open", json!({"name":"Original saved blank.figcraft","data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
        let before = opened.doc().unwrap().clone();
        assert_eq!(opened.ui_dark, None, "the input arrives before an interface reports its theme");
        opened.set_ui_dark(true);
        assert_eq!(backgrounds(&opened), [color], "a saved page is authoritative even when opened without a path");
        assert_eq!(opened.doc().unwrap(), &before);
        assert!(!opened.ds().unwrap().is_dirty());
        assert_eq!(opened.undo_depth(), (0, 0));
    }
}

#[test]
fn theme_background_first_light_theme_is_consumed_before_a_new_owner_receives_the_document() {
    let mut first = Session::new();
    first.set_ui_dark(false);
    let mut second = Session::empty();
    second.docs.push(first.docs.pop().unwrap());
    second.set_ui_dark(true);
    assert_eq!(backgrounds(&second), ["#F5F5F5"], "an initialized page does not become fresh again in a new interface owner");
    assert!(!second.ds().unwrap().is_dirty());
    assert_eq!(second.undo_depth(), (0, 0));
}

#[test]
fn theme_background_live_welcome_and_remote_edits_keep_authoritative_page_color() {
    let mut room = crate::collab_live_tests::Room::new(figcraft_doc::Document::new("Original shared blank"), 2, 207_600);
    for index in 0..2 {
        let peer = room.s(index);
        let before = peer.doc().unwrap().clone();
        peer.set_ui_dark(true);
        assert_eq!(backgrounds(peer), ["#F5F5F5"]);
        assert_eq!(peer.doc().unwrap(), &before, "a welcome is authoritative, not a fresh Design creation");
        assert_eq!(peer.undo_depth(), (0, 0));
    }
    room.check();
    room.s(0).execute("shape.rectangle", &json!({"x":10,"y":10,"width":20,"height":20})).unwrap();
    room.settle();
    room.check();
    assert_eq!(backgrounds(room.s(1)), ["#F5F5F5"]);
    assert_eq!(room.s(1).undo_depth().0, 0, "a peer's real edit is separate from local theme state");
}

#[test]
fn theme_background_version_copies_and_branch_tabs_preserve_snapshot_page_authority() {
    for version in [false, true] {
        let mut session = Session::new();
        if version {
            let saved = ex(&mut session, "file.saveVersion", json!({"name":"Original light version"}));
            ex(&mut session, "version.duplicate", json!({"id":saved["id"]}));
        } else {
            ex(&mut session, "branch.create", json!({"name":"Original light branch"}));
        }
        let before: Vec<_> = session.docs.iter().map(|doc| (doc.doc.clone(), doc.is_dirty())).collect();
        let history = session.undo_depth();
        session.set_ui_dark(true);
        assert_eq!(backgrounds(&session), ["#F5F5F5"], "version={version}");
        let after: Vec<_> = session.docs.iter().map(|doc| (doc.doc.clone(), doc.is_dirty())).collect();
        assert_eq!(after, before, "theme initialization changes neither the snapshot nor its source file");
        assert_eq!(session.undo_depth(), history);
    }
}

#[test]
fn theme_background_an_edited_new_design_keeps_its_existing_color_and_undo_state() {
    for content in [false, true] {
        let mut session = Session::new();
        if content {
            ex(&mut session, "shape.rectangle", json!({"x":10,"y":10,"width":20,"height":20}));
        } else {
            ex(&mut session, "page.rename", json!({"name":"Original authored blank page"}));
        }
        assert!(session.ds().unwrap().is_dirty());
        let before = session.doc().unwrap().clone();
        let history = session.undo_depth();
        for dark in [true, false, true] {
            session.set_ui_dark(dark);
            assert_eq!(backgrounds(&session), ["#F5F5F5"]);
            assert_eq!(session.doc().unwrap(), &before);
            assert!(session.ds().unwrap().is_dirty());
            assert_eq!(session.undo_depth(), history);
        }
    }
}
