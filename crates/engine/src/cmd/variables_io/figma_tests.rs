use crate::Session;
use figcraft_doc::{DesignTokenType, VariableType, VariableValue};
use serde_json::{Value, json};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}
fn sample() -> Value {
    json!({
        "paint":{"accent":{"$type":"color","$value":{"colorSpace":"srgb","components":[0.1,0.4,0.7],"alpha":0.8}}},
        "enabled":{"$type":"number","$value":2,"$extensions":{"com.figma.type":"boolean"}},
        "enabled-copy":{"$type":"number","$value":"{enabled}","$extensions":{"com.figma.type":"boolean"}},
        "label":{"$type":"string","$value":"Original synthetic caption"},
        "font":{"$type":"fontFamily","$value":"Synthetic Font"},
        "spacing":{"$type":"dimension","$value":{"value":12,"unit":"px"}},
        "transition":{"$type":"duration","$value":{"value":0.4,"unit":"s"}}
    })
}
fn output(s: &mut Session, p: Value) -> Value {
    serde_json::from_str(run(s, "variable.export", p)["text"].as_str().unwrap()).unwrap()
}

#[test]
fn figma_native_export_string_type_extensions_and_aliases_import() {
    let mut s = Session::new();
    let input = json!({
        "caption":{"$type":"string","$value":"Original synthetic caption","$extensions":{"com.figma.type":"string","com.figma.variableId":"synthetic-foreign-id","com.figma.scopes":["ALL_SCOPES"]}},
        "font":{"$type":"string","$value":"Synthetic Font","$extensions":{"com.figma.type":"string"}},
        "copy":{"$type":"string","$value":"{caption}","$extensions":{"com.figma.type":"string"}},
        "$extensions":{"com.figma.modeName":"Original synthetic mode"}
    });
    run(&mut s, "variable.import", json!({"format":"figma","variables":input}));
    let vars = &s.doc().unwrap().variables;
    let caption = vars.get("caption").unwrap();
    assert_eq!(caption.kind, VariableType::String);
    assert_ne!(caption.id, "synthetic-foreign-id");
    assert_eq!(caption.values.values().next(), Some(&VariableValue::String("Original synthetic caption".into())));
    assert_eq!(vars.get("copy").unwrap().values.values().next(), Some(&VariableValue::Alias(caption.id.clone())));
    let out = output(&mut s, json!({"format":"figma"}));
    assert_eq!(out["font"]["$type"], "string");
    assert_eq!(out["font"]["$value"], "Synthetic Font");
    assert_eq!(out["copy"]["$value"], "{caption}");
}

#[test]
fn figma_native_export_millisecond_duration_and_aliases_normalize_to_seconds() {
    let mut s = Session::new();
    run(
        &mut s,
        "variable.import",
        json!({"format":"figma","variables":{
            "transition":{"$type":"duration","$value":{"value":400,"unit":"ms"}},
            "copy":{"$type":"duration","$value":"{transition}"}
        }}),
    );
    let vars = &s.doc().unwrap().variables;
    let transition = vars.get("transition").unwrap();
    assert_eq!(transition.design_token_type, Some(DesignTokenType::Duration));
    assert_eq!(transition.values.values().next(), Some(&VariableValue::Number(0.4)));
    assert_eq!(vars.get("copy").unwrap().values.values().next(), Some(&VariableValue::Alias(transition.id.clone())));
    let out = output(&mut s, json!({"format":"figma"}));
    assert_eq!(out["transition"]["$value"], json!({"value":0.4,"unit":"s"}));
    assert_eq!(out["copy"]["$type"], "duration");
    assert_eq!(out["copy"]["$value"], "{transition}");
}

#[test]
fn figma_mode_imports_documented_literals_and_typed_aliases() {
    let mut s = Session::new();
    let r = run(&mut s, "variable.import", json!({"variables":sample(),"format":"figma","collection":"Theme","mode":"Night"}));
    assert_eq!(r["variables"], 7);
    let vars = &s.doc().unwrap().variables;
    assert_eq!(vars.collections[0].name, "Theme");
    assert_eq!(vars.collections[0].modes[0].name, "Night");
    assert_eq!(vars.get("enabled").unwrap().kind, VariableType::Boolean);
    assert_eq!(vars.get("enabled").unwrap().values.values().next(), Some(&VariableValue::Boolean(true)));
    assert_eq!(vars.get("label").unwrap().kind, VariableType::String);
    assert_eq!(vars.get("label").unwrap().design_token_type, None);
    assert_eq!(vars.get("transition").unwrap().design_token_type, Some(DesignTokenType::Duration));
    assert_eq!(vars.get("transition").unwrap().values.values().next(), Some(&VariableValue::Number(0.4)));
    assert!(vars.get("paint/accent").is_some());
    assert!(matches!(vars.get("enabled-copy").unwrap().values.values().next(), Some(VariableValue::Alias(_))));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc().unwrap().variables.collections.is_empty());
}

#[test]
fn figma_auto_detection_is_separate_from_explicit_strict_dtcg() {
    let mut s = Session::new();
    assert!(s.execute("variable.import", &json!({"variables":sample(),"format":"dtcg"})).is_err());
    assert!(s.doc().unwrap().variables.variables.is_empty());
    run(&mut s, "variable.import", json!({"variables":sample()}));
    assert_eq!(s.doc().unwrap().variables.get("enabled").unwrap().kind, VariableType::Boolean);
}

#[test]
fn figma_mode_export_preserves_strings_booleans_durations_and_local_links() {
    let mut s = Session::new();
    run(&mut s, "variable.import", json!({"variables":sample(),"format":"figma"}));
    let out = output(&mut s, json!({"format":"figma"}));
    assert_eq!(out["enabled"]["$type"], "number");
    assert_eq!(out["enabled"]["$value"], 1);
    assert_eq!(out["enabled"]["$extensions"]["com.figma.type"], "boolean");
    assert_eq!(out["label"]["$type"], "string");
    assert_eq!(out["label"]["$value"], "Original synthetic caption");
    assert_eq!(out["transition"]["$value"], json!({"value":0.4,"unit":"s"}));
    assert_eq!(out["spacing"]["$value"], json!({"value":12.0,"unit":"px"}));
    assert_eq!(out["enabled-copy"]["$value"], "{enabled}");
    let mut target = Session::new();
    run(&mut target, "variable.import", json!({"variables":out,"format":"figma"}));
    assert_eq!(target.doc().unwrap().variables.variables.len(), 7);
    assert_eq!(target.doc().unwrap().variables.get("enabled").unwrap().kind, VariableType::Boolean);
}

#[test]
fn figma_mode_update_preserves_other_modes_and_existing_metadata() {
    let mut s = Session::new();
    run(&mut s, "variable.createCollection", json!({"name":"Theme","modes":["Day","Night"]}));
    run(
        &mut s,
        "variable.create",
        json!({"collection":"Theme","name":"spacing","type":"number","values":{"Day":4,"Night":8},"description":"Keep metadata"}),
    );
    let id = s.doc().unwrap().variables.get("spacing").unwrap().id.clone();
    {
        let var = s.doc_mut().unwrap().variables.variables.iter_mut().find(|v| v.id == id).unwrap();
        var.scopes = Some(vec!["GAP".into()]);
        var.code_syntax.insert("WEB".into(), "var(--spacing)".into());
        var.design_token_type = Some(DesignTokenType::Dimension);
    }
    let before = s.doc().unwrap().variables.get_by_id(&id).unwrap().clone();
    run(
        &mut s,
        "variable.import",
        json!({"format":"figma","collection":"Theme","mode":"Night","variables":{"spacing":{"$type":"number","$value":24}}}),
    );
    let vars = &s.doc().unwrap().variables;
    let v = vars.get_by_id(&id).unwrap();
    let c = vars.collection_by_id(&v.collection).unwrap();
    assert_eq!(v.values[&c.find_mode("Day").unwrap().id], VariableValue::Number(4.0));
    assert_eq!(v.values[&c.find_mode("Night").unwrap().id], VariableValue::Number(24.0));
    assert_eq!(
        (v.description.clone(), v.scopes.clone(), v.code_syntax.clone(), v.design_token_type),
        (before.description, before.scopes, before.code_syntax, before.design_token_type)
    );
}

#[test]
fn figma_external_aliases_resolve_local_targets_without_reimporting_them() {
    let mut s = Session::new();
    run(&mut s, "variable.createCollection", json!({"name":"Foundation"}));
    run(&mut s, "variable.create", json!({"collection":"Foundation","name":"spacing","type":"number","value":8}));
    let target = s.doc().unwrap().variables.get("spacing").unwrap().id.clone();
    let alias = json!({"space":{"$type":"number","$value":8,"$extensions":{"com.figma.aliasData":{"targetVariableSetName":"Foundation","targetVariableName":"spacing"}}}});
    let r = run(&mut s, "variable.import", json!({"format":"figma","collection":"Semantic","variables":alias}));
    assert_eq!(r["variables"], 1);
    assert_eq!(r["updated"], 0);
    assert_eq!(s.doc().unwrap().variables.get("space").unwrap().values.values().next(), Some(&VariableValue::Alias(target.clone())));
    let out = output(&mut s, json!({"format":"figma","collections":["Semantic"]}));
    assert_eq!(out["space"]["$value"], 8.0);
    assert_eq!(out["space"]["$extensions"]["com.figma.aliasData"]["targetVariableName"], "spacing");
}

#[test]
fn figma_rejects_cycles_wrong_types_units_metadata_and_ambiguous_names_atomically() {
    let cases = [
        json!({"a":{"$type":"number","$value":"{b}"},"b":{"$type":"number","$value":"{a}"}}),
        json!({"a":{"$type":"number","$value":1},"b":{"$type":"number","$value":"{a}","$extensions":{"com.figma.type":"boolean"}}}),
        json!({"a":{"$type":"duration","$value":{"value":10,"unit":"minute"}}}),
        json!({"a":{"$type":"dimension","$value":{"value":1,"unit":"rem"}}}),
        json!({"a":{"$type":"number","$value":1,"$extensions":{"ai.storyteller.figcraft":{"id":"v1"}}}}),
        json!({"a/b":{"$type":"number","$value":1},"a":{"b":{"$type":"number","$value":2}}}),
        json!({"a":{"$type":"number","$value":1,"$extensions":{"com.figma.aliasData":{"targetVariableName":"missing"}}}}),
    ];
    for case in cases {
        let mut s = Session::new();
        let before = serde_json::to_value(s.doc().unwrap()).unwrap();
        assert!(s.execute("variable.import", &json!({"format":"figma","variables":case})).is_err(), "{case}");
        assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
    }
}

#[test]
fn figma_native_export_dialect_keeps_conflicting_types_and_invalid_times_atomic() {
    let cases = [
        json!({"a":{"$type":"number","$value":1,"$extensions":{"com.figma.type":"string"}}}),
        json!({"a":{"$type":"string","$value":"text","$extensions":{"com.figma.type":"boolean"}}}),
        json!({"a":{"$type":"string","$value":"text","$extensions":{"com.figma.type":"unknown"}}}),
        json!({"a":{"$type":"string","$value":"text","$extensions":{"com.figma.type":false}}}),
        json!({"a":{"$type":"duration","$value":{"value":-1,"unit":"ms"}}}),
        json!({"a":{"$type":"duration","$value":{"value":-1,"unit":"s"}}}),
        json!({"a":{"$type":"duration","$value":{"value":null,"unit":"ms"}}}),
        json!({"a":{"$type":"duration","$value":{"value":"Infinity","unit":"ms"}}}),
        json!({"a":{"$type":"dimension","$value":{"value":400,"unit":"ms"}}}),
        json!({"a":{"$type":"string","$value":"caption"},"b":{"$type":"duration","$value":"{a}"}}),
    ];
    for case in cases {
        let mut s = Session::new();
        let before = serde_json::to_value(s.doc().unwrap()).unwrap();
        assert!(s.execute("variable.import", &json!({"format":"figma","variables":case})).is_err(), "{case}");
        assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
    }
}

#[test]
fn standard_dtcg_duration_uses_seconds_and_css_retains_time_units() {
    let mut s = Session::new();
    run(&mut s, "variable.import", json!({"format":"dtcg","variables":{"delay":{"$type":"duration","$value":{"value":1200,"unit":"ms"}}}}));
    let out = output(&mut s, json!({"format":"dtcg"}));
    assert_eq!(out["Tokens"]["delay"]["$value"], json!({"value":1.2,"unit":"s"}));
    let css = run(&mut s, "variable.export", json!({"format":"css"}));
    assert!(css["text"].as_str().unwrap().contains("1.2s"));
}

#[test]
fn figma_unknown_extensions_are_reported_and_deep_inputs_are_refused() {
    let mut s = Session::new();
    let r = run(
        &mut s,
        "variable.import",
        json!({"format":"figma","variables":{"a":{"$type":"number","$value":1,"$extensions":{"com.figma.sourceId":"foreign"}}}}),
    );
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    let mut deep = json!({"$type":"number","$value":1});
    for _ in 0..70 {
        deep = json!({"group":deep});
    }
    let before = serde_json::to_value(s.doc().unwrap()).unwrap();
    assert!(s.execute("variable.import", &json!({"format":"figma","variables":deep})).is_err());
    assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
}

#[test]
fn figma_external_alias_lookup_preserves_existing_collection_id_authority() {
    let mut s = Session::new();
    let empty = run(&mut s, "variable.createCollection", json!({"name":"Other"}))["id"].clone();
    run(&mut s, "variable.createCollection", json!({"name":"Foundation"}));
    run(&mut s, "variable.create", json!({"collection":"Foundation","name":"spacing","type":"number","value":8}));
    let mut params = json!({"format":"figma","collection":"Semantic","variables":{"space":{"$type":"number","$value":8,"$extensions":{"com.figma.aliasData":{"targetVariableID":"missing","targetVariableSetID":empty,"targetVariableSetName":"Foundation","targetVariableName":"spacing"}}}}});
    let before = serde_json::to_value(s.doc().unwrap()).unwrap();
    assert!(s.execute("variable.import", &params).is_err(), "an existing set ID must retain authority even if its named variable is absent");
    assert_eq!(serde_json::to_value(s.doc().unwrap()).unwrap(), before);
    params["variables"]["space"]["$extensions"]["com.figma.aliasData"]["targetVariableSetID"] = json!("missing-set");
    run(&mut s, "variable.import", params);
    let v = s.doc().unwrap().variables.get("space").unwrap();
    assert!(matches!(v.values.values().next(), Some(VariableValue::Alias(_))));
}

#[test]
fn figma_hsl_zero_and_negative_numeric_booleans_keep_documented_meanings() {
    let mut s = Session::new();
    run(
        &mut s,
        "variable.import",
        json!({"format":"figma","variables":{
            "off":{"$type":"number","$value":0,"$extensions":{"com.figma.type":"boolean"}},
            "on":{"$type":"number","$value":-3,"$extensions":{"com.figma.type":"boolean"}},
            "paint":{"$type":"color","$value":{"colorSpace":"hsl","components":[120,1,0.5],"alpha":0.5}}
        }}),
    );
    let out = output(&mut s, json!({"format":"figma"}));
    assert_eq!(out["off"]["$value"], 0);
    assert_eq!(out["on"]["$value"], 1);
    assert_eq!(out["paint"]["$value"]["colorSpace"], "srgb");
    assert_eq!(out["paint"]["$value"]["alpha"], 0.5);
}

#[test]
fn figma_export_is_confined_and_invalid_names_never_replace_existing_output() {
    use crate::automation::file_access::{Roots, confined};
    use std::sync::Arc;
    let dir = std::env::temp_dir().join(format!("figcraft-figma-mode-roots-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("allowed")).unwrap();
    let roots = Arc::new(Roots::new(&[], &[dir.join("allowed").to_string_lossy().into_owned()]).unwrap());
    let mut s = Session::new();
    run(&mut s, "variable.import", json!({"format":"figma","variables":sample()}));
    confined(Some(&roots), || {
        let error = s.execute("variable.export", &json!({"format":"figma","path":dir.join("outside.json")})).unwrap_err().to_string();
        assert!(error.contains("automation path rejected"), "{error}");
        run(&mut s, "variable.export", json!({"format":"figma","path":dir.join("allowed/output.json")}));
    });
    assert!(!dir.join("outside.json").exists());
    let original = std::fs::read(dir.join("allowed/output.json")).unwrap();
    s.doc_mut().unwrap().variables.variables[0].name = "invalid.name".into();
    assert!(s.execute("variable.export", &json!({"format":"figma","path":dir.join("allowed/output.json")})).is_err());
    assert_eq!(std::fs::read(dir.join("allowed/output.json")).unwrap(), original);
    let mut long = serde_json::Map::new();
    long.insert("a".repeat(4097), json!({"$type":"number","$value":1}));
    assert!(s.execute("variable.import", &json!({"format":"figma","variables":long})).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

// Existing native bindings/scopes retain unit authority over descriptive duration metadata.
#[test]
fn token_duration_css_native_roles_win_without_rewriting_metadata() {
    let cases = [
        ("scope opacity", Some("OPACITY"), None, ""),
        ("scope gap", Some("GAP"), None, "px"),
        ("bound opacity", None, Some("opacity"), ""),
        ("bound gap", None, Some("gap"), "px"),
        ("binding wins over scope", Some("OPACITY"), Some("gap"), "px"),
    ];
    let mut failures = Vec::new();
    for (label, scope, field, unit) in cases {
        let mut s = Session::new();
        run(&mut s, "variable.import", json!({"format":"dtcg","variables":{"delay":{"$type":"duration","$value":{"value":1.2,"unit":"s"}}}}));
        if let Some(field) = field {
            let frame = run(&mut s, "frame.create", json!({"width":100,"height":100,"props":{"layout":"vertical","gap":1.2}}))["id"].clone();
            run(&mut s, "variable.bind", json!({"ids":[frame],"field":field,"variable":"delay"}));
        }
        if let Some(scope) = scope {
            // Scope edits do not erase an already established binding's physical role.
            run(&mut s, "variable.update", json!({"variable":"delay","scopes":[scope]}));
        }
        let before = s.doc().unwrap().clone();
        let undo = s.undo_depth();
        for format in ["css", "tailwind"] {
            let result = run(&mut s, "variable.export", json!({"format":format}));
            let css = result["text"].as_str().unwrap();
            let lines: Vec<_> = css.lines().filter(|line| line.trim_start().starts_with("--")).collect();
            assert_eq!(lines.len(), 1, "one original duration variable: {css}");
            if !lines[0].trim_end().ends_with(&format!(": 1.2{unit};")) {
                failures.push(format!("{label}/{format}: {}", lines[0]));
            }
            assert_eq!(s.doc().unwrap(), &before, "export is read-only");
            assert_eq!(s.undo_depth(), undo);
            assert_eq!(s.doc().unwrap().variables.get("delay").unwrap().design_token_type, Some(DesignTokenType::Duration));
        }
    }
    assert!(failures.is_empty(), "native binding/scope unit authority must win: {failures:?}");
}

#[test]
fn token_duration_css_alias_reader_role_reaches_the_literal_target() {
    let mut s = Session::new();
    run(&mut s, "variable.import", json!({"format":"dtcg","variables":{"delay":{"$type":"duration","$value":{"value":0.4,"unit":"s"}}}}));
    run(&mut s, "variable.create", json!({"collection":"Tokens","name":"reader","type":"number","value":{"alias":"delay"}}));
    let frame = run(&mut s, "frame.create", json!({"width":100,"height":100}))["id"].clone();
    run(&mut s, "variable.bind", json!({"ids":[frame],"field":"opacity","variable":"reader"}));
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth();
    let mut failures = Vec::new();
    for format in ["css", "tailwind"] {
        let result = run(&mut s, "variable.export", json!({"format":format}));
        let css = result["text"].as_str().unwrap();
        let literals: Vec<_> = css.lines().filter(|line| line.trim_start().starts_with("--") && !line.contains("var(")).collect();
        assert_eq!(literals.len(), 1, "the one literal target and its live reader alias: {css}");
        if !literals[0].trim_end().ends_with(": 0.4;") {
            failures.push(format!("{format}: {}", literals[0]));
        }
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), undo);
        assert_eq!(s.doc().unwrap().variables.get("delay").unwrap().design_token_type, Some(DesignTokenType::Duration));
    }
    assert!(failures.is_empty(), "a bound reader lends its unit to the alias target: {failures:?}");
}
