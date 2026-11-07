//! #1205: the actual Layers row follows the rectangle's corner edits without reselecting it.
use crate::layers_drag_tests::Ui;
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

fn row_glyph(ui: &mut Ui, id: NodeId) -> Vec<String> {
    let row = ui.row_at(id);
    ui.frame_output(vec![])
        .shapes
        .into_iter()
        .filter_map(|c| {
            let b = c.shape.visual_bounding_rect();
            let small = b.width() <= 20.0 && b.height() <= 20.0;
            (small && b.left() > 56.0 && b.right() < 200.0 && (b.center().y - row.y).abs() < 5.0).then(|| format!("{:?}", c.shape))
        })
        .collect()
}

#[test]
fn issue_1205_layers_preview_tracks_corner_radius_edits_and_undo() {
    let mut session = Session::new();
    let id = NodeId(session.execute("shape.rectangle", &json!({"x":0,"y":0,"width":100,"height":100})).unwrap()["id"].as_u64().unwrap());
    let mut ui = Ui::new(session);
    let plain = row_glyph(&mut ui, id);
    assert!(!plain.is_empty(), "the layer's row draws its glyph");
    ui.app.session.execute("node.set", &json!({"id":id,"props":{"cornerRadius":30}})).unwrap();
    let rounded = row_glyph(&mut ui, id);
    assert_ne!(rounded, plain, "the visible layer preview must reflect the corner-radius edit immediately");
    ui.app.session.execute("node.set", &json!({"id":id,"props":{"topLeftRadius":0}})).unwrap();
    assert_ne!(row_glyph(&mut ui, id), rounded, "individual corners must also be visible in the preview");
    ui.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(row_glyph(&mut ui, id), rounded);
    ui.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(row_glyph(&mut ui, id), plain);
}
