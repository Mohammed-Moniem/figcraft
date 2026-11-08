use crate::Session;
use figcraft_doc::{VariableType, VariableValue};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}
fn output(s: &mut Session, p: Value) -> Value {
    serde_json::from_str(ex(s, "variable.export", p)["text"].as_str().unwrap()).unwrap()
}
fn scene() -> Session {
    let mut s = Session::new();
    ex(&mut s, "variable.createCollection", json!({"name":"Theme","modes":["Light","Dark"]}));
    ex(
        &mut s,
        "variable.create",
        json!({"collection":"Theme","name":"paint/base","type":"color","values":{"Light":"#12345680","Dark":"#abcdef"},"description":"Brand ink"}),
    );
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"spacing.small","type":"number","value":8}));
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"visible","type":"boolean","value":false}));
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"label","type":"string","value":"Hello"}));
    let base = s.doc().unwrap().variables.get("paint/base").unwrap().id.clone();
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"semantic","type":"color","value":{"alias":base}}));
    s
}
fn values(s: &Session) -> BTreeMap<String, (VariableType, String, BTreeMap<String, VariableValue>)> {
    let vars = &s.doc().unwrap().variables;
    vars.variables
        .iter()
        .map(|v| {
            let c = vars.collection_by_id(&v.collection).unwrap();
            let values = c
                .modes
                .iter()
                .map(|m| {
                    let modes = BTreeMap::from([(c.id.clone(), m.id.clone())]);
                    (m.name.clone(), vars.resolve(&v.id, &modes).unwrap())
                })
                .collect();
            (v.name.clone(), (v.kind, v.description.clone(), values))
        })
        .collect()
}

#[test]
fn dtcg_round_trip_keeps_modes_aliases_strings_booleans_and_names() {
    let mut source = scene();
    let before = values(&source);
    let out = output(&mut source, json!({"format":"dtcg"}));
    assert_eq!(out["Theme"]["semantic"]["$value"], "{Theme.paint/base}");
    assert_eq!(out["Theme"]["spacing%2Esmall"]["$value"], json!({"value":8.0,"unit":"px"}));
    assert!(out["Theme"].get("label").is_none(), "no invented DTCG string type");
    assert!(out["Theme"].get("visible").is_none(), "no invented DTCG boolean type");
    let mut target = Session::new();
    let r = ex(&mut target, "variable.import", json!({"variables":out}));
    assert_eq!(r["variables"], 5);
    assert_eq!(before, values(&target));
    assert!(matches!(target.doc().unwrap().variables.get("semantic").unwrap().values.values().next(), Some(VariableValue::Alias(_))));
    ex(&mut target, "edit.undo", json!({}));
    assert!(target.doc().unwrap().variables.variables.is_empty(), "one undo step");
}

#[test]
fn dtcg_selected_mode_is_interoperable_and_edited_tokens_override_native_metadata() {
    let mut source = scene();
    let mut out = output(&mut source, json!({"format":"dtcg","mode":"Dark"}));
    assert_eq!(out["Theme"]["paint/base"]["$value"]["components"], json!([171.0f32 / 255.0, 205.0f32 / 255.0, 239.0f32 / 255.0]));
    out["Theme"]["spacing%2Esmall"]["$value"]["value"] = json!(24);
    let mut target = Session::new();
    ex(&mut target, "variable.import", json!({"variables":out}));
    let v = values(&target);
    let n = &v["spacing.small"].2;
    assert_eq!(n["Dark"], VariableValue::Number(24.0));
    assert_eq!(n["Light"], VariableValue::Number(8.0));
    assert!(source.execute("variable.export", &json!({"format":"dtcg","mode":"Missing"})).is_err());
}

#[test]
fn dtcg_imports_spec_group_type_and_color_example_with_aliases_and_dimensions() {
    // Adapted synthetic values from DTCG 2025.10 Format §6.3.2 Example 12 and Color §4.1.
    // https://www.designtokens.org/tr/2025.10/format/#group-type-inheritance
    // https://www.designtokens.org/tr/2025.10/color/#format
    let input = json!({"color":{"$type":"color","primary":{"$value":{"colorSpace":"srgb","components":[0.0,0.4,0.8],"alpha":0.5}},"alias":{"$value":"{color.primary}"}},
        "spacing":{"small":{"$type":"dimension","$value":{"value":0.5,"unit":"rem"}},"medium":{"$type":"dimension","$value":{"value":{"$ref":"#/spacing/small/$value/value"},"unit":"rem"}}},
        "opacity":{"$type":"number","$value":0.75}});
    let mut s = Session::new();
    let r = ex(&mut s, "variable.import", json!({"variables":input,"remSize":20}));
    assert_eq!(r["variables"], 5);
    let vars = &s.doc().unwrap().variables;
    let space = vars.collections.iter().find(|c| c.name == "spacing").unwrap();
    assert_eq!(vars.variables.iter().filter(|v| v.collection == space.id).count(), 2);
    assert!(vars.variables.iter().filter(|v| v.collection == space.id).all(|v| v.values.values().all(|x| *x == VariableValue::Number(10.0))));
    let out = output(&mut s, json!({"format":"dtcg"}));
    assert_eq!(out["color"]["alias"]["$value"], "{color.primary}");
}

#[test]
fn dtcg_rejects_bad_types_cycles_pointers_and_values_without_mutating() {
    let cases = [
        json!({"a":{"$type":"number","$value":"{b}"},"b":{"$value":"{a}"}}),
        json!({"a":{"$type":"number","$value":"{missing}"}}),
        json!({"a":{"$type":"color","$value":{"colorSpace":"srgb","components":[2,0,0]}}}),
        json!({"a":{"$type":"dimension","$value":{"value":2,"unit":"em"}}}),
        json!({"a":{"$type":"number","$value":"4"}}),
        json!({"a":{"$type":"shadow","$value":{}}}),
        json!({"a":{"$value":4}}),
        json!({"a":{"$type":"number","$value":{"$ref":"https://example.invalid/tokens"}}}),
        json!({"a":{"$type":"number","$value":{"$ref":"#/a/$value"}}}),
        json!({"a":{"$type":"number","$value":4,"child":{"$value":1}}}),
        json!({"a":{"$type":"number","$value":4,"$extensions":{"other.vendor":{"x":1}}}}),
        json!({"a":{"$type":"fontWeight","$value":"Bold"}}),
        json!({"base":{"$type":"number","$value":1},"colors":{"$type":"color","bad":{"$type":"color","$value":"{base}"}}}),
    ];
    for case in cases {
        let mut s = Session::new();
        let before = s.doc().unwrap().clone();
        let e = s.execute("variable.import", &json!({"variables":case})).unwrap_err().to_string();
        assert!(e.contains("DTCG"), "{e}");
        assert_eq!(s.doc().unwrap(), &before);
    }
}

#[test]
fn dtcg_converts_linear_and_display_p3_colors_and_rejects_deep_input() {
    let mut s = Session::new();
    ex(&mut s, "file.colorProfile", json!({"profile":"displayP3"}));
    let color = json!({"red":{"$type":"color","$value":{"colorSpace":"srgb-linear","components":[0.25,0.0,0.0]}}});
    ex(&mut s, "variable.import", json!({"variables":color}));
    let out = output(&mut s, json!({"format":"dtcg"}));
    assert_eq!(out["Tokens"]["red"]["$value"]["colorSpace"], "display-p3");
    let mut deep = json!({"$type":"number","$value":1});
    for _ in 0..70 {
        deep = json!({"g":deep});
    }
    assert!(s.execute("variable.import", &json!({"variables":deep})).unwrap_err().to_string().contains("resource limit"));
}

#[test]
fn dtcg_json_pointer_alias_and_file_roundtrip_use_existing_capped_reader() {
    let mut s = Session::new();
    ex(
        &mut s,
        "variable.import",
        json!({"variables":{"one":{"$type":"number","$value":1},"two":{"$type":"number","$value":{"$ref":"#/one/$value"}}}}),
    );
    assert!(matches!(s.doc().unwrap().variables.get("two").unwrap().values.values().next(), Some(VariableValue::Alias(_))));
    let dir = std::env::temp_dir().join(format!("figcraft-dtcg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tokens.json");
    ex(&mut s, "variable.export", json!({"format":"dtcg","path":path}));
    let mut other = Session::new();
    ex(&mut other, "variable.import", json!({"variables":path}));
    assert_eq!(values(&s), values(&other));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dtcg_spec_top_level_pointer_aliases_and_property_access_are_supported() {
    // DTCG Format 2025.10 §7.1.2 Example 32: $ref is beside $type, without $value.
    let mut s = Session::new();
    let input = json!({"colors":{"blue":{"$type":"color","$value":{"colorSpace":"srgb","components":[0.0,0.4,0.8]}}},
        "semantic":{"primary":{"$ref":"#/colors/blue/$value","$type":"color"},"red":{"$ref":"#/colors/blue/$value/components/0","$type":"number"}}});
    let r = ex(&mut s, "variable.import", json!({"variables":input}));
    assert_eq!(r["variables"], 3);
    let out = output(&mut s, json!({"format":"dtcg"}));
    assert_eq!(out["semantic"]["primary"]["$value"], "{colors.blue}");
    assert_eq!(out["semantic"]["red"]["$value"], 0.0);
}

#[test]
fn dtcg_calculations_keep_live_references_after_import_id_remapping() {
    let mut source = scene();
    ex(
        &mut source,
        "variable.create",
        json!({"collection":"Theme","name":"faded","type":"color","value":{"alpha":{"variable":"paint/base","opacity":0.25}}}),
    );
    let first = output(&mut source, json!({"format":"dtcg"}));
    assert_eq!(first, output(&mut source, json!({"format":"dtcg"})), "deterministic export");
    let mut target = Session::new();
    ex(&mut target, "variable.createCollection", json!({"name":"Other"}));
    ex(&mut target, "variable.create", json!({"collection":"Other","name":"already-here","type":"number","value":3}));
    ex(&mut target, "variable.import", json!({"variables":first}));
    let vars = &target.doc().unwrap().variables;
    let base = vars.get("paint/base").unwrap();
    let faded = vars.get("faded").unwrap();
    assert!(faded.values.values().all(|v| matches!(v,VariableValue::ColorAlpha{variable,..} if variable==&base.id)));
    ex(&mut target, "variable.set", json!({"variable":"paint/base","value":"#ffffff"}));
    let vars = &target.doc().unwrap().variables;
    let faded = vars.get("faded").unwrap();
    let Some(VariableValue::Color(color)) = vars.resolve(&faded.id, &BTreeMap::new()) else { panic!("live calculation") };
    assert_eq!(color.to_rgba8(), [255, 255, 255, 64]);
}

#[test]
fn dtcg_native_extension_rejects_invalid_color_modes_and_dependency_conflicts_atomically() {
    let mut source = scene();
    let exported = output(&mut source, json!({"format":"dtcg"}));
    for bad_kind in 0..2 {
        let mut bad = exported.clone();
        let ext = &mut bad["$extensions"]["ai.storyteller.figcraft"];
        if bad_kind == 0 {
            let color = ext["variables"]["variables"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|v| v["resolvedType"] == "color" && v["name"] == "paint/base")
                .unwrap();
            let first = color["values"].as_object_mut().unwrap().values_mut().next().unwrap();
            first["value"]["r"] = json!(2.0);
        } else {
            ext["modes"] = json!({});
        }
        let mut target = Session::new();
        let before = target.doc().unwrap().clone();
        assert!(target.execute("variable.import", &json!({"variables":bad})).is_err());
        assert_eq!(target.doc().unwrap(), &before);
    }
    let mut target = Session::new();
    ex(&mut target, "variable.createCollection", json!({"name":"Theme"}));
    ex(&mut target, "variable.create", json!({"collection":"Theme","name":"paint/base","type":"number","value":1}));
    let before = target.doc().unwrap().clone();
    let err = target.execute("variable.import", &json!({"variables":exported})).unwrap_err().to_string();
    assert!(err.contains("conflicting type"), "{err}");
    assert_eq!(target.doc().unwrap(), &before);
}

#[test]
fn dtcg_profile_conversion_preserves_unedited_mix_expressions() {
    let mut source = scene();
    ex(
        &mut source,
        "variable.create",
        json!({"collection":"Theme","name":"mixed","type":"color","value":{"mix":{"from":{"variable":"paint/base"},"to":{"literal":"#ff0000"},"t":0.4}}}),
    );
    let exported = output(&mut source, json!({"format":"dtcg"}));
    let mut target = Session::new();
    ex(&mut target, "file.colorProfile", json!({"profile":"displayP3"}));
    ex(&mut target, "variable.import", json!({"variables":exported}));
    let mixed = target.doc().unwrap().variables.get("mixed").unwrap();
    assert!(mixed.values.values().all(|v| matches!(v, VariableValue::ColorMix { .. })));
}

#[test]
fn dtcg_collection_named_variable_collections_is_not_misdetected_as_empty_native_json() {
    let mut s = Session::new();
    let imported =
        ex(&mut s, "variable.import", json!({"variables":{"variableCollections":{"size":{"$type":"dimension","$value":{"value":10,"unit":"px"}}}}}));
    assert_eq!(imported["variables"], 1);
    assert_eq!(s.doc().unwrap().variables.collections[0].name, "variableCollections");
}

#[test]
fn dtcg_editing_a_base_token_keeps_unedited_derived_values_live() {
    let mut source = scene();
    ex(
        &mut source,
        "variable.create",
        json!({"collection":"Theme","name":"zDerived","type":"color","value":{"alpha":{"variable":"paint/base","opacity":0.25}}}),
    );
    let mut exported = output(&mut source, json!({"format":"dtcg"}));
    exported["Theme"]["paint/base"]["$value"]["components"] = json!([1.0, 1.0, 1.0]);
    let mut target = Session::new();
    ex(&mut target, "variable.import", json!({"variables":exported}));
    let vars = &target.doc().unwrap().variables;
    let derived = vars.get("zDerived").unwrap();
    assert!(matches!(derived.values.values().next(), Some(VariableValue::ColorAlpha { .. })));
    let Some(VariableValue::Color(color)) = vars.resolve(&derived.id, &BTreeMap::new()) else { panic!("derived color") };
    assert_eq!(color.to_rgba8(), [255, 255, 255, 64]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dtcg_import_keeps_the_existing_regular_file_cap() {
    let mut target = Session::new();
    let dir = std::env::temp_dir().join(format!("figcraft-dtcg-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oversized.tokens.json");
    std::fs::File::create(&path).unwrap().set_len(256 * 1024 * 1024 + 1).unwrap();
    let before = target.doc().unwrap().clone();
    let error = target.execute("variable.import", &json!({"variables":path})).unwrap_err().to_string();
    assert!(error.contains("more than the 256 MB allowed here"), "{error}");
    assert_eq!(target.doc().unwrap(), &before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dtcg_untyped_alias_follows_reference_before_parent_group_default() {
    // Format §5.2.2: an alias without token-level type takes its referenced token's type.
    let mut s = Session::new();
    ex(&mut s, "variable.import", json!({"variables":{"base":{"$type":"number","$value":1},"colors":{"$type":"color","alias":{"$value":"{base}"}}}}));
    assert_eq!(s.doc().unwrap().variables.get("alias").unwrap().kind, VariableType::Number);
}

#[test]
fn dtcg_json_text_with_pointer_imports_through_the_batch_transport() {
    let mut s = Session::new();
    let tokens = json!({"one":{"$type":"number","$value":1},"two":{"$type":"number","$ref":"#/one/$value"}});
    ex(&mut s, "command.batch", json!({"commands":[{"command":"variable.import","params":{"variables":tokens.to_string()}}]}));
    assert_eq!(s.doc().unwrap().variables.variables.len(), 2);
}

#[test]
fn dtcg_empty_export_imports_with_a_clear_warning() {
    let mut s = Session::new();
    let out = output(&mut s, json!({"format":"dtcg"}));
    let result = ex(&mut s, "variable.import", json!({"variables":out}));
    assert_eq!(result["variables"], 0);
    assert!(result["warnings"].to_string().contains("no variables found"));
}

#[test]
fn dtcg_types_survive_native_files_and_variables_json_without_restricting_pickers() {
    use figcraft_doc::DesignTokenType;
    let mut s = Session::new();
    ex(
        &mut s,
        "variable.import",
        json!({"variables":{"Theme":{
            "count":{"$type":"number","$value":42},
            "space":{"$type":"dimension","$value":{"value":8,"unit":"px"}},
            "family":{"$type":"fontFamily","$value":"Sample Sans"},
            "weight":{"$type":"fontWeight","$value":550}
        }}}),
    );
    let vars = &s.doc().unwrap().variables;
    let count = vars.get("count").unwrap();
    assert_eq!(count.design_token_type, Some(DesignTokenType::Number));
    assert!(vars.variables.iter().all(|v| v.scopes.is_none()), "type provenance does not impose usage scopes");
    for scope in ["WIDTH_HEIGHT", "FONT_SIZE", "OPACITY", "FONT_WEIGHT"] {
        assert!(count.in_scope(scope));
    }
    let native = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (doc, _) = crate::cmd::file::load_bytes(&native, "token-types.figcraft").unwrap();
    let mut reopened = Session::with_document(doc);
    let native_json = output(&mut reopened, json!({"format":"json"}));
    let mut from_json = Session::new();
    ex(&mut from_json, "variable.import", json!({"variables":native_json}));
    for session in [&mut reopened, &mut from_json] {
        let out = output(session, json!({"format":"dtcg"}));
        for (name, ty) in [("count", "number"), ("space", "dimension"), ("family", "fontFamily"), ("weight", "fontWeight")] {
            assert_eq!(out["Theme"][name]["$type"], ty);
        }
        assert_eq!(out["Theme"]["count"]["$value"], 42.0);
        assert_eq!(out["Theme"]["family"]["$value"], "Sample Sans");
        assert_eq!(session.doc().unwrap().variables.get("count").unwrap().scopes, None);
    }
}

#[test]
fn dtcg_aliases_of_different_standard_types_flatten_but_native_expressions_stay_live() {
    use figcraft_doc::DesignTokenType;
    let mut s = Session::new();
    ex(&mut s, "variable.createCollection", json!({"name":"Theme"}));
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"base","type":"number","value":400}));
    for name in ["weight", "space"] {
        ex(&mut s, "variable.create", json!({"collection":"Theme","name":name,"type":"number","value":{"alias":"base"}}));
    }
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"general","type":"string","value":"Sample Sans"}));
    ex(&mut s, "variable.create", json!({"collection":"Theme","name":"family","type":"string","value":{"alias":"general"}}));
    let mut doc = s.doc().unwrap().clone();
    for v in &mut doc.variables.variables {
        v.design_token_type = match v.name.as_str() {
            "base" => Some(DesignTokenType::Number),
            "weight" => Some(DesignTokenType::FontWeight),
            "space" => Some(DesignTokenType::Dimension),
            "family" => Some(DesignTokenType::FontFamily),
            _ => None,
        };
    }
    let mut s = Session::with_document(doc);
    let out = output(&mut s, json!({"format":"dtcg"}));
    assert_eq!(out["Theme"]["weight"]["$type"], "fontWeight");
    assert_eq!(out["Theme"]["weight"]["$value"], 400.0);
    assert_eq!(out["Theme"]["space"]["$value"], json!({"value":400.0,"unit":"px"}));
    assert_eq!(out["Theme"]["family"]["$value"], "Sample Sans");
    assert!(out["Theme"].get("general").is_none());
    let mut other = Session::new();
    ex(&mut other, "variable.import", json!({"variables":out}));
    assert!(other.doc().unwrap().variables.get("weight").unwrap().values.values().all(|v| matches!(v, VariableValue::Alias(_))));
    assert!(other.doc().unwrap().variables.get("family").unwrap().values.values().all(|v| matches!(v, VariableValue::Alias(_))));
    ex(&mut other, "variable.set", json!({"variable":"base","value":600}));
    ex(&mut other, "variable.set", json!({"variable":"general","value":"New Sans"}));
    let out = output(&mut other, json!({"format":"dtcg"}));
    assert_eq!(out["Theme"]["weight"]["$value"], 600.0);
    assert_eq!(out["Theme"]["space"]["$value"], json!({"value":600.0,"unit":"px"}));
    assert_eq!(out["Theme"]["family"]["$value"], "New Sans");
}

#[test]
fn dtcg_binding_inferred_type_enriches_native_metadata_and_css_context_still_wins() {
    use figcraft_doc::DesignTokenType;
    let mut source = Session::new();
    ex(&mut source, "variable.createCollection", json!({"name":"Theme"}));
    ex(&mut source, "variable.create", json!({"collection":"Theme","name":"amount","type":"number","value":0.5}));
    let frame = ex(&mut source, "frame.create", json!({"width":100,"height":100}))["id"].clone();
    ex(&mut source, "variable.bind", json!({"ids":[frame],"field":"opacity","variable":"amount"}));
    assert_eq!(source.doc().unwrap().variables.get("amount").unwrap().design_token_type, None);
    let out = output(&mut source, json!({"format":"dtcg"}));
    assert_eq!(out["Theme"]["amount"]["$type"], "number");
    let mut other = Session::new();
    ex(&mut other, "variable.import", json!({"variables":out}));
    assert_eq!(other.doc().unwrap().variables.get("amount").unwrap().design_token_type, Some(DesignTokenType::Number));
    assert_eq!(output(&mut other, json!({"format":"dtcg"}))["Theme"]["amount"]["$type"], "number");
    let frame = ex(&mut other, "frame.create", json!({"width":100,"height":100}))["id"].clone();
    ex(&mut other, "variable.bind", json!({"ids":[frame],"field":"width","variable":"amount"}));
    let css = ex(&mut other, "variable.export", json!({"format":"css"}))["text"].as_str().unwrap().to_owned();
    assert!(css.contains("--amount: 0.5px;"), "actual width usage takes priority over provenance: {css}");
    assert_eq!(output(&mut other, json!({"format":"dtcg"}))["Theme"]["amount"]["$type"], "number");
}

#[test]
fn dtcg_invalid_native_type_hint_is_rejected_before_import_mutation() {
    let mut source = scene();
    let mut out = output(&mut source, json!({"format":"dtcg"}));
    let variable = out["$extensions"]["ai.storyteller.figcraft"]["variables"]["variables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["name"] == "paint/base")
        .unwrap();
    variable["designTokenType"] = json!("number");
    let mut other = Session::new();
    let before = other.doc().unwrap().clone();
    let err = other.execute("variable.import", &json!({"variables":out})).unwrap_err().to_string();
    assert!(err.contains("designTokenType") && err.contains("incompatible"), "{err}");
    assert_eq!(other.doc().unwrap(), &before);
}

#[test]
fn dtcg_type_hint_compatibility_is_checked_in_native_documents_and_variables_json() {
    use figcraft_doc::DesignTokenType;
    let mut source = scene();
    let mut doc = source.doc().unwrap().clone();
    let color = doc.variables.variables.iter_mut().find(|v| v.name == "paint/base").unwrap();
    color.design_token_type = Some(DesignTokenType::Number);
    assert!(doc.validate().iter().any(|s| s.contains("design token type")));
    doc.repair();
    assert_eq!(doc.variables.get("paint/base").unwrap().design_token_type, None);
    let mut variables = output(&mut source, json!({"format":"json"}));
    let bad = variables["meta"]["variables"].as_object_mut().unwrap().values_mut().find(|v| v["name"] == "paint/base").unwrap();
    bad["designTokenType"] = json!("fontFamily");
    let mut other = Session::new();
    let imported = ex(&mut other, "variable.import", json!({"variables":variables}));
    assert!(imported["warnings"].to_string().contains("designTokenType"));
    assert!(other.doc().unwrap().variables.get("paint/base").is_none());
    assert!(other.doc().unwrap().variables.get("spacing.small").is_some(), "readable variables still import");
}

#[test]
fn dtcg_variable_create_preserves_valid_hints_and_refuses_incompatible_ones_atomically() {
    let mut s = Session::new();
    ex(&mut s, "variable.createCollection", json!({"name":"Theme"}));
    for hint in [json!("unknown"), json!("fontFamily"), json!(42), json!({})] {
        let before = s.doc().unwrap().clone();
        let error = s
            .execute("variable.create", &json!({"collection":"Theme","name":"bad","type":"number","value":1,"designTokenType":hint}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("designTokenType"), "{error}");
        assert_eq!(s.doc().unwrap(), &before);
    }
    for (hint, kind, value) in [
        ("color", "color", json!("#123456")),
        ("dimension", "number", json!(8)),
        ("number", "number", json!(42)),
        ("fontFamily", "string", json!("Sample Sans")),
        ("fontWeight", "number", json!(500)),
    ] {
        ex(&mut s, "variable.create", json!({"collection":"Theme","name":hint,"type":kind,"value":value,"designTokenType":hint}));
    }
    let out = output(&mut s, json!({"format":"dtcg"}));
    for hint in ["color", "dimension", "number", "fontFamily", "fontWeight"] {
        assert_eq!(out["Theme"][hint]["$type"], hint);
        assert_eq!(s.doc().unwrap().variables.get(hint).unwrap().scopes, None);
    }
}
