use serde_json::json;

use super::*;
use crate::params::id_of;
use figcraft_doc::{Style, StyleRun, TextStyle};

fn text(s: &mut Session, family: &str) -> figcraft_doc::NodeId {
    let id = id_of(&s.execute("text.create", &json!({"text": "Hamburgefonstiv", "x": 0, "y": 0})).unwrap()["id"]).unwrap();
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(id).unwrap().kind {
        t.style.font_family = family.into();
    }
    id
}

fn families(s: &mut Session) -> Vec<(String, u64)> {
    let r = s.execute("text.missingFonts", &json!({})).unwrap();
    r["families"].as_array().unwrap().iter().map(|f| (f["family"].as_str().unwrap().to_string(), f["layers"].as_u64().unwrap())).collect()
}

/// An installed family (built in, or found by the system scan), if this machine has any.
fn installed() -> Option<String> {
    figcraft_text::scan_system_fonts();
    figcraft_text::family_names().into_iter().next()
}

#[test]
fn lists_families_of_layers_ranges_styles_and_variables_once_each() {
    let mut s = Session::new();
    text(&mut s, "FigCraft Missing A");
    let b = text(&mut s, "figcraft missing a");
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(b).unwrap().kind {
        let style = TextStyle { font_family: "FigCraft Missing B".into(), ..t.style.clone() };
        t.runs.push(StyleRun { start: 0, end: 3, style });
    }
    let doc = s.doc_mut().unwrap();
    doc.styles.push(Style {
        id: "S:1".into(),
        name: "Heading".into(),
        description: String::new(),
        kind: StyleKind::Text { style: TextStyle { font_family: "FigCraft Missing C".into(), ..TextStyle::default() } },
        bound: Default::default(),
    });
    s.execute("variable.createCollection", &json!({"name": "Type", "modes": ["Light", "Dark"]})).unwrap();
    s.execute("variable.create", &json!({"collection": "Type", "name": "Family", "type": "string", "value": "FigCraft Missing D"})).unwrap();
    s.execute("variable.set", &json!({"variable": "Family", "mode": "Dark", "value": "FigCraft Missing H"})).unwrap();
    let var = s.doc().unwrap().variables.get("Family").unwrap().id.clone();
    s.doc_mut().unwrap().get_mut(b).unwrap().bound.insert("fontFamily".into(), var);

    assert_eq!(
        families(&mut s),
        vec![
            ("FigCraft Missing A".to_string(), 2),
            ("FigCraft Missing B".to_string(), 1),
            ("FigCraft Missing C".to_string(), 0),
            ("FigCraft Missing D".to_string(), 0),
            ("FigCraft Missing H".to_string(), 0),
        ]
    );
    assert!(s.ui_requests.iter().any(|r| r.kind == "dialog.missingFonts"));
}

#[test]
fn installed_families_are_not_listed() {
    let Some(family) = installed() else {
        eprintln!("skipped: no fonts on this machine");
        return;
    };
    let mut s = Session::new();
    text(&mut s, &family);
    text(&mut s, &family.to_uppercase());
    text(&mut s, "FigCraft Missing E");
    assert_eq!(families(&mut s), vec![("FigCraft Missing E".to_string(), 1)]);
}

#[test]
fn a_family_found_only_by_the_system_scan_is_not_missing() {
    figcraft_text::scan_system_fonts();
    let embedded: Vec<String> = figcraft_text::CRAFT_FONTS.iter().map(|f| f.family.to_lowercase()).collect();
    let Some(family) = figcraft_text::family_names().into_iter().find(|f| !embedded.contains(&f.to_lowercase())) else {
        eprintln!("skipped: no system fonts on this machine");
        return;
    };
    let mut s = Session::new();
    text(&mut s, &family);
    assert!(families(&mut s).is_empty(), "{family} is installed");
}

#[test]
fn blank_families_and_aliases_are_handled() {
    let mut s = Session::new();
    text(&mut s, "  ");
    s.execute("variable.createCollection", &json!({"name": "Type"})).unwrap();
    s.execute("variable.create", &json!({"collection": "Type", "name": "Base", "type": "string", "value": "FigCraft Missing F"})).unwrap();
    let base = s.doc().unwrap().variables.get("Base").unwrap().id.clone();
    let t = text(&mut s, "FigCraft Missing G");
    let doc = s.doc_mut().unwrap();
    // A variable that only aliases another, and one aliasing itself.
    let mut alias = doc.variables.get(&base).unwrap().clone();
    alias.id = "V:alias".into();
    alias.name = "Alias".into();
    for v in alias.values.values_mut() {
        *v = VariableValue::Alias(base.clone());
    }
    let mut cycle = alias.clone();
    cycle.id = "V:cycle".into();
    cycle.name = "Cycle".into();
    for v in cycle.values.values_mut() {
        *v = VariableValue::Alias("V:cycle".into());
    }
    doc.variables.variables.push(alias);
    doc.variables.variables.push(cycle);
    doc.get_mut(t).unwrap().bound.insert("fontFamily".into(), "V:alias".into());
    let other = text(&mut s, "FigCraft Missing G");
    s.doc_mut().unwrap().get_mut(other).unwrap().bound.insert("fontFamily".into(), "V:cycle".into());
    // The bound layer takes the value its alias resolves to (bindings are applied after the next
    // command); the cycle resolves to nothing.
    let NodeKind::Text(bound) = &s.doc().unwrap().node(t).unwrap().kind else { panic!() };
    assert_eq!(bound.style.font_family, "FigCraft Missing F");
    assert_eq!(families(&mut s), vec![("FigCraft Missing F".to_string(), 1), ("FigCraft Missing G".to_string(), 1)]);
}

/// Aliases in several modes are followed once per variable: a chain that branches in every mode
/// would otherwise grow to billions of steps (a file can hold one).
#[test]
fn branching_alias_chains_are_walked_once_per_variable() {
    let mut s = Session::new();
    s.execute("variable.createCollection", &json!({"name": "Type"})).unwrap();
    for i in 0..32 {
        s.execute("variable.create", &json!({"collection": "Type", "name": format!("V{i}"), "type": "string", "value": "x"})).unwrap();
    }
    let doc = s.doc_mut().unwrap();
    let ids: Vec<String> = doc.variables.variables.iter().map(|v| v.id.clone()).collect();
    for (i, var) in doc.variables.variables.iter_mut().enumerate() {
        let next = ids.get(i + 1).cloned().unwrap_or_else(|| "FigCraft Missing H".into());
        let leaf = |v: String| {
            if i + 1 < ids.len() { VariableValue::Alias(v) } else { VariableValue::String(v) }
        };
        var.values = [("m1".to_string(), leaf(next.clone())), ("m2".to_string(), leaf(next))].into();
    }
    assert_eq!(string_values(&doc.variables, &ids[0]), vec!["FigCraft Missing H", "FigCraft Missing H"]);
}

#[test]
fn needs_an_open_document() {
    let mut s = Session::empty();
    assert!(s.execute("text.missingFonts", &json!({})).is_err());
}

/// Make `id`'s box stale, as if it was measured in another font: `dw` wider than its content.
fn stale(s: &mut Session, id: figcraft_doc::NodeId, dw: f64) -> f64 {
    let doc = s.doc_mut().unwrap();
    let n = doc.get_mut(id).unwrap();
    let want = figcraft_text::auto_size(n).unwrap().0;
    n.width = want + dw;
    doc.take_dirty();
    want
}

/// The width of layer `id`.
fn width(s: &Session, id: figcraft_doc::NodeId) -> f64 {
    s.doc().unwrap().node(id).unwrap().width
}

/// The ids a refit reports.
fn refitted(r: &Value) -> Vec<u64> {
    r["refitted"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect()
}

/// Stale boxes fit their text again, by family, by layer or everywhere; fixed boxes keep theirs.
#[test]
fn refit_measures_stale_text_boxes_again() {
    let mut s = Session::new();
    let a = text(&mut s, "FigCraft Refit A");
    let b = text(&mut s, "FigCraft Refit B");
    let fixed = text(&mut s, "FigCraft Refit A");
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(fixed).unwrap().kind {
        t.auto_resize = figcraft_doc::TextAutoResize::None;
    }
    s.execute("select.none", &json!({})).unwrap();
    let want_a = stale(&mut s, a, -30.0);
    let want_b = stale(&mut s, b, 25.0);
    s.doc_mut().unwrap().get_mut(fixed).unwrap().width = 7.0;
    s.doc_mut().unwrap().take_dirty();
    // Only the families asked for.
    let only_a = super::refit(s.doc_mut().unwrap(), None, Some(&["figcraft refit a".to_string()]));
    assert_eq!(only_a, vec![a]);
    assert_eq!(width(&s, a), want_a);
    assert_eq!(width(&s, b), want_b + 25.0);
    // Only the layers asked for, then everything; a fixed box keeps its size.
    stale(&mut s, a, 12.0);
    assert!(refitted(&s.execute("text.refit", &json!({"ids": [b]})).unwrap()) == vec![b.0]);
    assert_eq!((width(&s, a), width(&s, b)), (want_a + 12.0, want_b));
    assert!(refitted(&s.execute("text.refit", &json!({})).unwrap()) == vec![a.0]);
    assert_eq!(width(&s, a), want_a);
    assert_eq!(width(&s, fixed), 7.0);
    // Nothing stale: nothing changes.
    assert!(refitted(&s.execute("text.refit", &json!({})).unwrap()).is_empty());
    assert!(s.execute("text.refit", &json!({"ids": ["nope"]})).is_err());
    // An `ids` that isn't a list is refused, not taken as the whole document.
    stale(&mut s, a, 9.0);
    assert!(s.execute("text.refit", &json!({"ids": a.0.to_string()})).is_err());
    assert_eq!(width(&s, a), want_a + 9.0);
    assert!(Session::empty().execute("text.refit", &json!({})).is_err());
}

/// Text inside an instance is refitted too.
#[test]
fn refit_reaches_text_in_instances() {
    let mut s = Session::new();
    let t = text(&mut s, "FigCraft Refit C");
    let comp = id_of(&s.execute("component.create", &json!({"ids": [t]})).unwrap()["id"]).unwrap();
    let inst = id_of(&s.execute("instance.create", &json!({"component": comp, "x": 300, "y": 0})).unwrap()["id"]).unwrap();
    let copy = s.doc().unwrap().node(inst).unwrap().children[0];
    let want = stale(&mut s, copy, -20.0);
    let r = s.execute("text.refit", &json!({"ids": [inst]})).unwrap();
    assert_eq!(refitted(&r), vec![copy.0]);
    assert_eq!(width(&s, copy), want);
}

/// `text.loadFont` refuses what isn't a font file; `text.rescanFonts` reports what it added.
#[test]
fn fonts_load_from_files_and_rescans_report_new_families() {
    let mut s = Session::new();
    assert!(s.execute("text.loadFont", &json!({})).is_err());
    assert!(s.execute("text.loadFont", &json!({"data": "bm90IGEgZm9udA=="})).is_err(), "not a font file");
    assert!(s.execute("text.loadFont", &json!({"data": "%%%"})).is_err());
    assert!(s.execute("text.loadFont", &json!({"path": "no-such-dir/x.ttf"})).is_err());
    if let Some(f) = figcraft_text::CRAFT_FONTS.first() {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(f.bytes);
        let r = s.execute("text.loadFont", &json!({"data": data})).unwrap();
        assert_eq!(r["families"].as_array().map(Vec::len), Some(1));
    }
    let r = s.execute("text.rescanFonts", &json!({})).unwrap();
    assert!(r["added"].is_array());
}

/// Optional validation inputs stay outside every application source tree and build font input.
/// Native Node SHA-256 qualification before/after the selected owner control is mandatory;
/// this test helper checks the explicit manifest contract, file regularity and bounded sizes only.
fn external_appearance_fixture_fonts() -> (Vec<u8>, Vec<u8>) {
    use std::io::Read as _;
    use std::path::{Path, PathBuf};
    let root = PathBuf::from(std::env::var_os("FIGCRAFT_APPEARANCE_EXTERNAL_ROOT").expect("external validation fixture root is required"));
    let manifest = PathBuf::from(std::env::var_os("FIGCRAFT_APPEARANCE_EXTERNAL_MANIFEST").expect("pinned external validation manifest is required"));
    assert_eq!(
        std::env::var("FIGCRAFT_APPEARANCE_EXTERNAL_MANIFEST_SHA256").expect("external manifest SHA qualification is required"),
        "89108405cdaf97c1caf733cd0ad9e737d5f7a067e84fc0b5f927e3c3747bba4d"
    );
    assert!(root.is_absolute() && manifest.is_absolute());
    assert_eq!(root.canonicalize().unwrap(), root, "canonical external fixture path required");
    assert_eq!(manifest.canonicalize().unwrap(), manifest, "canonical external manifest path required");
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap();
    let workspace = crate_root.ancestors().nth(2).unwrap();
    assert!(
        !root.starts_with(workspace) && !manifest.starts_with(workspace),
        "fonts and validation manifests must remain outside the application source tree"
    );
    let metadata = std::fs::symlink_metadata(&manifest).unwrap();
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.len(), 745);
    let mut actual_manifest = Vec::new();
    std::fs::File::open(&manifest).unwrap().take(746).read_to_end(&mut actual_manifest).unwrap();
    const EXPECTED_MANIFEST: &[u8] = br##"{
  "schemaVersion": 1,
  "fixtureSet": "figcraft-jqval-distinct-metrics-v1",
  "license": "OFL-1.1",
  "family": "JQVal",
  "style": "Regular",
  "files": [
    {
      "role": "A",
      "relativePath": "fonts/JQVal-Regular-original-metrics.ttf",
      "bytes": 411608,
      "sha256": "5928c1e0c9525050845e5d74f3c882ffd8b8082c9ed2760b876c0679522f9a58"
    },
    {
      "role": "B",
      "relativePath": "fonts/JQVal-Regular-double-advances.ttf",
      "bytes": 411608,
      "sha256": "c46ce0d2b80d12e618f9cf40b8c3252017b863f51588a0258879b2d3026fef85"
    },
    {
      "role": "license",
      "relativePath": "OFL.txt",
      "bytes": 4380,
      "sha256": "262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a"
    }
  ]
}
"##;
    assert_eq!(actual_manifest.as_slice(), EXPECTED_MANIFEST, "exact external input contract required");
    let read = |relative: &str, size: u64| {
        let file = root.join(relative);
        assert_eq!(file.canonicalize().unwrap(), file, "no redirected validation inputs");
        let metadata = std::fs::symlink_metadata(&file).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert_eq!(metadata.len(), size);
        let mut bytes = Vec::new();
        std::fs::File::open(file).unwrap().take(size + 1).read_to_end(&mut bytes).unwrap();
        assert_eq!(u64::try_from(bytes.len()).unwrap(), size);
        bytes
    };
    let a = read("fonts/JQVal-Regular-original-metrics.ttf", 411608);
    let b = read("fonts/JQVal-Regular-double-advances.ttf", 411608);
    let _license = read("OFL.txt", 4380);
    (a, b)
}

fn external_appearance_load(s: &mut Session, bytes: &[u8]) -> Value {
    use base64::Engine as _;
    s.execute("text.loadFont", &json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes)})).unwrap()
}

fn external_appearance_text(s: &mut Session, name: &str, width: Option<f64>) -> NodeId {
    let mut p = json!({"text":"I","name":name,"props":{"fontFamily":"JQVal","fontStyle":"Regular","fontSize":128}});
    if let Some(width) = width {
        p["width"] = json!(width);
    }
    id_of(&s.execute("text.create", &p).unwrap()["id"]).unwrap()
}

fn external_appearance_value(s: &Session, id: NodeId) -> std::sync::Arc<figcraft_doc::TextAppearance> {
    let NodeKind::Text(t) = &s.doc().unwrap().node(id).unwrap().kind else { panic!("text fixture") };
    t.appearance.clone().expect("original named face has portable geometry")
}

fn external_appearance_remotes(s: &mut Session, fixed: NodeId) -> (NodeId, NodeId, NodeId) {
    let doc = s.doc_mut().unwrap();
    let page = doc.first_page().unwrap();
    let store = doc.ensure_library_store();
    let template = doc.node(fixed).unwrap().clone();
    let insert_remote = |doc: &mut figcraft_doc::Document, name: &str| {
        let id = doc.alloc_id();
        let mut n = template.clone();
        n.id = id;
        n.name = name.into();
        doc.insert(n, store, None).unwrap();
        assert!(doc.is_remote(id));
        id
    };
    let clean = insert_remote(doc, "Read-only same-box clean");
    let predirty = insert_remote(doc, "Read-only same-box predirty");
    let other = doc.alloc_id();
    let mut n = template;
    n.id = other;
    n.name = "Unrelated text-family scope witness".into();
    let t = n.kind.text_mut().unwrap();
    t.style.font_family = "Unrelated AP scope witness".into();
    t.appearance = None;
    doc.insert(n, page, None).unwrap();
    (clean, predirty, other)
}

fn external_appearance_remote_caches(
    s: &Session,
    remote_before: &[(NodeId, figcraft_doc::Node)],
    other_family_before: &std::sync::Arc<figcraft_doc::Node>,
    other_family_text: NodeId,
) {
    use std::sync::Arc;
    let appearance = external_appearance_value;
    let without_cache = |n: &figcraft_doc::Node| {
        let mut n = n.clone();
        n.kind.text_mut().unwrap().appearance = None;
        n
    };

    for (id, old) in remote_before {
        let new = s.doc().unwrap().node(*id).unwrap();
        assert_eq!(without_cache(old), without_cache(new), "all authored read-only fields remain exact");
        let ap = appearance(s, *id);
        assert_eq!(ap.box_width, 200.0);
        assert_eq!(ap.width, 68.75);
        assert_eq!(ap.glyphs[0].advance, 68.75);
    }
    assert!(
        Arc::ptr_eq(other_family_before, s.doc().unwrap().node_arc(other_family_text).unwrap()),
        "font-family scope must leave unrelated text untouched"
    );
}

fn external_appearance_saved_history(s: &mut Session, auto: NodeId, fixed: NodeId) -> (Value, figcraft_doc::Document) {
    let appearance = external_appearance_value;
    // The selected global face is not undoable. Restoring the old authored snapshot
    // must settle both automatic bounds and portable cache before it can be saved.
    let delivered = s.execute("file.saveAs", &json!({"data":true})).unwrap();
    assert_eq!(s.execute("file.markSaved", &json!({"document":delivered["document"],"revision":delivered["revision"]})).unwrap()["saved"], true);
    assert!(!s.ds().unwrap().is_dirty());
    let saved_b = s.doc().unwrap().clone();
    for (command, field) in [("edit.undo", "undone"), ("edit.redo", "redone")] {
        assert_eq!(s.execute(command, &json!({})).unwrap()[field], true);
        assert_eq!(width(s, auto), 69.0);
        assert_eq!(width(s, fixed), 200.0);
        for (id, box_width) in [(auto, 69.0), (fixed, 200.0)] {
            let current = appearance(s, id);
            assert_eq!(current.box_width, box_width);
            assert_eq!(current.width, 68.75);
            assert_eq!(current.glyphs[0].advance, 68.75);
        }
        assert_eq!(s.doc().unwrap(), &saved_b);
        assert!(!s.ds().unwrap().is_dirty(), "history settled to the exact saved B document");
    }
    (delivered, saved_b)
}

fn external_appearance_historical_readback(
    s: &mut Session,
    auto: NodeId,
    fixed: NodeId,
    remote_clean: NodeId,
    remote_predirty: NodeId,
    auto_a: std::sync::Arc<figcraft_doc::TextAppearance>,
    fixed_a: std::sync::Arc<figcraft_doc::TextAppearance>,
) {
    use base64::Engine as _;
    let appearance = external_appearance_value;
    // A historical/imported cache may predate the currently selected session face.
    // Encoding must refresh its owned clone even when editable inputs still fingerprint equal.
    for (id, old) in [(auto, auto_a), (fixed, fixed_a)] {
        let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(id).unwrap().kind else { panic!("text") };
        t.appearance = Some(old);
    }
    s.doc_mut().unwrap().take_dirty();
    let data = s.execute("file.saveAs", &json!({"data":true})).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD.decode(data["data"].as_str().unwrap()).unwrap();
    let (fresh, _) = figcraft_format::load_bytes(&bytes).unwrap();
    for id in [auto, fixed, remote_clean, remote_predirty] {
        let NodeKind::Text(t) = &fresh.node(id).unwrap().kind else { panic!("saved text") };
        assert_eq!(t.appearance.as_ref().unwrap().glyphs[0].advance, 68.75);
    }
    let preserved = s.execute("text.preserveAppearance", &json!({})).unwrap();
    assert!(preserved["captured"].as_u64().unwrap() >= 2);
    for id in [auto, fixed, remote_clean, remote_predirty] {
        assert_eq!(appearance(s, id).glyphs[0].advance, 68.75);
    }
    let bytes = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    let (native, _) = figcraft_format::load_bytes(&bytes).unwrap();
    for id in [auto, fixed, remote_clean, remote_predirty] {
        let NodeKind::Text(t) = &native.node(id).unwrap().kind else { panic!("saved text") };
        assert_eq!(t.appearance.as_ref().unwrap().glyphs[0].advance, 68.75);
    }
}

fn external_appearance_loaded_geometry(s: &Session, loaded: &Value, auto: NodeId, fixed: NodeId) {
    let appearance = external_appearance_value;
    assert!(refitted(loaded).contains(&auto.0));
    assert!(!refitted(loaded).contains(&fixed.0), "refitted remains a size-change report");
    assert_eq!(width(s, auto), 69.0);
    assert_eq!(width(s, fixed), 200.0);
    for (id, box_width) in [(auto, 69.0), (fixed, 200.0)] {
        let a = appearance(s, id);
        assert_eq!(a.box_width, box_width);
        assert_eq!(a.width, 68.75);
        assert_eq!(a.glyphs[0].advance, 68.75);
        assert_eq!(a.lines[0].width, 68.75);
    }
}

fn external_appearance_duplicates_and_remote_guard(
    s: &mut Session,
    a: &[u8],
    b: &[u8],
    auto: NodeId,
    fixed: NodeId,
    remote_clean: NodeId,
) -> (std::sync::Arc<figcraft_doc::TextAppearance>, std::sync::Arc<figcraft_doc::TextAppearance>, figcraft_doc::Document) {
    use std::sync::Arc;
    let load = external_appearance_load;
    let appearance = external_appearance_value;
    let auto_b = appearance(s, auto);
    let fixed_b = appearance(s, fixed);
    for duplicate in [a, b] {
        load(s, duplicate);
        assert_eq!(width(s, auto), 69.0);
        assert!(Arc::ptr_eq(&auto_b, &appearance(s, auto)));
        assert!(Arc::ptr_eq(&fixed_b, &appearance(s, fixed)));
    }
    let unchanged = s.doc().unwrap().clone();
    assert!(s.execute("node.set", &json!({"id":remote_clean,"props":{"width":201}})).is_err());
    assert_eq!(s.doc().unwrap(), &unchanged, "ordinary authored remote geometry still refuses and rolls back");
    (auto_b, fixed_b, unchanged)
}

/// Font replacement refreshes portable geometry even when editable inputs or a fixed box did not change.
#[test]
#[ignore = "external validation fixtures required; use qualified owner control with --include-ignored"]
fn loaded_face_refreshes_auto_and_fixed_portable_appearance_and_duplicate_loads_keep_selected_face() {
    use std::sync::Arc;
    let (a, b) = external_appearance_fixture_fonts();
    let load = external_appearance_load;
    let create = external_appearance_text;
    let appearance = external_appearance_value;
    let mut s = Session::new();
    assert!(load(&mut s, &a)["families"].as_array().unwrap().iter().any(|f| f == "JQVal"));
    let auto = create(&mut s, "Automatic probe", None);
    let fixed = create(&mut s, "Fixed probe", Some(200.0));
    let guest = create(&mut s, "Partial missing font guest", Some(200.0));
    let guest_appearance = appearance(&s, guest);
    if let NodeKind::Text(t) = &mut s.doc_mut().unwrap().get_mut(guest).unwrap().kind {
        let mut missing = t.style.clone();
        missing.font_family = "Absent JQVal portable guest face".into();
        t.runs.push(StyleRun { start: 0, end: 1, style: missing });
        t.auto_resize = figcraft_doc::TextAutoResize::None;
    }
    // Read-only same-box copies use the existing library-store fixture pattern, not a second global face test.
    let (remote_clean, remote_predirty, other_family_text) = external_appearance_remotes(&mut s, fixed);
    let remote_a = appearance(&s, remote_clean);
    s.doc_mut().unwrap().take_dirty();
    assert!(figcraft_text::has_family("Inter"), "the required craft-fonts input includes Inter");
    let unrelated_text =
        id_of(&s.execute("text.create", &json!({"text":"Unrelated text","props":{"fontFamily":"Inter","fontSize":24}})).unwrap()["id"]).unwrap();
    let unrelated_text_before = s.doc().unwrap().node_arc(unrelated_text).unwrap().clone();
    let unrelated = id_of(&s.execute("shape.rectangle", &json!({"width":20,"height":20})).unwrap()["id"]).unwrap();
    let unrelated_before = s.doc().unwrap().node_arc(unrelated).unwrap().clone();
    assert_eq!(width(&s, auto), 35.0);
    assert_eq!(appearance(&s, auto).glyphs[0].advance, 34.375);
    assert_eq!(appearance(&s, fixed).glyphs[0].advance, 34.375);
    let auto_a = appearance(&s, auto);
    let fixed_a = appearance(&s, fixed);
    s.doc_mut().unwrap().mark_dirty([remote_predirty]);
    assert!(s.doc().unwrap().dirty().contains(&remote_predirty));
    assert!(!s.doc().unwrap().dirty().contains(&remote_clean));
    let remote_before: Vec<_> = [remote_clean, remote_predirty].into_iter().map(|id| (id, s.doc().unwrap().node(id).unwrap().clone())).collect();
    let other_family_before = s.doc().unwrap().node_arc(other_family_text).unwrap().clone();
    let loaded = load(&mut s, &b);
    external_appearance_loaded_geometry(&s, &loaded, auto, fixed);
    assert!(Arc::ptr_eq(&guest_appearance, &appearance(&s, guest)), "a missing named run never captures a substitute");
    assert!(Arc::ptr_eq(&unrelated_before, s.doc().unwrap().node_arc(unrelated).unwrap()));
    assert!(Arc::ptr_eq(&unrelated_text_before, s.doc().unwrap().node_arc(unrelated_text).unwrap()));
    let check_remote_caches = |s: &Session| external_appearance_remote_caches(s, &remote_before, &other_family_before, other_family_text);
    assert!(
        !refitted(&loaded).contains(&remote_clean.0) && !refitted(&loaded).contains(&remote_predirty.0),
        "same-box cache changes are not size refits"
    );
    check_remote_caches(&s);
    let (_delivered, _saved_b) = external_appearance_saved_history(&mut s, auto, fixed);
    let (_auto_b, _fixed_b, _unchanged) = external_appearance_duplicates_and_remote_guard(&mut s, &a, &b, auto, fixed, remote_clean);
    let restore_old_cache = |s: &mut Session, id: NodeId| {
        s.doc_mut().unwrap().get_mut(id).unwrap().kind.text_mut().unwrap().appearance = Some(remote_a.clone());
    };
    restore_old_cache(&mut s, remote_clean);
    s.doc_mut().unwrap().take_dirty();
    assert!(s.execute("text.preserveAppearance", &json!({})).unwrap()["captured"].as_u64().unwrap() > 0);
    check_remote_caches(&s);
    restore_old_cache(&mut s, remote_clean);
    s.doc_mut().unwrap().take_dirty();
    s.execute("text.refit", &json!({})).unwrap();
    check_remote_caches(&s);
    #[cfg(not(target_arch = "wasm32"))]
    {
        restore_old_cache(&mut s, remote_predirty);
        assert!(s.doc().unwrap().dirty().contains(&remote_predirty));
        // JQVal is already available, so this takes the synchronous refit branch, never a network/install path.
        assert_eq!(s.execute("font.download", &json!({"family":"JQVal"})).unwrap()["state"], "installed");
        check_remote_caches(&s);
    }
    external_appearance_historical_readback(&mut s, auto, fixed, remote_clean, remote_predirty, auto_a, fixed_a);
}
