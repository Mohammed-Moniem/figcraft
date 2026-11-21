use figcraft_doc::LibraryLink;
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, has_text, text_center};

const LINK: &str = "ws://127.0.0.1:9/?file=kit&team=1";
const REVIEW: &str = "Review library updates";

#[test]
fn file_panel_offers_review_only_for_changes_to_used_assets_from_an_approved_library() {
    let mut publisher = Session::new();
    let button = publisher.execute("node.create", &json!({"type":"component","name":"Button"})).unwrap()["id"].clone();
    let unused = publisher.execute("node.create", &json!({"type":"component","name":"Unused"})).unwrap()["id"].clone();
    let published = publisher.execute("library.publish", &json!({})).unwrap();
    let id = published["id"].as_str().unwrap();
    let mut consumer = Session::new();
    consumer.doc_mut().unwrap().libraries_mut().enabled.push(LibraryLink { id: id.into(), name: "Team Kit".into(), path: LINK.into(), version: 1 });
    consumer.libraries.insert(LINK, Ok(publisher.doc().unwrap().published.as_ref().unwrap().clone()));
    consumer.execute("library.place", &json!({"library":id,"component":button})).unwrap();
    let mut ui = Ui::new(consumer);

    publisher.execute("node.set", &json!({"id":unused,"props":{"fill":"#FD7C60"}})).unwrap();
    publisher.execute("library.publish", &json!({})).unwrap();
    let unused_change = publisher.doc().unwrap().published.as_ref().unwrap().clone();
    assert_eq!(unused_change.manifest.version, 2);
    publisher.execute("node.set", &json!({"id":button,"props":{"fill":"#487ACC"}})).unwrap();
    publisher.execute("library.publish", &json!({})).unwrap();
    let used_change = publisher.doc().unwrap().published.as_ref().unwrap().clone();
    assert_eq!(used_change.manifest.version, 3);
    ui.app.session.libraries.insert(LINK, Ok(used_change.clone()));
    assert!(!ui.app.session.library_allowed(LINK));
    assert!(!has_text(&flat(&ui.frame_output(vec![]).shapes), REVIEW));

    ui.app.session.execute("library.check", &json!({"library":id})).unwrap();
    ui.app.session.libraries.insert(LINK, Ok(unused_change));
    assert!(!has_text(&flat(&ui.frame_output(vec![]).shapes), REVIEW));
    ui.app.session.libraries.insert(LINK, Ok(used_change));
    let review = text_center(&flat(&ui.frame_output(vec![]).shapes), REVIEW);
    ui.button(review, true);
    ui.button(review, false);
    assert_eq!(ui.app.ui.left_tab, crate::LeftTab::Assets);
    assert!(has_text(&flat(&ui.frame_output(vec![]).shapes), "Library updates available"));
    ui.app.session.execute("library.acceptUpdates", &json!({})).unwrap();
    ui.app.ui.left_tab = crate::LeftTab::File;
    assert!(!has_text(&flat(&ui.frame_output(vec![]).shapes), REVIEW));
}
