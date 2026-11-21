use super::*;
use figcraft_doc::LibraryLink;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    page: PathBuf,
    library: PathBuf,
    library_id: String,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("figcraft-2122-cli-{}-{tag}", std::process::id()));
        assert!(!root.exists());
        std::fs::create_dir_all(root.join("project/lib")).unwrap();
        std::fs::create_dir_all(root.join("project/pages")).unwrap();
        std::fs::create_dir_all(root.join("project/archive/deep")).unwrap();
        let library = root.join("project/lib/Library.figcraft");
        let page = root.join("project/pages/Page.figcraft");
        let mut publisher = Session::new();
        publisher.execute("node.create", &json!({"type":"component","name":"Sidebar","width":200,"height":300})).unwrap();
        let manifest = publisher.execute("library.publish", &json!({})).unwrap();
        publisher.execute("file.saveAs", &json!({"path":library.to_str().unwrap()})).unwrap();
        let library_id = manifest["id"].as_str().unwrap().to_string();
        let mut doc = Document::new("Portable consumer");
        doc.libraries_mut().enabled.push(LibraryLink {
            id: library_id.clone(),
            name: "Untitled".into(),
            path: Path::new("..").join("lib/Library.figcraft").to_string_lossy().into(),
            version: 1,
        });
        std::fs::write(&page, figcraft_format::save_bytes(&doc, None).unwrap()).unwrap();
        Self { root, page, library, library_id }
    }

    fn output(&self, extension: &str) -> PathBuf {
        self.root.join(format!("project/archive/deep/Copy.{extension}"))
    }

    fn assert_link(&self, output: &Path) {
        let (doc, issues) = figcraft_format::load_bytes(&std::fs::read(output).unwrap()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
        let link = &doc.libraries.enabled[0];
        assert_eq!(link.id, self.library_id);
        assert!(!Path::new(&link.path).is_absolute());
        assert_eq!(std::fs::canonicalize(output.parent().unwrap().join(&link.path)).unwrap(), std::fs::canonicalize(&self.library).unwrap());
        let mut reopened = Session::empty();
        reopened.execute("file.open", &json!({"path":output.to_str().unwrap()})).unwrap();
        let listed = reopened.execute("library.list", &json!({})).unwrap();
        assert!(listed["libraries"][0].get("error").is_none(), "{listed}");
        assert_eq!(listed["libraries"][0]["id"], self.library_id);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn issue_2122_review_convert_native_output_rebases_the_actual_input_origin() {
    let f = Fixture::new("convert-native");
    let out = f.output("figcraft");
    assert_eq!(convert(f.page.to_str().unwrap(), out.to_str().unwrap()).unwrap(), ExitCode::SUCCESS);
    f.assert_link(&out);
}

#[test]
fn issue_2122_review_convert_json_output_rebases_the_actual_input_origin() {
    let f = Fixture::new("convert-json");
    let out = f.output("json");
    assert_eq!(convert(f.page.to_str().unwrap(), out.to_str().unwrap()).unwrap(), ExitCode::SUCCESS);
    f.assert_link(&out);
}

fn script_output(extension: &str) {
    let f = Fixture::new(&format!("script-{extension}"));
    let script = f.root.join("script.json");
    std::fs::write(&script, b"[]").unwrap();
    let out = f.output(extension);
    assert_eq!(run_script(script.to_str().unwrap(), Some(f.page.to_str().unwrap()), Some(out.to_str().unwrap())).unwrap(), ExitCode::SUCCESS);
    f.assert_link(&out);
}

#[test]
fn issue_2122_review_script_json_output_uses_the_active_documents_retained_origin() {
    script_output("json");
}

#[test]
fn issue_2122_review_script_native_output_already_preserves_its_origin() {
    script_output("figcraft");
}

#[test]
fn issue_2122_review_unknown_origin_writer_preserves_relative_spelling_without_cwd_resolution() {
    let f = Fixture::new("unknown-origin");
    let (doc, _) = figcraft_format::load_bytes(&std::fs::read(&f.page).unwrap()).unwrap();
    let raw = doc.libraries.enabled[0].path.clone();
    let out = f.output("json");
    write_doc(&doc, doc.first_page().unwrap(), out.to_str().unwrap()).unwrap();
    let (decoded, _) = figcraft_format::load_bytes(&std::fs::read(out).unwrap()).unwrap();
    assert_eq!(decoded.libraries.enabled[0].path, raw);
}
