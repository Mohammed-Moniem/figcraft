//! Shared automation JSON contracts; no document model, engine, renderer or UI.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
use figcraft_color::Color;
use serde_json::{Value, json};

pub fn f64_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) => s.trim().trim_end_matches("px").trim_end_matches('%').trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Parse the last colon-delimited node-id component, preserving the native ID spelling contract.
pub fn parse_node_id(s: &str) -> Result<u64, std::num::ParseIntError> {
    let t = s.trim();
    let t = t.rsplit(':').next().unwrap_or(t);
    if t.starts_with('+') {
        return "-".parse::<u64>();
    }
    t.parse::<u64>()
}

pub fn id_of(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => parse_node_id(s).ok(),
        _ => None,
    }
}

pub fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::String(s) => Color::parse_css(s),
        Value::Object(_) => serde_json::from_value::<Color>(v.clone()).ok().map(Color::sanitized),
        Value::Array(a) => {
            let c = a.iter().map(|x| x.as_f64().map(|f| f as f32)).collect::<Option<Vec<f32>>>()?;
            match c.as_slice() {
                [r, g, b] => Some(Color::new(*r, *g, *b, 1.0)),
                [r, g, b, a] => Some(Color::new(*r, *g, *b, *a)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Read the friendlier paint/effect JSON that the docs and MCP tools describe as the engine's own
/// form: any `"color"` given as a CSS string ("#RRGGBB[AA]", "rgb(…)") becomes {r,g,b,a},
/// `{"type":"linearGradient"}` becomes `{"type":"gradient","kind":"linear"}`, gradients get default
/// handles (top to bottom) when `from`/`to`/`width` are missing, and stops given as plain colours
/// are spread evenly. Anything else is left for serde to refuse.
pub fn normalize_paint(v: &mut Value) {
    normalize_paintish(v, 0);
}

fn normalize_paintish(v: &mut Value, depth: usize) {
    if depth > 16 {
        return;
    }
    match v {
        Value::Array(a) => {
            for x in a {
                normalize_paintish(x, depth + 1);
            }
        }
        Value::Object(o) => {
            if let Some(c) = o.get("color").filter(|c| c.is_string()).and_then(color_of) {
                o.insert("color".into(), serde_json::to_value(c).unwrap_or(Value::Null));
            }
            let is_gradient = o.get("type").and_then(Value::as_str).is_some_and(|t| t.to_ascii_lowercase().contains("gradient"));
            if is_gradient {
                // Accept {"type":"linearGradient"} as well as {"type":"gradient","kind":"linear"}.
                if let Some(t) = o.get("type").and_then(Value::as_str).map(str::to_ascii_lowercase)
                    && t != "gradient"
                {
                    let kind = t.trim_end_matches("gradient").to_string();
                    o.insert("type".into(), json!("gradient"));
                    o.entry("kind").or_insert(json!(if kind.is_empty() { "linear".to_string() } else { kind }));
                }
                o.entry("kind").or_insert(json!("linear"));
                let linear = o.get("kind").and_then(Value::as_str) == Some("linear");
                let (from, to, width) = if linear { ([0.5, 0.0], [0.5, 1.0], [-0.5, 0.0]) } else { ([0.5, 0.5], [0.5, 1.0], [1.0, 0.5]) };
                o.entry("from").or_insert(json!(from));
                o.entry("to").or_insert(json!(to));
                // A linear gradient's width handle goes square to its axis (else its bands skew,
                // or vanish when the handle lies on the axis).
                let pt = |k: &str| o.get(k).and_then(Value::as_array).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                if linear
                    && !o.contains_key("width")
                    && let (Some(f), Some(t)) = (pt("from"), pt("to"))
                {
                    o.insert("width".into(), json!([f.0 - (t.1 - f.1), f.1 + (t.0 - f.0)]));
                }
                o.entry("width").or_insert(json!(width));
                if let Some(Value::Array(stops)) = o.get_mut("stops") {
                    let n = stops.len();
                    for (i, s) in stops.iter_mut().enumerate() {
                        if let Some(c) = color_of(s).filter(|_| s.is_string()) {
                            let pos = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
                            *s = json!({"position": pos, "color": c});
                        } else {
                            normalize_paintish(s, depth + 1);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Top-level node keys accepted by `fields`, including the structure kept in every tree.
pub const NODE_KEYS: &[&str] = &[
    "absolute",
    "absoluteBounds",
    "annotations",
    "arc",
    "background",
    "blend",
    "board",
    "booleanOperation",
    "boundVariables",
    "brush",
    "characters",
    "childCount",
    "children",
    "clip",
    "codeLinks",
    "componentProperties",
    "componentPropertyDefinitions",
    "constraints",
    "cornerRadius",
    "cornerSmoothing",
    "dashes",
    "dashPhase",
    "description",
    "devChanged",
    "devNote",
    "devResources",
    "devStatus",
    "effects",
    "exports",
    "fills",
    "flows",
    "fontFamily",
    "fontSize",
    "fontStyle",
    "fontWeight",
    "fontVariations",
    "gridCell",
    "grids",
    "hangingList",
    "hangingPunctuation",
    "height",
    "highlightColor",
    "id",
    "innerRadius",
    "interactions",
    "isMask",
    "layout",
    "leadingTrim",
    "letterSpacing",
    "lineHeight",
    "lineIndentations",
    "lineTypes",
    "lineWrapStyles",
    "listSpacing",
    "listStartOffsets",
    "listStyle",
    "locked",
    "mainComponent",
    "mainComponentName",
    "maskType",
    "maxHeight",
    "maxLines",
    "maxWidth",
    "measurements",
    "minHeight",
    "minWidth",
    "miterAngle",
    "motion",
    "name",
    "opacity",
    "operandOperation",
    "overrides",
    "pluginData",
    "pointCount",
    "rotation",
    "sectionContentsHidden",
    "segments",
    "sizing",
    "slot",
    "strokeAlign",
    "strokeJoin",
    "strokeSides",
    "strokeWeight",
    "strokes",
    "styledRuns",
    "styles",
    "textAlign",
    "textAutoResize",
    "textDecoration",
    "textDecorationColor",
    "textDecorationOffset",
    "textDecorationSkipInk",
    "textDecorationStyle",
    "textDecorationThickness",
    "textPath",
    "textStyleRanges",
    "type",
    "vertices",
    "visible",
    "width",
    "widthProfile",
    "wordBreak",
    "wrapStyle",
    "x",
    "y",
];

/// Which layer properties to build. Missing options keep the full, sparse inspection output.
#[derive(Clone, Debug, Default)]
pub struct InspectOptions {
    fields: Option<std::collections::BTreeSet<&'static str>>,
    outline: bool,
}

impl InspectOptions {
    pub fn outline(&self) -> bool {
        self.outline
    }

    /// Parse the same options for engine commands, control methods and MCP tools.
    pub fn parse(p: &Value) -> Result<Self, String> {
        let outline = match p.get("outline") {
            None => false,
            Some(Value::Bool(v)) => *v,
            Some(_) => return Err("`outline` must be a boolean".into()),
        };
        if outline && p.get("fields").is_some() {
            return Err("`outline: true` cannot be combined with `fields`".into());
        }
        let fields = match p.get("fields") {
            None if outline => Some(["id", "name", "type", "characters", "mainComponentName", "visible"].into_iter().collect()),
            None => None,
            Some(Value::Array(values)) => {
                let mut fields = std::collections::BTreeSet::new();
                for value in values {
                    let key = value.as_str().ok_or("`fields` must be an array of strings")?;
                    let key = NODE_KEYS
                        .iter()
                        .copied()
                        .find(|k| *k == key)
                        .ok_or_else(|| format!("unknown field `{key}`; valid keys: {}", NODE_KEYS.join(", ")))?;
                    fields.insert(key);
                }
                Some(fields)
            }
            Some(_) => return Err("`fields` must be an array of strings".into()),
        };
        Ok(Self { fields, outline })
    }

    pub fn includes(&self, key: &str) -> bool {
        self.fields.as_ref().is_none_or(|fields| fields.contains(key))
    }

    pub fn explicit(&self, key: &str) -> bool {
        self.fields.as_ref().is_some_and(|fields| fields.contains(key))
    }
}

#[cfg(test)]
mod tests;
