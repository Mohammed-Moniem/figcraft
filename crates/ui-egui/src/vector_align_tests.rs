//! Headless UI contracts for the actual Point row and existing Alt alignment shortcut.

use crate::layers_drag_tests::Ui;
use egui::accesskit::Node;
use egui::{Event, Key, Modifiers, Pos2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

fn path(s: &mut Session, points: serde_json::Value) -> NodeId {
    NodeId(s.execute("vector.create", &json!({"points":points,"closed":false})).unwrap()["id"].as_u64().unwrap())
}

fn fixture(vertices: &[usize]) -> (Ui, NodeId) {
    let mut s = Session::new();
    let id = path(&mut s, json!([[20, 80], [80, 20], [140, 100], [50, 160]]));
    s.execute("vector.edit.begin", &json!({"id":id})).unwrap();
    s.execute("vector.edit.select", &json!({"vertices":vertices})).unwrap();
    (Ui::new(s), id)
}

fn tree(ui: &mut Ui) -> Vec<Node> {
    ui.ctx.enable_accesskit();
    ui.frame(vec![]);
    ui.frame_output(vec![]).platform_output.accesskit_update.unwrap().nodes.into_iter().map(|(_, node)| node).collect()
}

fn named<'a>(tree: &'a [Node], label: &str) -> Option<&'a Node> {
    tree.iter().find(|node| node.label().is_some_and(|text| text.starts_with(label)))
}

fn click(ui: &mut Ui, node: &Node) {
    let r = node.bounds().unwrap();
    let p = Pos2::new(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32);
    ui.button(p, true);
    ui.button(p, false);
    ui.frame(vec![]);
}

fn xs(ui: &Ui, id: NodeId) -> Vec<f64> {
    let doc = ui.app.session.doc().unwrap();
    let world = doc.world_transform(id);
    figcraft_engine::vectoredit::network(doc, id).unwrap().vertices.iter().map(|point| (world * point.point()).x).collect()
}

#[test]
fn issue_2109_ui_point_row_has_six_enabled_buttons_and_click_dispatches_points() {
    let (mut ui, id) = fixture(&[0, 1]);
    let nodes = tree(&mut ui);
    for label in ["Align left", "Align horizontal centers", "Align right", "Align top", "Align vertical centers", "Align bottom"] {
        assert!(!named(&nodes, label).unwrap_or_else(|| panic!("missing {label}")).is_disabled());
    }
    assert!(named(&nodes, "Distribute and tidy").is_none());
    click(&mut ui, named(&nodes, "Align left").unwrap());
    let x = xs(&ui, id);
    assert!((x[0] - 20.0).abs() < 1e-6 && (x[1] - 20.0).abs() < 1e-6 && (x[2] - 140.0).abs() < 1e-6);
}

#[test]
fn issue_2109_ui_single_point_buttons_are_enabled_and_remain_a_noop() {
    let (mut ui, id) = fixture(&[1]);
    let before = xs(&ui, id);
    let depth = ui.app.session.undo_depth();
    let nodes = tree(&mut ui);
    let right = named(&nodes, "Align right").unwrap();
    assert!(!right.is_disabled());
    click(&mut ui, right);
    assert_eq!(xs(&ui, id), before);
    assert_eq!(ui.app.session.undo_depth(), depth);
    assert!(named(&nodes, "Distribute and tidy").is_none());
}

#[test]
fn issue_2109_ui_distribute_menu_has_only_point_distribution_actions() {
    let (mut ui, _) = fixture(&[0, 1, 2]);
    let nodes = tree(&mut ui);
    click(&mut ui, named(&nodes, "Distribute and tidy").unwrap());
    let menu = tree(&mut ui);
    assert!(named(&menu, "Distribute horizontal spacing").is_some());
    assert!(named(&menu, "Distribute vertical spacing").is_some());
    assert!(named(&menu, "Tidy up").is_none());
}

#[test]
fn issue_2109_ui_existing_alt_d_shortcut_aligns_selected_points_right() {
    let (mut ui, id) = fixture(&[0, 1]);
    let modifiers = Modifiers::ALT;
    ui.frame_with(
        vec![
            Event::Key { key: Key::D, physical_key: Some(Key::D), pressed: true, repeat: false, modifiers },
            Event::Key { key: Key::D, physical_key: Some(Key::D), pressed: false, repeat: false, modifiers },
        ],
        modifiers,
    );
    let x = xs(&ui, id);
    assert!((x[0] - 80.0).abs() < 1e-6 && (x[1] - 80.0).abs() < 1e-6 && (x[2] - 140.0).abs() < 1e-6);
}

#[test]
fn issue_2109_ui_segment_endpoints_in_other_layer_enable_distribution() {
    let mut s = Session::new();
    let a = path(&mut s, json!([[0, 0], [70, 60]]));
    let b = path(&mut s, json!([[210, 140], [280, 200]]));
    s.execute("vector.edit.begin", &json!({"ids":[a,b]})).unwrap();
    s.execute("vector.edit.select", &json!({"id":a,"segments":[0],"vertices":[1]})).unwrap();
    s.execute("vector.edit.select", &json!({"id":b,"vertices":[0],"add":true})).unwrap();
    let mut ui = Ui::new(s);
    let nodes = tree(&mut ui);
    click(&mut ui, named(&nodes, "Distribute and tidy").unwrap());
    let menu = tree(&mut ui);
    click(&mut ui, named(&menu, "Distribute horizontal spacing").unwrap());
    assert_eq!(xs(&ui, a), [0.0, 105.0]);
    assert_eq!(xs(&ui, b), [210.0, 280.0]);
}

#[test]
fn issue_2109_ui_other_selected_points_keep_row_with_empty_active_layer() {
    let mut s = Session::new();
    let a = path(&mut s, json!([[10, 40], [70, 100], [100, 140]]));
    let b = path(&mut s, json!([[200, 0], [250, 30]]));
    s.execute("vector.edit.begin", &json!({"ids":[a,b]})).unwrap();
    s.execute("vector.edit.select", &json!({"id":a,"vertices":[0,1]})).unwrap();
    s.execute("vector.edit.select", &json!({"id":b,"vertices":[],"add":true})).unwrap();
    let mut ui = Ui::new(s);
    let before = xs(&ui, b);
    let nodes = tree(&mut ui);
    click(&mut ui, named(&nodes, "Align right").unwrap());
    assert_eq!(xs(&ui, a), [70.0, 70.0, 100.0]);
    assert_eq!(xs(&ui, b), before);
}

#[test]
fn issue_2109_ui_empty_selection_and_width_tool_keep_the_existing_hint_panel() {
    let (mut ui, _) = fixture(&[]);
    assert!(named(&tree(&mut ui), "Align left").is_none());
    ui.app.session.execute("vector.edit.select", &json!({"vertices":[0,1,2]})).unwrap();
    ui.app.session.execute("vector.edit.tool", &json!({"tool":"width"})).unwrap();
    assert!(named(&tree(&mut ui), "Align left").is_none());
}
