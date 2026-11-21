#![cfg(not(target_arch = "wasm32"))]
use super::*;
use figcraft_doc::LibraryLink;
use figcraft_engine::Session;
use std::path::{Path, PathBuf};

struct Fixture {
    root: PathBuf,
    pages: Vec<(PathBuf, String, String)>,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("figcraft-2122-ui-{}-{tag}", std::process::id()));
        assert!(!root.exists());
        let mut pages = Vec::new();
        for name in ["First", "Second"] {
            let lib = root.join(name).join("lib/Library.figcraft");
            let page = root.join(name).join("pages/Page.figcraft");
            std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
            std::fs::create_dir_all(page.parent().unwrap()).unwrap();
            let mut publisher = Session::new();
            let component =
                publisher.execute("node.create", &json!({"type":"component","name":format!("{name} component"),"width":80,"height":60})).unwrap();
            publisher.execute("style.create", &json!({"type":"paint","name":format!("{name} style"),"from":component["id"]})).unwrap();
            publisher.execute("variable.createCollection", &json!({"name":format!("{name} tokens")})).unwrap();
            publisher
                .execute(
                    "variable.create",
                    &json!({"collection":format!("{name} tokens"),"name":format!("{name} spacing"),"type":"number","value":8}),
                )
                .unwrap();
            let published = publisher.execute("library.publish", &json!({})).unwrap();
            publisher.execute("file.saveAs", &json!({"path":lib.to_str().unwrap()})).unwrap();
            let id = published["id"].as_str().unwrap().to_string();
            let mut doc = Document::new(name);
            doc.libraries_mut().enabled.push(LibraryLink {
                id: id.clone(),
                name: name.into(),
                path: Path::new("..").join("lib/Library.figcraft").to_string_lossy().into(),
                version: 1,
            });
            std::fs::write(&page, figcraft_format::save_bytes(&doc, None).unwrap()).unwrap();
            pages.push((page, id, name.to_string()));
        }
        assert_ne!(pages[0].1, pages[1].1);
        Self { root, pages }
    }
    fn open(&self, app: &mut FigApp, index: usize) {
        let (page, id, _) = &self.pages[index];
        app.session.execute("file.open", &json!({"path":page.to_str().unwrap()})).unwrap();
        let listed = app.session.execute("library.list", &json!({})).unwrap();
        assert!(listed["libraries"][0].get("error").is_none(), "{listed}");
        assert_eq!(listed["libraries"][0]["id"], *id);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn issue_2122_review_asset_listing_reads_the_correct_relative_library_components_and_styles() {
    let f = Fixture::new("assets");
    let mut app = FigApp::new(Session::empty(), Box::new(crate::NoServices));
    for index in [0, 1, 0, 1] {
        f.open(&mut app, index);
        let doc = app.session.doc().unwrap();
        let listing = build_listing(&app, doc, "", (0, String::new(), 0));
        assert_eq!(listing.libraries.len(), 1);
        let section = &listing.libraries[0];
        assert_eq!(section.id, f.pages[index].1);
        assert!(section.doc.is_some(), "successfully loaded relative library must appear in Assets");
        assert!(!section.tiles.is_empty());
        assert_eq!(section.styles.len(), 1);
        assert_eq!(section.styles[0].name, format!("{} style", f.pages[index].2));
    }
}

#[test]
fn issue_2122_review_variable_picker_reads_the_correct_relative_library_collections() {
    let f = Fixture::new("variables");
    let mut app = FigApp::new(Session::empty(), Box::new(crate::NoServices));
    for index in [0, 1, 0, 1] {
        f.open(&mut app, index);
        let doc = app.session.doc().unwrap();
        let libs = crate::panels::variable_ui::libraries(&app, doc);
        assert_eq!(libs.len(), 1, "successfully loaded relative library must be visible to variable pickers");
        assert_eq!(libs[0].manifest.id, f.pages[index].1);
        let groups = crate::panels::variable_ui::groups(doc, &libs, |_, _| true);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.name, format!("{} tokens", f.pages[index].2));
        assert_eq!(groups[0].2[0].name, format!("{} spacing", f.pages[index].2));
    }
}
