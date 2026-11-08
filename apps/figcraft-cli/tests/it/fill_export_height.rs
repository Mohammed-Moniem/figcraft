//! Original #2034 controls: the same .fig's converted geometry and rasterized Fill bar agree.
use std::path::{Path, PathBuf};
use std::process::Command;

use figcraft_doc::{Document, NodeId, Sizing};
use figcraft_engine::Session;
use figcraft_render::Pixmap;
use serde_json::{Value, json};

const SHORT: &str = "Short";
const LONG: &str = "Original first line\nOriginal second line";

fn command(session: &mut Session, id: &str, params: Value) -> Value {
    session.execute(id, &params).unwrap()
}

fn set(session: &mut Session, id: NodeId, props: Value) {
    command(session, "node.set", json!({"id":id,"props":props}));
}

fn fixture(text: &str) -> (Session, NodeId, NodeId, NodeId) {
    assert!(figcraft_text::find_face("Inter", "Regular").is_some(), "this measurement requires the pinned embedded Inter Regular face");
    let mut session = Session::new();
    let row = NodeId(command(&mut session, "frame.create", json!({"name":"Original row","width":220,"height":100}))["id"].as_u64().unwrap());
    set(&mut session, row, json!({"layout":"horizontal","padding":0,"gap":10,"fills":["#FFFFFF"],"sizingV":"hug"}));
    let bar =
        NodeId(
            command(
                &mut session,
                "shape.rectangle",
                json!({"parent":row,"width":10,"height":20,"props":{"name":"Original accent","fill":"#FF0000"}}),
            )["id"]
                .as_u64()
                .unwrap(),
        );
    let copy = NodeId(
        command(
            &mut session,
            "text.create",
            json!({"parent":row,"text":text,"props":{"name":"Original copy","fontFamily":"Inter","fontSize":20,"fill":"#000000"}}),
        )["id"]
            .as_u64()
            .unwrap(),
    );
    set(&mut session, copy, json!({"width":200,"textAutoResize":"height"}));
    set(&mut session, bar, json!({"sizingV":"fill"}));
    (session, row, bar, copy)
}

fn named<'a>(doc: &'a Document, name: &str) -> &'a figcraft_doc::Node {
    doc.nodes().find(|node| node.name == name).unwrap()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.001, "{a} != {b}");
}

fn run(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_figcraft-cli")).arg("--no-system-fonts").args(args).output().unwrap();
    assert!(output.status.success(), "{args:?}: {}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

fn red_extent(path: &Path) -> (u32, u32) {
    let image = Pixmap::decode_png(&std::fs::read(path).unwrap()).unwrap();
    let mut first = image.height();
    let mut last = 0;
    for (i, rgba) in image.data().chunks_exact(4).enumerate() {
        if rgba[0] > 250 && rgba[1] < 5 && rgba[2] < 5 && rgba[3] > 250 {
            let y = i as u32 / image.width();
            first = first.min(y);
            last = last.max(y);
        }
    }
    assert!(first < image.height(), "the original red accent must actually render: {}", path.display());
    (first, last + 1)
}

fn output_directory() -> (PathBuf, bool) {
    if let Some(path) = std::env::var_os("FIGCRAFT_FILL_EXPORT_EVIDENCE_DIR") {
        let path = PathBuf::from(path);
        std::fs::create_dir(&path).unwrap(); // Evidence is fresh, never adopted or overwritten.
        return (path, true);
    }
    for counter in 0..100 {
        let path = std::env::temp_dir().join(format!("figcraft-original-fill-export-{}-{counter}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return (path, false),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("fresh fixture directory: {error}"),
        }
    }
    panic!("cannot create a fresh original fixture directory")
}

#[test]
fn issue_2034_same_fig_json_and_png_agree_after_fill_parent_growth() {
    let (output, keep) = output_directory();
    for case in ["hug-follows-text", "fixed-parent-grows", "restore-hug-after-text", "final-text-first", "hug-font-grows"] {
        let (mut session, row, bar, copy) = fixture(if case == "final-text-first" { LONG } else { SHORT });
        // Cross-axis Fill retains Hug. Fixed-parent controls choose Fixed explicitly.
        let initially_fixed = matches!(case, "fixed-parent-grows" | "restore-hug-after-text");
        if initially_fixed {
            set(&mut session, row, json!({"sizingV":"fixed"}));
        }
        let initial = session.doc().unwrap().node(row).unwrap().height;
        assert_eq!(session.doc().unwrap().node(row).unwrap().child_layout.sizing_v, if initially_fixed { Sizing::Fixed } else { Sizing::Hug });
        assert_eq!(session.doc().unwrap().node(bar).unwrap().child_layout.sizing_v, Sizing::Fill);
        match case {
            "hug-follows-text" => {
                set(&mut session, copy, json!({"characters":LONG}));
                assert!(session.doc().unwrap().node(copy).unwrap().height > initial + 10.0);
                close(session.doc().unwrap().node(row).unwrap().height, session.doc().unwrap().node(copy).unwrap().height);
            }
            "fixed-parent-grows" => set(&mut session, row, json!({"height":initial+57.0})),
            "restore-hug-after-text" => {
                set(&mut session, copy, json!({"characters":LONG}));
                close(session.doc().unwrap().node(row).unwrap().height, initial);
                assert!(session.doc().unwrap().node(copy).unwrap().height > initial + 10.0);
                set(&mut session, row, json!({"sizingV":"hug"}));
                assert_eq!(session.doc().unwrap().node(bar).unwrap().child_layout.sizing_v, Sizing::Fill);
                close(session.doc().unwrap().node(bar).unwrap().height, session.doc().unwrap().node(copy).unwrap().height);
            }
            "hug-font-grows" => {
                set(&mut session, copy, json!({"fontSize":40}));
                assert_eq!(session.doc().unwrap().node(bar).unwrap().child_layout.sizing_v, Sizing::Fill);
                close(session.doc().unwrap().node(bar).unwrap().height, session.doc().unwrap().node(copy).unwrap().height);
            }
            _ => assert!(initial > 35.0, "the original final text must have multiple lines"),
        }
        let doc = session.doc().unwrap();
        let expected = doc.node(row).unwrap().height;
        close(doc.node(bar).unwrap().height, expected);
        assert_eq!(doc.node(bar).unwrap().child_layout.sizing_v, Sizing::Fill);
        assert_eq!(doc.node(row).unwrap().child_layout.sizing_v, if case == "fixed-parent-grows" { Sizing::Fixed } else { Sizing::Hug });
        if case != "final-text-first" {
            assert!(expected > initial + 10.0, "{case}: the parent must actually grow");
        }
        let dir = output.join(case);
        std::fs::create_dir(&dir).unwrap();
        let fig = dir.join("original.fig");
        std::fs::write(&fig, figcraft_engine::cmd::file::fig_bytes(doc).unwrap().0).unwrap();
        let json = dir.join("converted.json");
        let rendered = dir.join("rendered.png");
        let exported = dir.join("exported.png");
        let from_json = dir.join("rendered-json.png");
        run(&["convert", fig.to_str().unwrap(), json.to_str().unwrap()]);
        run(&["render", fig.to_str().unwrap(), "-o", rendered.to_str().unwrap()]);
        run(&["export", fig.to_str().unwrap(), "-o", exported.to_str().unwrap()]);
        run(&["render", json.to_str().unwrap(), "-o", from_json.to_str().unwrap()]);
        let converted = figcraft_format::from_json(&std::fs::read_to_string(&json).unwrap()).unwrap().0;
        close(named(&converted, "Original row").height, expected);
        close(named(&converted, "Original accent").height, expected);
        let extents = [&rendered, &exported, &from_json].map(|path| red_extent(path));
        for &(first, end) in &extents {
            assert!((f64::from(end - first) - expected).abs() <= 1.0, "{case}: JSON height{expected}, red extent{first}..{end}");
        }
        let pixels = Pixmap::decode_png(&std::fs::read(&rendered).unwrap()).unwrap();
        let json_pixels = Pixmap::decode_png(&std::fs::read(&from_json).unwrap()).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (json_pixels.width(), json_pixels.height()));
        assert_eq!(pixels.data(), json_pixels.data(), "{case}: the same geometry converted to JSON must rasterize identically");
        eprintln!("{}", json!({"case":case,"initialRow":initial,"finalRow":expected,"barHeight":doc.node(bar).unwrap().height,"redExtents":extents}));
    }
    if !keep {
        std::fs::remove_dir_all(output).unwrap();
    }
}
