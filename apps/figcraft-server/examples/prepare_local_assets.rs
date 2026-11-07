//! Creates synthetic library/plugin fixtures only inside an explicit empty directory.
use figcraft_engine::{Session, library_server};
use figcraft_plugin::bundle::Bundle;
use figcraft_server::{config::Config, store::Store};
use serde_json::json;
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::path::PathBuf::from(std::env::args().nth(1).ok_or("data directory required")?);
    if !data.is_absolute() || data.exists() {
        return Err("fixture needs a new absolute data directory".into());
    }
    let cfg = Config { data: data.clone(), directory_only: true, ..Config::default() };
    let mut s = Session::new();
    let r = s.execute("shape.rectangle", &json!({"width":40,"height":30}))?;
    s.execute("style.create", &json!({"type":"paint","name":"Fixture Brand","from":r["id"]}))?;
    s.execute("library.publish", &json!({}))?;
    let mut store = Store::open(&data, cfg.store())?;
    let snapshot = figcraft_collab::Replica::new(s.doc()?.clone()).snapshot()?;
    let (file, edit, view) = store.publish("Fixture local library", &snapshot)?;
    store.publish_library(&file.id, &library_server::archive(s.doc()?.published.as_ref().ok_or("library missing")?)?)?;
    let plugin = "org.example.local-assets-fixture";
    let manifest=json!({"id":plugin,"name":"Local assets fixture","version":"1.0.0","main":"module.wasm","permissions":{"access":"write","commands":["shape.rectangle"]}}).to_string();
    let wasm = wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"figcraft_run\") (param i32) (result i32) i32.const 0))")?;
    store.directory().publish(&Bundle { manifest, wasm, assets: vec![] }.encode())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(data.join("fixture-private.json"))?;
    out.write_all(&serde_json::to_vec(&json!({"file":file.id,"edit":edit.token,"view":view.token,"plugin":plugin}))?)?;
    out.sync_all()?;
    println!("Synthetic local assets fixture prepared.");
    Ok(())
}
