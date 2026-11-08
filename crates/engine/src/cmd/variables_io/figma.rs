//! Figma mode JSON is a documented variables dialect, distinct from strict standard DTCG.
//! Normalize into generated token identities, then reuse the bounded, typed DTCG reader.
//! Imported producer IDs never become native identities. Mode merging belongs to the command.

use crate::{Result, bad};
use figcraft_color::ColorProfile;
use figcraft_doc::{Collection, DesignTokenType, Document, VariableType, VariableValue, Variables};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_TOKENS: usize = 100000;
const TYPE: &str = "com.figma.type";
const ALIAS: &str = "com.figma.aliasData";

pub(super) struct Imported {
    pub variables: Variables,
    pub warnings: Vec<String>,
    pub external: BTreeMap<String, String>,
}

fn error(path: &str, why: &str) -> crate::EngineError {
    bad("variable.import", format!("Figma tokens {path}: {why}"))
}

pub(super) fn detected(root: &Value) -> bool {
    let mut pending = vec![root];
    let mut visited = 0;
    while let Some(v) = pending.pop() {
        visited += 1;
        if visited > 2000000 {
            return false;
        }
        if let Some(o) = v.as_object() {
            if matches!(o.get("$type").and_then(Value::as_str), Some("string"))
                || o.get("$extensions").and_then(Value::as_object).is_some_and(|e| e.keys().any(|k| k.starts_with("com.figma")))
            {
                return true;
            }
            if pending.len().saturating_add(o.len()) > 2000000 {
                return false;
            }
            pending.extend(o.values());
        }
    }
    false
}

#[derive(Clone)]
struct Token {
    path: String,
    name: String,
    kind: String,
    value: Value,
    boolean: bool,
    description: String,
    alias: Option<Value>,
}

fn collect(
    v: &Value,
    path: &mut Vec<String>,
    inherited: Option<&str>,
    tokens: &mut Vec<Token>,
    unknown: &mut usize,
    names: &mut usize,
) -> Result<()> {
    let bytes = path.iter().fold(path.len().saturating_sub(1), |n, p| n.saturating_add(p.len()));
    if bytes > 4096 {
        return Err(error("/", "normalized token names must be at most4096 bytes"));
    }
    let label = path.join(".");
    let o = v.as_object().ok_or_else(|| error(&label, "group or token must be an object"))?;
    if o.contains_key("$extends") || o.contains_key("$ref") || o.contains_key("$root") {
        return Err(error(&label, "$extends, $ref and $root are not supported in Figma mode JSON"));
    }
    *unknown += o.keys().filter(|k| k.starts_with('$') && !matches!(k.as_str(), "$type" | "$value" | "$description" | "$extensions")).count();
    let kind = match o.get("$type") {
        None => inherited,
        Some(Value::String(s)) => Some(s.as_str()),
        _ => return Err(error(&label, "$type must be a string")),
    };
    let ext = match o.get("$extensions") {
        None => None,
        Some(Value::Object(e)) => Some(e),
        _ => return Err(error(&label, "$extensions must be an object")),
    };
    if ext.is_some_and(|e| e.contains_key("ai.storyteller.figcraft")) {
        return Err(error(&label, "native identity extensions are not accepted by the Figma adapter"));
    }
    if let Some(e) = ext {
        *unknown += e.keys().filter(|k| k.as_str() != TYPE && k.as_str() != ALIAS).count();
    }
    if let Some(value) = o.get("$value") {
        if path.is_empty() || o.keys().any(|k| !k.starts_with('$')) {
            return Err(error(&label, "token must have a name and cannot contain child groups"));
        }
        if tokens.len() >= MAX_TOKENS {
            return Err(error(&label, "more than 100000 tokens"));
        }
        *names = names.saturating_add(bytes.saturating_mul(2));
        if *names > 64 * 1024 * 1024 {
            return Err(error("/", "normalized names exceed64 MiB"));
        }
        let kind = kind.ok_or_else(|| error(&label, "$type is required"))?;
        let boolean = match ext.and_then(|e| e.get(TYPE)) {
            None => false,
            Some(Value::String(s)) if s == "boolean" && kind == "number" => true,
            Some(Value::String(s)) if s == "string" && matches!(kind, "string" | "fontFamily") => false,
            _ => return Err(error(&label, "com.figma.type must match a numeric boolean or string token")),
        };
        if !matches!(kind, "color" | "number" | "dimension" | "duration" | "fontFamily" | "string") || boolean && kind != "number" {
            return Err(error(&label, "unsupported token type or boolean extension"));
        }
        let description = match o.get("$description") {
            None => String::new(),
            Some(Value::String(s)) => s.clone(),
            _ => return Err(error(&label, "$description must be a string")),
        };
        tokens.push(Token {
            path: label,
            name: path.join("/"),
            kind: kind.into(),
            value: value.clone(),
            boolean,
            description,
            alias: ext.and_then(|e| e.get(ALIAS)).cloned(),
        });
    } else {
        if ext.is_some_and(|e| e.contains_key(TYPE) || e.contains_key(ALIAS)) {
            return Err(error(&label, "Figma type/alias extensions must belong to a token"));
        }
        for (key, child) in o {
            if key.starts_with('$') {
                continue;
            }
            if key.is_empty() || key.contains(['.', '{', '}']) {
                return Err(error(&label, "token names cannot contain periods or braces"));
            }
            path.push(key.clone());
            collect(child, path, kind, tokens, unknown, names)?;
            path.pop();
        }
    }
    Ok(())
}

fn reference(v: &Value) -> Option<&str> {
    v.as_str()?.strip_prefix('{')?.strip_suffix('}')
}
fn string_param(p: &Value, key: &str, default: &str) -> Result<String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default.into()),
        Some(Value::String(s)) if !s.trim().is_empty() && s.chars().count() <= 512 => Ok(s.clone()),
        _ => Err(error(key, "expected a nonempty string of at most512 characters")),
    }
}

pub(super) fn import(root: &Value, p: &Value, doc: &Document) -> Result<Imported> {
    super::dtcg::bounded(root)?;
    let collection = string_param(p, "collection", "Tokens")?;
    let existing = doc.variables.collection(&collection);
    if existing.is_some_and(|c| c.extends.is_some() || crate::library::is_remote_collection(doc, &c.id)) {
        return Err(error(&collection, "choose a writable local collection"));
    }
    let collection = existing.map_or(collection, |c| c.name.clone());
    let mode = string_param(p, "mode", "Default")?;
    let mut tokens = Vec::new();
    let mut unknown = 0;
    collect(root, &mut Vec::new(), None, &mut tokens, &mut unknown, &mut 0)?;
    if tokens.is_empty() {
        return Err(error("/", "no tokens"));
    }
    let mut names = BTreeSet::new();
    for token in &tokens {
        if !names.insert(token.name.clone()) {
            return Err(error(&token.path, "duplicate normalized variable name"));
        }
    }
    let paths: BTreeMap<_, _> = tokens.iter().enumerate().map(|(i, t)| (t.path.clone(), i)).collect();
    let mut normalized = Map::new();
    let mut external = BTreeMap::new();
    let mut external_for = BTreeMap::new();
    for (i, t) in tokens.iter().enumerate() {
        let ty = match t.kind.as_str() {
            "color" => "color",
            "fontFamily" | "string" => "fontFamily",
            _ => "number",
        };
        let mut value = if let Some(target) = reference(&t.value) {
            let index = paths.get(target).ok_or_else(|| error(&t.path, "alias target is missing"))?;
            json!(format!("{{Figma.t{index}}}"))
        } else {
            match t.kind.as_str() {
                "dimension" | "duration" => {
                    let n = t.value["value"].as_f64().filter(|n| n.is_finite()).ok_or_else(|| error(&t.path, "expected a finite unit value"))?;
                    let n = match (t.kind.as_str(), t.value["unit"].as_str()) {
                        ("dimension", Some("px")) => n,
                        ("duration", Some("s")) if n >= 0.0 => n,
                        ("duration", Some("ms")) if n >= 0.0 => n / 1000.0,
                        _ => return Err(error(&t.path, "Figma dimensions require px and nonnegative durations require s or ms")),
                    };
                    json!(n)
                }
                "color" => {
                    if !matches!(t.value["colorSpace"].as_str(), Some("srgb" | "hsl")) {
                        return Err(error(&t.path, "Figma mode colours require srgb or hsl"));
                    }
                    t.value.clone()
                }
                _ => t.value.clone(),
            }
        };
        if let Some(data) = &t.alias {
            let data = data.as_object().ok_or_else(|| error(&t.path, "com.figma.aliasData must be an object"))?;
            if data.values().any(|v| !v.is_string()) {
                return Err(error(&t.path, "aliasData fields must be strings"));
            }
            let by_id = data.get("targetVariableID").and_then(Value::as_str).and_then(|id| doc.variables.get_by_id(id));
            let find_named = |coll: Option<&Collection>| {
                let name = data.get("targetVariableName").and_then(Value::as_str)?;
                doc.variables.variables.iter().find(|v| coll.is_some_and(|c| c.id == v.collection) && v.name == name)
            };
            let collection_by_id = data.get("targetVariableSetID").and_then(Value::as_str).and_then(|id| doc.variables.collection_by_id(id));
            let target = by_id
                .or_else(|| {
                    let collection = collection_by_id.or_else(|| {
                        data.get("targetVariableSetName")
                            .and_then(Value::as_str)
                            .and_then(|name| doc.variables.collections.iter().find(|c| c.name == name))
                    });
                    find_named(collection)
                })
                .ok_or_else(|| error(&t.path, "external alias target was not found locally; import its collection first"))?;
            if doc
                .variables
                .collection_by_id(&target.collection)
                .is_none_or(|c| c.extends.is_some() || crate::library::is_remote_collection(doc, &c.id))
            {
                return Err(error(&t.path, "external aliases require a local writable collection"));
            }
            let placeholder = format!("Figma:External:{i}");
            external.insert(placeholder.clone(), target.id.clone());
            external_for.insert(i, placeholder);
            // The generic reader validates the fallback literal; the explicit external link is
            // installed on the isolated result after all token kinds have been resolved.
            if reference(&value).is_some() {
                return Err(error(&t.path, "external alias requires a literal fallback value"));
            }
        }
        if t.boolean && reference(&value).is_none() {
            let n = value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| error(&t.path, "boolean requires a finite numeric value"))?;
            value = json!(n);
        }
        normalized.insert(format!("t{i}"), json!({"$type":ty,"$value":value,"$description":t.description}));
    }
    let (mut vars, mut warnings) = super::dtcg::import(&json!({"Figma":normalized}), &json!({}), doc.color_profile)?;
    let coll = vars.collections.first_mut().ok_or_else(|| error("/", "missing normalized collection"))?;
    coll.name = collection;
    let first_mode = coll.modes.first_mut().ok_or_else(|| error("/", "missing normalized mode"))?;
    first_mode.name = mode;
    let mut mapped = BTreeMap::new();
    for v in &mut vars.variables {
        let index = v.name.strip_prefix('t').and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| error("/", "invalid normalized token identity"))?;
        let t = tokens.get(index).ok_or_else(|| error("/", "missing normalized token"))?;
        mapped.insert(index, v.id.clone());
        v.name = t.name.clone();
        v.design_token_type = DesignTokenType::parse(&t.kind);
        if t.boolean {
            v.kind = VariableType::Boolean;
            v.design_token_type = None;
            for value in v.values.values_mut() {
                if let VariableValue::Number(n) = value {
                    *value = VariableValue::Boolean(*n != 0.0);
                }
            }
        }
    }
    for (i, placeholder) in external_for {
        let id = mapped.get(&i).ok_or_else(|| error("/", "missing external token mapping"))?;
        let target = external.get(&placeholder).and_then(|id| doc.variables.get_by_id(id)).ok_or_else(|| error("/", "missing external variable"))?;
        let var = vars.variables.iter_mut().find(|v| v.id == *id).ok_or_else(|| error("/", "missing external variable slot"))?;
        if var.kind != target.kind {
            return Err(error(&var.name, "external alias has a conflicting type"));
        }
        for value in var.values.values_mut() {
            *value = VariableValue::Alias(placeholder.clone());
        }
    }
    for var in &vars.variables {
        for value in var.values.values() {
            if let VariableValue::Alias(id) = value {
                let kind = vars.get_by_id(id).map(|v| v.kind).or_else(|| external.get(id).and_then(|id| doc.variables.get_by_id(id)).map(|v| v.kind));
                if kind != Some(var.kind) {
                    return Err(error(&var.name, "alias has a conflicting variable type"));
                }
            }
        }
    }
    if unknown > 0 {
        warnings.push(format!("{unknown} metadata fields were not retained; producer IDs do not become native identities"));
    }
    Ok(Imported { variables: vars, warnings, external })
}

pub(super) fn export(doc: &Document, collections: &[&Collection], p: &Value) -> Result<(String, Vec<String>)> {
    let [coll] = collections else {
        return Err(bad("variable.export", "Figma mode export requires exactly one collection"));
    };
    let mode = match p.get("mode") {
        None | Some(Value::Null) => coll.modes.first(),
        Some(Value::String(m)) => coll.find_mode(m),
        _ => None,
    }
    .ok_or_else(|| bad("variable.export", "choose an existing mode"))?;
    let modes = BTreeMap::from([(coll.id.clone(), mode.id.clone())]);
    let vars = &doc.variables;
    let mut out = Map::new();
    let mut warnings = Vec::new();
    let mut count = 0usize;
    let mut text = 0usize;
    for var in vars.variables.iter().filter(|v| v.collection == coll.id) {
        count += 1;
        text = text.saturating_add(var.name.len()).saturating_add(var.description.len());
        if let Some(VariableValue::String(value)) = vars.resolve(&var.id, &modes) {
            text = text.saturating_add(value.len());
        }
        if count > MAX_TOKENS || text > 64 * 1024 * 1024 {
            return Err(bad("variable.export", "Figma mode resource limit (100000 tokens, 64 MiB of text) exceeded"));
        }
    }
    if count == 0 {
        return Err(bad("variable.export", "selected collection has no variables"));
    }
    for var in vars.variables.iter().filter(|v| v.collection == coll.id) {
        if var.name.is_empty() || var.name.len() > 4096 || var.name.starts_with('$') || var.name.contains(['.', '{', '}']) {
            return Err(bad("variable.export", format!("{}: name cannot be represented unambiguously in Figma token JSON", var.name)));
        }
        let kind = match var.kind {
            VariableType::Color => "color",
            VariableType::Boolean => "number",
            VariableType::String => {
                if var.design_token_type == Some(DesignTokenType::FontFamily) {
                    "fontFamily"
                } else {
                    "string"
                }
            }
            VariableType::Number => match var.design_token_type {
                Some(DesignTokenType::Dimension) => "dimension",
                Some(DesignTokenType::Duration) => "duration",
                _ => "number",
            },
        };
        let mut ext = Map::new();
        if var.kind == VariableType::Boolean {
            ext.insert(TYPE.into(), json!("boolean"));
        }
        let raw = var.value_or_first(&mode.id);
        if matches!(raw, Some(VariableValue::ColorAlpha { .. } | VariableValue::ColorMix { .. })) {
            warnings.push(format!("{}: colour calculation was flattened to a literal", var.name));
        }
        if let Some(VariableValue::Alias(id)) = raw {
            let target = vars.get_by_id(id).ok_or_else(|| bad("variable.export", "alias target is missing"))?;
            if target.kind != var.kind || vars.resolve(&var.id, &modes).is_none() {
                return Err(bad("variable.export", "alias has a cycle or conflicting type"));
            }
        }
        let local_alias = raw
            .and_then(|v| match v {
                VariableValue::Alias(id) => vars.get_by_id(id),
                _ => None,
            })
            .filter(|target| target.collection == coll.id);
        let value = if let Some(target) = local_alias {
            json!(format!("{{{}}}", target.name))
        } else {
            if let Some(VariableValue::Alias(id)) = raw {
                let target = vars.get_by_id(id).ok_or_else(|| bad("variable.export", "alias target is missing"))?;
                let collection = vars.collection_by_id(&target.collection).ok_or_else(|| bad("variable.export", "alias collection is missing"))?;
                ext.insert(ALIAS.into(),json!({"targetVariableID":target.id,"targetVariableSetID":collection.id,"targetVariableSetName":collection.name,"targetVariableName":target.name}));
                warnings.push(format!("{}: external alias requires a matching accessible collection at the destination", var.name));
            }
            match vars.resolve(&var.id, &modes) {
                Some(VariableValue::Color(c)) if var.kind == VariableType::Color => {
                    let c = doc.color_profile.conversion_to(ColorProfile::Srgb).apply(c);
                    if [c.r, c.g, c.b, c.a].iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&c.a) {
                        return Err(bad("variable.export", "colour components and alpha must be finite; alpha must be in 0..1"));
                    }
                    let components = [c.r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), c.b.clamp(0.0, 1.0)];
                    if components != [c.r, c.g, c.b] {
                        warnings.push(format!("{}: colour was clipped to the sRGB gamut", var.name));
                    }
                    json!({"colorSpace":"srgb","components":components,"alpha":c.a})
                }
                Some(VariableValue::Boolean(v)) if var.kind == VariableType::Boolean => json!(if v { 1 } else { 0 }),
                Some(VariableValue::String(s)) if var.kind == VariableType::String => {
                    if reference(&json!(s)).is_some() {
                        return Err(bad("variable.export", "string literal uses reserved alias syntax"));
                    }
                    json!(s)
                }
                Some(VariableValue::Number(n)) if n.is_finite() && var.kind == VariableType::Number => match kind {
                    "dimension" => json!({"value":n,"unit":"px"}),
                    "duration" if n >= 0.0 => json!({"value":n,"unit":"s"}),
                    "duration" => return Err(bad("variable.export", "duration must be nonnegative")),
                    _ => json!(n),
                },
                _ => return Err(bad("variable.export", format!("{}: value cannot be resolved", var.name))),
            }
        };
        let mut token = json!({"$type":kind,"$value":value,"$description":var.description});
        if !ext.is_empty() {
            token["$extensions"] = Value::Object(ext);
        }
        if out.insert(var.name.clone(), token).is_some() {
            return Err(bad("variable.export", "duplicate variable names"));
        }
    }
    if !doc.color_profile.is_srgb() {
        warnings.push("Figma mode colours were converted to sRGB".into());
    }
    let output = Value::Object(out);
    super::dtcg::bounded(&output)?;
    let mut bytes = JsonOutput(Vec::new());
    serde_json::to_writer_pretty(&mut bytes, &output).map_err(|e| bad("variable.export", e.to_string()))?;
    let text = String::from_utf8(bytes.0).map_err(|e| bad("variable.export", e.to_string()))?;
    Ok((text, warnings))
}

struct JsonOutput(Vec<u8>);
impl std::io::Write for JsonOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 64 * 1024 * 1024 {
            return Err(std::io::Error::other("Figma mode JSON exceeds64 MiB"));
        }
        self.0.try_reserve(bytes.len()).map_err(std::io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn figma_writer_refuses_oversized_chunks_before_allocating() {
        use std::io::Write;
        let mut writer = super::JsonOutput(Vec::new());
        assert!(writer.write(&vec![b'x'; 64 * 1024 * 1024 + 1]).is_err());
        assert!(writer.0.is_empty());
    }
}
