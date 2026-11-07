//! Offline preparation of the exact print PDF; this command never contacts a printer.

use base64::Engine as _;
use figcraft_doc::NodeId;
use figcraft_pdf::{Fit, FitOrientation, FitScale};
use serde_json::{Value, json};

use crate::{Result, Session, bad};

const CMD: &str = "file.printPdf";

fn word<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match p.get(key) {
        None => Ok(default),
        Some(Value::String(s)) => Ok(s),
        _ => Err(bad(CMD, format!("`{key}` must be a string"))),
    }
}

fn paper(value: Option<&Value>) -> Result<Option<(f64, f64)>> {
    let size = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
            "a3" => (841.89, 1190.55),
            "a4" => (595.28, 841.89),
            "a5" => (419.53, 595.28),
            "letter" => (612.0, 792.0),
            "legal" => (612.0, 1008.0),
            "tabloid" => (792.0, 1224.0),
            _ => return Err(bad(CMD, "unknown paper; use A3, A4, A5, Letter, Legal, Tabloid or [width,height]")),
        },
        Some(Value::Array(a)) if a.len() == 2 => {
            let number = |i| a.get(i).and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0 && *n <= 14_400.0);
            let (Some(w), Some(h)) = (number(0), number(1)) else {
                return Err(bad(CMD, "paper sides must be finite positive points, at most 14400"));
            };
            (w, h)
        }
        _ => return Err(bad(CMD, "paper must be a name or [width,height]")),
    };
    Ok(Some(size))
}

pub(super) fn print_pdf(s: &mut Session, p: &Value) -> Result<Value> {
    let object = p.as_object().ok_or_else(|| bad(CMD, "parameters must be an object"))?;
    for key in object.keys() {
        if !["ids", "paper", "orientation", "scale", "margin", "grayscale", "path"].contains(&key.as_str()) {
            return Err(bad(CMD, format!("unknown option `{key}`")));
        }
    }
    let page = s.page()?;
    let doc = s.doc()?;
    let all = super::printable_frames(doc, page, None);
    let frames = match p.get("ids") {
        Some(Value::Array(ids)) if !ids.is_empty() && ids.len() <= 10_000 => {
            let mut frames = Vec::with_capacity(ids.len());
            for id in ids {
                let id = id.as_u64().map(NodeId).ok_or_else(|| bad(CMD, "ids must contain integer layer ids"))?;
                if !all.contains(&id) || frames.contains(&id) {
                    return Err(bad(CMD, "ids must name unique visible top-level frames on the current page"));
                }
                frames.push(id);
            }
            frames
        }
        None => {
            let selected = super::printable_frames(doc, page, Some(&s.selection()));
            if selected.is_empty() { all } else { selected }
        }
        _ => return Err(bad(CMD, "ids must be a nonempty array of at most 10000 frame ids")),
    };
    let paper = paper(p.get("paper"))?;
    if paper.is_none() && ["orientation", "scale", "margin"].iter().any(|key| p.get(*key).is_some()) {
        return Err(bad(CMD, "orientation, scale and margin require paper"));
    }
    let orientation = match word(p, "orientation", "auto")? {
        "auto" => FitOrientation::Auto,
        "portrait" => FitOrientation::Portrait,
        "landscape" => FitOrientation::Landscape,
        _ => return Err(bad(CMD, "orientation must be auto, portrait or landscape")),
    };
    let scale = match word(p, "scale", "shrink")? {
        "shrink" => FitScale::Shrink,
        "fit" => FitScale::Fill,
        _ => return Err(bad(CMD, "scale must be shrink or fit")),
    };
    let margin = match p.get("margin") {
        None => paper.map_or(18.0, |(w, h)| 18.0_f64.min(w.min(h) / 4.0)),
        Some(v) => v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(|| bad(CMD, "margin must be finite nonnegative points"))?,
    };
    if paper.is_some_and(|(w, h)| margin > w.min(h) / 4.0) {
        return Err(bad(CMD, "margin must not exceed one quarter of the short paper side"));
    }
    let grayscale = match p.get("grayscale") {
        None => false,
        Some(Value::Bool(v)) => *v,
        _ => return Err(bad(CMD, "grayscale must be a boolean")),
    };
    let path = match p.get("path") {
        None => None,
        Some(Value::String(path)) if !path.trim().is_empty() => Some(path),
        _ => return Err(bad(CMD, "path must be a nonempty string")),
    };
    let fit = paper.map(|paper| Fit { paper, orientation, scale, margin });
    let plan = super::print_plan(doc, &frames, fit, grayscale)?;
    let sheets: Vec<Value> = plan
        .sheets
        .iter()
        .map(|sheet| json!({"frame":sheet.frame,"width":sheet.sheet.0,"height":sheet.sheet.1,"scale":sheet.scale,"offset":sheet.offset}))
        .collect();
    let mut result = json!({"format":"pdf","bytes":plan.pdf.len(),"frames":frames.len(),"grayscale":grayscale,"sheets":sheets});
    if let Some(path) = path {
        super::load::write(path, &plan.pdf)?;
        result["path"] = json!(path);
    } else {
        result["data"] = json!(base64::engine::general_purpose::STANDARD.encode(&plan.pdf));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Session, NodeId) {
        let mut s = Session::new();
        let f = s.execute("frame.create", &json!({"width":200,"height":100,"name":"Print red"})).unwrap()["id"].as_u64().map(NodeId).unwrap();
        s.execute("fill.set", &json!({"ids":[f],"color":"#ff0000"})).unwrap();
        s.set_selection(vec![f]);
        (s, f)
    }

    #[test]
    fn print_pdf_prepares_gray_sheets_and_writes_the_same_offline_document() {
        let (mut s, f) = session();
        let revision = s.ds().unwrap().revision;
        let color = s.execute(CMD, &json!({"ids":[f],"paper":"A4"})).unwrap();
        let gray = s.execute(CMD, &json!({"ids":[f],"paper":"A4","grayscale":true})).unwrap();
        let decode = |r: &Value| base64::engine::general_purpose::STANDARD.decode(r["data"].as_str().unwrap()).unwrap();
        let bytes = decode(&gray);
        assert!(bytes.starts_with(b"%PDF"));
        assert_ne!(bytes, decode(&color));
        assert_eq!(gray["frames"], 1);
        assert_eq!(gray["sheets"][0]["width"], 841.89);
        assert_eq!(gray["sheets"][0]["height"], 595.28);
        assert!(!String::from_utf8_lossy(&bytes).contains("/Annots"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("print grayscale.pdf");
        let result = s.execute(CMD, &json!({"ids":[f],"paper":"A4","grayscale":true,"path":path})).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(result.get("data").is_none());
        assert_eq!(s.ds().unwrap().revision, revision, "preparation never edits the design");
    }

    #[test]
    fn malformed_print_options_fail_before_output_or_document_effects() {
        let (mut s, f) = session();
        let before = serde_json::to_value(s.doc().unwrap()).unwrap();
        for p in [
            json!({"grayscale":"yes"}),
            json!({"paper":[0,10]}),
            json!({"paper":[10,"20"]}),
            json!({"paper":"unknown"}),
            json!({"paper":"A4","margin":300}),
            json!({"paper":"A4","orientation":"up"}),
            json!({"paper":"A4","scale":true}),
            json!({"margin":0}),
            json!({"paper":"A4","margin":-1}),
            json!({"ids":[f,f]}),
            json!({"ids":[-1]}),
            json!({"ids":[]}),
            json!({"path":null}),
            json!({"greyscale":true}),
        ] {
            assert!(s.execute(CMD, &p).is_err(), "{p}");
            assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before, "{p}");
        }
        assert!(s.execute(CMD, &json!({"paper":[10,10]})).is_ok(), "a valid small paper gets a bounded default margin");
    }

    #[test]
    fn print_ids_must_belong_to_visible_top_level_frames_on_this_page() {
        let (mut s, f) = session();
        let child = s.execute("shape.rectangle", &json!({"parent":f,"width":20,"height":20})).unwrap()["id"].clone();
        assert!(s.execute(CMD, &json!({"ids":[child]})).is_err());
        s.execute("page.add", &json!({"name":"Other"})).unwrap();
        assert!(s.execute(CMD, &json!({"ids":[f]})).is_err(), "a previously selected frame cannot cross a page boundary");
    }
}
