//! The existing variables model exchanged with the DTCG 2025.10 Format/Color modules.
//! The namespaced extension keeps modes and native-only values; it is never a custom `$type`.
use std::collections::{BTreeMap, BTreeSet};

use figcraft_color::{Color, ColorProfile, Hsl, Lch, Oklch};
use figcraft_doc::{Collection, DesignTokenType, Document, Mode, Variable, VariableType, VariableValue, Variables};
use serde_json::{Map, Value, json};

use crate::{Result, bad};

const EXT: &str = "ai.storyteller.figcraft";
const MAX_TOKENS: usize = 100_000;
const MAX_NODES: usize = 2_000_000;
const MAX_DEPTH: usize = 64;

fn error(path: &str, why: &str) -> crate::EngineError {
    bad("variable.import", format!("DTCG {path}: {why}"))
}

/// Bound object inputs too: they need not have passed the file reader or JSON recursion limit.
fn bounded(root: &Value) -> Result<()> {
    let mut pending = vec![(root, 0)];
    let (mut count, mut text) = (0usize, 0usize);
    while let Some((v, depth)) = pending.pop() {
        count += 1;
        if count > MAX_NODES || depth > MAX_DEPTH || text > 64 * 1024 * 1024 {
            return Err(error("/", "resource limit (64 levels, 2 million values, 64 MiB of text) exceeded"));
        }
        match v {
            Value::Object(o) => {
                if o.len().saturating_add(pending.len()) > MAX_NODES {
                    return Err(error("/", "JSON member limit exceeded"));
                }
                text = o.keys().fold(text, |n, k| n.saturating_add(k.len()));
                pending.extend(o.values().map(|v| (v, depth + 1)));
            }
            Value::Array(a) => {
                if a.len().saturating_add(pending.len()) > MAX_NODES {
                    return Err(error("/", "JSON member limit exceeded"));
                }
                pending.extend(a.iter().map(|v| (v, depth + 1)));
            }
            Value::String(s) => text = text.saturating_add(s.len()),
            _ => {}
        }
        if text > 64 * 1024 * 1024 {
            return Err(error("/", "resource limit (64 MiB of text) exceeded"));
        }
    }
    Ok(())
}

/// A reversible legal DTCG name; `%` is escaped too, preventing encoded-name collisions.
fn key(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if matches!(c, '%' | '.' | '{' | '}') || i == 0 && c == '$' {
            out.push_str(&format!("%{:02X}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    if out.is_empty() { "%00".into() } else { out }
}

/// A DTCG group may itself be named variableCollections; native-shape detection alone
/// would silently read that legal group as an empty REST response.
pub(super) fn has_tokens(root: &Value) -> bool {
    let mut pending = vec![root];
    let mut visited = 0;
    while let Some(value) = pending.pop() {
        visited += 1;
        if visited > MAX_NODES {
            return true;
        } // The importer reports the resource error.
        if let Some(o) = value.as_object() {
            if o.contains_key("$value") || o.contains_key("$ref") || o.get("$extensions").is_some_and(|v| v.get(EXT).is_some()) {
                return true;
            }
            if pending.len().saturating_add(o.len()) > MAX_NODES {
                return true;
            }
            pending.extend(o.values());
        } else if let Some(a) = value.as_array() {
            if pending.len().saturating_add(a.len()) > MAX_NODES {
                return true;
            }
            pending.extend(a);
        }
    }
    false
}

fn standard_type(v: &Variable, roles: &crate::token_roles::Roles) -> Option<DesignTokenType> {
    v.design_token_type.or_else(|| match v.kind {
        VariableType::Color => Some(DesignTokenType::Color),
        VariableType::Number if roles.role(v) == crate::token_roles::NumberRole::Unitless => Some(DesignTokenType::Number),
        VariableType::Number => Some(DesignTokenType::Dimension),
        VariableType::String | VariableType::Boolean => None,
    })
}

pub(super) fn export(doc: &Document, colls: &[&Collection], p: &Value) -> Result<String> {
    let native = Variables {
        collections: colls.iter().map(|c| (*c).clone()).collect(),
        variables: doc.variables.variables.iter().filter(|v| colls.iter().any(|c| c.id == v.collection)).cloned().collect(),
    };
    validate_native(&native)?;
    let mut modes = BTreeMap::new();
    let mut matched = false;
    for c in colls {
        let chosen = p.get("mode").and_then(Value::as_str).and_then(|m| c.find_mode(m));
        matched |= chosen.is_some();
        let m = chosen.or_else(|| c.modes.first()).ok_or_else(|| error(&c.name, "collection has no modes"))?;
        modes.insert(c.id.clone(), m.id.clone());
    }
    if p.get("mode").is_some() && (!p["mode"].is_string() || !matched) {
        return Err(bad("variable.export", "DTCG mode must name a mode in an exported collection"));
    }
    let paths: BTreeMap<_, _> = native
        .variables
        .iter()
        .filter_map(|v| {
            let c = native.collection_by_id(&v.collection)?;
            Some((v.id.clone(), format!("{}.{}", key(&c.name), key(&v.name))))
        })
        .collect();
    let roles = crate::token_roles::Roles::new(doc);
    let types: BTreeMap<_, _> = native.variables.iter().filter_map(|v| Some((v.id.clone(), standard_type(v, &roles)?))).collect();
    let mut root = Map::new();
    for c in colls {
        let mut group = Map::new();
        for v in native.variables.iter().filter(|v| v.collection == c.id) {
            // General strings/booleans stay native-only; a retained fontFamily type is standard.
            let Some(token_type) = types.get(&v.id).copied() else { continue };
            let ty = token_type.as_str();
            let raw = v.value_or_first(modes.get(&c.id).map(String::as_str).unwrap_or_default());
            let alias = raw.and_then(|value| match value {
                VariableValue::Alias(id) if types.get(id) == Some(&token_type) => Some(id),
                _ => None,
            });
            let value = if let Some(id) = alias {
                let path = paths.get(id).ok_or_else(|| error(&v.name, "alias target is outside the exported collections"))?;
                json!(format!("{{{path}}}"))
            } else {
                match native.resolve(&v.id, &modes) {
                    Some(VariableValue::Color(c)) => {
                        json!({"colorSpace": if doc.color_profile.is_srgb() { "srgb" } else { "display-p3" }, "components": [c.r,c.g,c.b], "alpha":c.a})
                    }
                    Some(VariableValue::Number(n)) if ty == "fontWeight" && !(1.0..=1000.0).contains(&n) => {
                        return Err(error(&v.name, "fontWeight outside 1..1000"));
                    }
                    Some(VariableValue::String(text)) if ty == "fontFamily" => {
                        if token_reference(&json!(text)).is_some() {
                            return Err(error(&v.name, "fontFamily literal uses reserved alias syntax"));
                        }
                        json!(text)
                    }
                    Some(VariableValue::Number(n)) if n.is_finite() && ty == "dimension" => json!({"value":n,"unit":"px"}),
                    Some(VariableValue::Number(n)) if n.is_finite() => json!(n),
                    _ => return Err(error(&v.name, "value cannot be resolved in the selected mode")),
                }
            };
            group.insert(
                key(&v.name),
                json!({"$type":ty,"$value":value,"$description":v.description,
                "$extensions":{EXT:{"id":v.id,"exportedValue":value,"exportedType":ty}}}),
            );
        }
        if root.insert(key(&c.name), Value::Object(group)).is_some() {
            return Err(error(&c.name, "collection names must be unique"));
        }
    }
    root.insert("$extensions".into(), json!({EXT:{"version":1,"variables":native,"modes":modes,"colorProfile":doc.color_profile}}));
    let value = Value::Object(root);
    bounded(&value)?;
    serde_json::to_string_pretty(&value).map_err(|e| error("/", &e.to_string()))
}

#[derive(Clone)]
struct Token {
    path: String,
    collection: String,
    name: String,
    kind: Option<String>,
    value: Value,
    description: String,
    native: Value,
}

fn extensions(v: &Value, path: &str) -> Result<()> {
    if let Some(e) = v.get("$extensions") {
        let o = e.as_object().ok_or_else(|| error(path, "$extensions must be an object"))?;
        if o.keys().any(|k| k != EXT) {
            return Err(error(path, "foreign extension data cannot be preserved by the variables model"));
        }
    }
    Ok(())
}

fn walk(v: &Value, path: &[String], inherited: Option<&str>, out: &mut Vec<Token>) -> Result<()> {
    let label = path.join(".");
    let o = v.as_object().ok_or_else(|| error(&label, "group or token must be an object"))?;
    extensions(v, &label)?;
    if v.get("$extends").is_some() {
        return Err(error(&label, "group $extends is not supported; resolve the group first"));
    }
    let ty = match o.get("$type") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => return Err(error(&label, "$type must be a string")),
        None => inherited,
    };
    let description = match o.get("$description") {
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(error(&label, "$description must be a string")),
        None => String::new(),
    };
    if o.contains_key("$value") && o.contains_key("$ref") {
        return Err(error(&label, "token cannot have both $value and $ref"));
    }
    // The Format module's §7.1.2 puts $ref alongside $type for a whole-token/property alias;
    // references nested inside $value use the same resolution path below.
    let pointer = o.get("$ref").map(|r| json!({"$ref":r}));
    if let Some(value) = o.get("$value").or(pointer.as_ref()) {
        if o.keys().any(|k| !k.starts_with('$')) {
            return Err(error(&label, "a token cannot also contain groups or tokens"));
        }
        if out.len() >= MAX_TOKENS {
            return Err(error(&label, "more than 100000 tokens"));
        }
        let (collection, name) = if path.len() > 1 { (path[0].clone(), path[1..].join("/")) } else { ("Tokens".into(), label.clone()) };
        // Format §5.2.2 specifically resolves untyped aliases from their target before
        // considering a parent group default. An explicit token type still must agree.
        let kind = if o.get("$type").is_none() && token_reference(value).is_some() { None } else { ty.map(str::to_owned) };
        out.push(Token { path: label, collection, name, kind, value: value.clone(), description, native: v["$extensions"][EXT].clone() });
    } else {
        for (k, child) in o {
            if k.starts_with('$') && k != "$root" {
                continue;
            }
            if k != "$root" && (k.is_empty() || k.contains(['.', '{', '}'])) {
                return Err(error(&label, "invalid token or group name"));
            }
            let mut next = path.to_vec();
            next.push(k.clone());
            walk(child, &next, ty, out)?;
        }
    }
    Ok(())
}

fn token_reference(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str().and_then(|s| s.strip_prefix('{')).and_then(|s| s.strip_suffix('}')) {
        return Some(s.to_owned());
    }
    let pointer = value.as_object().filter(|o| o.len() == 1)?.get("$ref")?.as_str()?;
    let token = pointer.strip_prefix("#/")?.strip_suffix("/$value")?;
    Some(token.split('/').map(|s| s.replace("~1", "/").replace("~0", "~")).collect::<Vec<_>>().join("."))
}

fn resolved_type(index: usize, tokens: &[Token], ids: &BTreeMap<String, usize>, seen: &mut BTreeSet<usize>) -> Result<String> {
    let t = tokens.get(index).ok_or_else(|| error("/", "missing token"))?;
    if !seen.insert(index) || seen.len() > Variables::RESOLVE_LIMIT {
        return Err(error(&t.path, "alias cycle or chain longer than the engine's 32-variable resolution limit"));
    }
    let target = token_reference(&t.value).map(|r| ids.get(&r).copied().ok_or_else(|| error(&t.path, "alias target does not exist"))).transpose()?;
    let target_type = target.map(|i| resolved_type(i, tokens, ids, seen)).transpose()?;
    let kind = t.kind.clone().or_else(|| target_type.clone()).ok_or_else(|| error(&t.path, "token has no declared or inherited type"))?;
    if target_type.is_some_and(|k| k != kind) {
        return Err(error(&t.path, "alias type differs from its target"));
    }
    Ok(kind)
}

/// Resolve local JSON Pointers to sub-values; never fetch external files or URLs.
fn component(value: &Value, root: &Value, path: &str, seen: &mut BTreeSet<String>, left: &mut (usize, usize), depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(error(path, "reference expansion depth exceeded"));
    }
    left.0 = left.0.checked_sub(1).ok_or_else(|| error(path, "reference expansion limit exceeded"))?;
    if let Some(s) = value.as_str() {
        left.1 = left.1.checked_sub(s.len()).ok_or_else(|| error(path, "reference text expansion limit exceeded"))?;
    }
    if let Some(o) = value.as_object() {
        if let Some(reference) = o.get("$ref") {
            let pointer =
                reference.as_str().and_then(|s| s.strip_prefix('#')).ok_or_else(|| error(path, "only local JSON Pointers are supported"))?;
            if o.len() != 1 || !seen.insert(pointer.into()) || seen.len() > MAX_DEPTH {
                return Err(error(path, "invalid or circular JSON Pointer"));
            }
            let target = root.pointer(pointer).ok_or_else(|| error(path, "JSON Pointer target does not exist"))?;
            let result = component(target, root, path, seen, left, depth + 1);
            seen.remove(pointer);
            return result;
        }
        let mut result = Map::new();
        for (k, v) in o {
            left.1 = left.1.checked_sub(k.len()).ok_or_else(|| error(path, "reference text expansion limit exceeded"))?;
            result.insert(k.clone(), component(v, root, path, seen, left, depth + 1)?);
        }
        return Ok(Value::Object(result));
    }
    if let Some(a) = value.as_array() {
        return a.iter().map(|v| component(v, root, path, seen, left, depth + 1)).collect::<Result<Vec<_>>>().map(Value::Array);
    }
    Ok(value.clone())
}

fn number(v: &Value, path: &str) -> Result<f64> {
    v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| error(path, "expected a finite number"))
}

fn color(v: &Value, profile: ColorProfile, path: &str) -> Result<Color> {
    let space = v["colorSpace"].as_str().ok_or_else(|| error(path, "colorSpace required"))?;
    let c = v["components"].as_array().filter(|a| a.len() == 3).ok_or_else(|| error(path, "color needs three numeric components"))?;
    let [a, b, c] = [number(&c[0], path)?, number(&c[1], path)?, number(&c[2], path)?];
    if [a, b, c].iter().any(|n| n.abs() > f64::from(f32::MAX)) {
        return Err(error(path, "color component cannot be represented"));
    }
    let alpha = v.get("alpha").map(|v| number(v, path)).transpose()?.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&alpha) {
        return Err(error(path, "alpha outside 0..1"));
    }
    let rgb = |a: f64, b: f64, c: f64| -> Result<Color> {
        if [a, b, c].iter().any(|x| !(0.0..=1.0).contains(x)) {
            return Err(error(path, "RGB component outside 0..1"));
        }
        Ok(Color::new(a as f32, b as f32, c as f32, alpha as f32))
    };
    let (source, out) = match space {
        "srgb" => (ColorProfile::Srgb, rgb(a, b, c)?),
        "display-p3" => (ColorProfile::DisplayP3, rgb(a, b, c)?),
        "srgb-linear" => {
            rgb(a, b, c)?;
            let encode = |x: f64| if x <= 0.0031308 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
            (ColorProfile::Srgb, rgb(encode(a), encode(b), encode(c))?)
        }
        "hsl" if (0.0..360.0).contains(&a) && (0.0..=100.0).contains(&b) && (0.0..=100.0).contains(&c) => {
            (ColorProfile::Srgb, Hsl { h: a as f32, s: (b / 100.0) as f32, l: (c / 100.0) as f32 }.to_color().with_alpha(alpha as f32))
        }
        "lch" if (0.0..=100.0).contains(&a) && b >= 0.0 && (0.0..360.0).contains(&c) => {
            (ColorProfile::Srgb, Lch { l: a as f32, c: b as f32, h: c as f32 }.to_color().with_alpha(alpha as f32))
        }
        "oklch" if (0.0..=1.0).contains(&a) && b >= 0.0 && (0.0..360.0).contains(&c) => {
            (ColorProfile::Srgb, Oklch { l: a as f32, c: b as f32, h: c as f32 }.to_color().with_alpha(alpha as f32))
        }
        _ => {
            return Err(error(
                path,
                "unsupported color space or out-of-range components (supported: srgb, srgb-linear, display-p3, hsl, lch, oklch)",
            ));
        }
    };
    Ok(source.conversion_to(profile).apply(out))
}

fn literal(kind: &str, value: &Value, profile: ColorProfile, rem: f64, path: &str) -> Result<VariableValue> {
    Ok(match kind {
        "color" => VariableValue::Color(color(value, profile, path)?),
        "number" => VariableValue::Number(number(value, path)?),
        "dimension" => {
            let n = number(&value["value"], path)?;
            let n = match value["unit"].as_str() {
                Some("px") => n,
                Some("rem") => n * rem,
                _ => return Err(error(path, "dimension unit must be px or rem")),
            };
            if !n.is_finite() {
                return Err(error(path, "dimension conversion overflow"));
            }
            VariableValue::Number(n)
        }
        "fontFamily" => {
            VariableValue::String(value.as_str().ok_or_else(|| error(path, "fontFamily arrays cannot be represented by a string variable"))?.into())
        }
        "fontWeight" => {
            let n = match value.as_str() {
                Some("thin" | "hairline") => 100.0,
                Some("extra-light" | "ultra-light") => 200.0,
                Some("light") => 300.0,
                Some("normal" | "regular" | "book") => 400.0,
                Some("medium") => 500.0,
                Some("semi-bold" | "demi-bold") => 600.0,
                Some("bold") => 700.0,
                Some("extra-bold" | "ultra-bold") => 800.0,
                Some("black" | "heavy") => 900.0,
                Some("extra-black" | "ultra-black") => 950.0,
                _ => number(value, path)?,
            };
            if !(1.0..=1000.0).contains(&n) {
                return Err(error(path, "fontWeight outside 1..1000"));
            }
            VariableValue::Number(n)
        }
        _ => return Err(error(path, &format!("unsupported token type {kind}; the variables model cannot represent this type"))),
    })
}

fn native_payload(value: &Value) -> Result<()> {
    let color = |v: &Value| -> Result<()> {
        for key in ["r", "g", "b", "a"] {
            let n = number(&v[key], "native color")?;
            if !(0.0..=1.0).contains(&n) {
                return Err(error("native color", "component outside 0..1"));
            }
        }
        Ok(())
    };
    if let Some(variables) = value["variables"].as_array() {
        for v in variables {
            if let Some(values) = v["values"].as_object() {
                for value in values.values() {
                    let data = &value["value"];
                    match value["type"].as_str() {
                        Some("color") => color(data)?,
                        Some("colorAlpha") if !(0.0..=1.0).contains(&number(&data["opacity"], "native opacity")?) => {
                            return Err(error("native opacity", "outside 0..1"));
                        }
                        Some("colorMix") => {
                            if !(0.0..=1.0).contains(&number(&data["t"], "native mix")?) {
                                return Err(error("native mix", "outside 0..1"));
                            }
                            for op in [&data["from"], &data["to"]] {
                                if let Some(v) = op.get("literal") {
                                    color(v)?;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn import(root: &Value, p: &Value, profile: ColorProfile) -> Result<(Variables, Vec<String>)> {
    bounded(root)?;
    let rem = p.get("remSize").map(|v| number(v, "remSize")).transpose()?.unwrap_or(16.0);
    if !(0.0..=100_000.0).contains(&rem) || rem == 0.0 {
        return Err(error("remSize", "expected 0 < remSize <= 100000"));
    }
    let mut tokens = Vec::new();
    walk(root, &[], None, &mut tokens)?;
    let ids: BTreeMap<_, _> = tokens.iter().enumerate().map(|(i, t)| (t.path.clone(), i)).collect();
    let ext = &root["$extensions"][EXT];
    let selected_modes: BTreeMap<String, String> = if ext.is_null() {
        BTreeMap::new()
    } else {
        serde_json::from_value(ext["modes"].clone()).map_err(|_| error("/", "invalid native selected modes"))?
    };
    let mut native = if ext.is_null() {
        Variables::default()
    } else {
        if ext["version"] != 1 {
            return Err(error("/", "unsupported FigCraft extension version"));
        }
        native_payload(&ext["variables"])?;
        let v: Variables =
            serde_json::from_value(ext["variables"].clone()).map_err(|e| error("/", &format!("invalid FigCraft variables extension: {e}")))?;
        validate_native(&v)?;
        if selected_modes.len() != v.collections.len()
            || v.collections.iter().any(|c| selected_modes.get(&c.id).is_none_or(|m| c.mode_by_id(m).is_none()))
        {
            return Err(error("/", "selected modes do not match native collections"));
        }
        v
    };
    if tokens.is_empty() && native.collections.is_empty() && ext.is_null() {
        return Err(error("/", "no tokens or native variables"));
    }
    let mut warnings = Vec::new();
    if tokens.is_empty() && native.variables.is_empty() {
        warnings.push("no variables found in the DTCG token file".into());
    }
    if tokens.iter().any(|t| t.kind.as_deref() == Some("dimension") && t.value["unit"] == "rem") {
        warnings.push(format!("DTCG rem dimensions converted to pixels using remSize={rem}"));
    }
    // Compare generated values with their original native calculations in the source profile.
    // Converting operands first could change a mix slightly and accidentally flatten it.
    let source_profile = if ext.is_null() {
        profile
    } else {
        serde_json::from_value(ext["colorProfile"].clone()).map_err(|_| error("/", "invalid native color profile"))?
    };
    let mut mapped = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut expansion = (MAX_NODES, 64 * 1024 * 1024);
    for (i, t) in tokens.iter().enumerate() {
        let kind = resolved_type(i, &tokens, &ids, &mut BTreeSet::new())?;
        let token_type = DesignTokenType::parse(&kind).ok_or_else(|| error(&t.path, &format!("unsupported token type {kind}")))?;
        let vt = token_type.variable_type();
        if let Some(id) = t.native["id"].as_str() {
            if !used.insert(id.to_owned()) {
                return Err(error(&t.path, "duplicate native variable mapping"));
            }
            let v = native.get_by_id(id).ok_or_else(|| error(&t.path, "native variable id does not exist"))?;
            if v.kind != vt {
                return Err(error(&t.path, "native variable type differs from token type"));
            }
            mapped.insert(i, id.to_owned());
        } else {
            let cid = match native.collections.iter().find(|c| c.name == t.collection) {
                Some(c) => c.id.clone(),
                None => {
                    let id = format!("DTCG:C{}", native.collections.len());
                    native.collections.push(Collection {
                        id: id.clone(),
                        name: t.collection.clone(),
                        modes: vec![Mode { id: format!("{id}:M0"), name: "Default".into() }],
                        extends: None,
                    });
                    id
                }
            };
            let id = format!("DTCG:V{i}");
            if native.get_by_id(&id).is_some() {
                return Err(error(&t.path, "variable id collision"));
            }
            native.variables.push(Variable {
                id: id.clone(),
                name: t.name.clone(),
                collection: cid,
                kind: vt,
                values: BTreeMap::new(),
                description: t.description.clone(),
                scopes: None,
                hidden_from_publishing: false,
                extends: None,
                overrides: BTreeSet::new(),
                design_token_type: Some(token_type),
                code_syntax: BTreeMap::new(),
            });
            mapped.insert(i, id);
        }
    }
    let mut incoming = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let kind = resolved_type(i, &tokens, &ids, &mut BTreeSet::new())?;
        let id = mapped.get(&i).ok_or_else(|| error(&t.path, "missing variable mapping"))?;
        let var = native.get_by_id(id).ok_or_else(|| error(&t.path, "missing variable"))?;
        let coll = native.collection_by_id(&var.collection).ok_or_else(|| error(&t.path, "missing collection"))?;
        let mid = ext["modes"][&coll.id]
            .as_str()
            .or_else(|| coll.default_mode().map(String::as_str))
            .ok_or_else(|| error(&t.path, "missing mode"))?
            .to_owned();
        if coll.mode_by_id(&mid).is_none() {
            return Err(error(&t.path, "invalid selected mode"));
        }
        // Unedited generated values keep their native expressions and precision. Edited token
        // values are authoritative for this mode, rather than being hidden by stale metadata.
        let unchanged = t.native["exportedValue"] == t.value && t.native["exportedType"] == kind && var.values.contains_key(&mid);
        let val = if let Some(target) = token_reference(&t.value) {
            let j = ids.get(&target).ok_or_else(|| error(&t.path, "alias target missing"))?;
            VariableValue::Alias(mapped.get(j).ok_or_else(|| error(&t.path, "alias mapping missing"))?.clone())
        } else {
            let value = component(&t.value, root, &t.path, &mut BTreeSet::new(), &mut expansion, 0)?;
            literal(&kind, &value, source_profile, rem, &t.path)?
        };
        let same = unchanged
            && match &val {
                VariableValue::Alias(_) => var.values.get(&mid) == Some(&val),
                _ => native.resolve(id, &selected_modes) == Some(val.clone()),
            };
        let hint = DesignTokenType::parse(&kind).ok_or_else(|| error(&t.path, "unsupported token type"))?;
        incoming.push((id.clone(), mid, val, t.description.clone(), same, hint));
    }
    // Validate against the original snapshot before applying any edits. Changing a base color
    // must leave an unedited derived expression live, rather than flattening its old token.
    for (id, mid, value, description, keep, hint) in incoming {
        if let Some(v) = native.variables.iter_mut().find(|v| v.id == id) {
            if !keep {
                v.values.insert(mid, value);
            }
            v.description = description;
            v.design_token_type = Some(hint);
        }
    }
    validate_native(&native)?;
    let conversion = source_profile.conversion_to(profile);
    native.for_each_color_mut(&mut |c| *c = conversion.apply(*c));
    Ok((native, warnings))
}

fn valid_native_value(value: &VariableValue) -> bool {
    let color = |c: &Color| [c.r, c.g, c.b, c.a].iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n));
    match value {
        VariableValue::Color(c) => color(c),
        VariableValue::Number(n) => n.is_finite(),
        VariableValue::ColorAlpha { opacity, .. } => opacity.is_finite() && (0.0..=1.0).contains(opacity),
        VariableValue::ColorMix { from, to, t } => {
            t.is_finite()
                && (0.0..=1.0).contains(t)
                && [from, to].iter().all(|op| match op {
                    figcraft_doc::ColorOperand::Literal { literal } => color(literal),
                    figcraft_doc::ColorOperand::Variable { .. } => true,
                })
        }
        _ => true,
    }
}

fn validate_native(vars: &Variables) -> Result<()> {
    if vars.variables.len() > MAX_TOKENS || vars.collections.len() > 4096 {
        return Err(error("/", "too many variables or collections"));
    }
    let mut cids = BTreeSet::new();
    let mut cnames = BTreeSet::new();
    for c in &vars.collections {
        let modes: BTreeSet<_> = c.modes.iter().map(|m| &m.id).collect();
        let names: BTreeSet<_> = c.modes.iter().map(|m| &m.name).collect();
        if !cids.insert(&c.id)
            || !cnames.insert(&c.name)
            || c.modes.is_empty()
            || c.modes.len() > 256
            || modes.len() != c.modes.len()
            || names.len() != c.modes.len()
        {
            return Err(error(&c.name, "invalid collection or modes"));
        }
    }
    let by_id: BTreeMap<_, _> = vars.variables.iter().map(|v| (v.id.as_str(), v)).collect();
    if by_id.len() != vars.variables.len() {
        return Err(error("/", "duplicate variable ids"));
    }
    let mut steps = 0usize;
    let mut names = BTreeSet::new();
    for v in &vars.variables {
        if v.design_token_type.is_some_and(|hint| hint.variable_type() != v.kind) {
            return Err(error(&v.name, "designTokenType is incompatible with resolvedType"));
        }
        if !names.insert((&v.collection, &v.name)) {
            return Err(error(&v.name, "duplicate variable name"));
        }
        let c = vars.collection_by_id(&v.collection).ok_or_else(|| error(&v.name, "collection missing"))?;
        if v.values.is_empty() || v.values.keys().any(|m| c.mode_by_id(m).is_none()) {
            return Err(error(&v.name, "missing values or unknown mode"));
        }
        for (mode, value) in &v.values {
            if !valid_native_value(value) {
                return Err(error(&v.name, "invalid number or color value"));
            }
            if value.type_of().is_some_and(|t| t != v.kind) {
                return Err(error(&v.name, "value type mismatch"));
            }
            let mut stack = vec![(v.id.as_str(), false)];
            let mut active = BTreeSet::new();
            let mut done = BTreeSet::new();
            while let Some((id, exit)) = stack.pop() {
                steps += 1;
                if steps > MAX_NODES {
                    return Err(error("/", "reference traversal limit exceeded"));
                }
                if exit {
                    active.remove(id);
                    done.insert(id);
                    continue;
                }
                if done.contains(id) {
                    continue;
                }
                if !active.insert(id) || active.len() + done.len() > Variables::RESOLVE_LIMIT {
                    return Err(error(&v.name, "reference cycle or graph larger than the engine's 32-variable resolution limit"));
                }
                let target = by_id.get(id).ok_or_else(|| error(&v.name, "reference target missing"))?;
                let tc = vars.collection_by_id(&target.collection).ok_or_else(|| error(&v.name, "target collection missing"))?;
                let tm = if tc.id == c.id { Some(mode) } else { tc.default_mode() };
                let value = tm.and_then(|m| target.value_or_first(m)).ok_or_else(|| error(&target.name, "target value missing"))?;
                if value.type_of().is_some_and(|t| t != target.kind) {
                    return Err(error(&target.name, "value type mismatch"));
                }
                stack.push((id, true));
                for reference in value.references() {
                    let next =
                        by_id.get(reference.as_str()).ok_or_else(|| error(&target.name, "reference target is outside the exported collections"))?;
                    if next.kind != target.kind {
                        return Err(error(&target.name, "reference type mismatch"));
                    }
                    stack.push((reference, false));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtcg_reference_expansion_is_bounded_even_when_the_input_is_a_small_dag() {
        let root = json!({"leaf":["payload","payload"],"branch":[{"$ref":"#/leaf"},{"$ref":"#/leaf"}]});
        let value = json!([{"$ref":"#/branch"},{"$ref":"#/branch"}]);
        let err = component(&value, &root, "sample", &mut BTreeSet::new(), &mut (8, 1024), 0).unwrap_err();
        assert!(err.to_string().contains("reference expansion limit"));
        let err = component(&value, &root, "sample", &mut BTreeSet::new(), &mut (1024, 8), 0).unwrap_err();
        assert!(err.to_string().contains("text expansion limit"));
    }
}
