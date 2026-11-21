//! Read validated display geometry while the native blob table is still alive. Ordinary REST
//! imports cannot supply this sidecar. Its keys are the final REST IDs of expanded text layers.
use super::kiwi::Kv;
use figcraft_doc::{NativeFontMeta, NativeFontName, NativeTextBaseline, NativeTextDecoration, NativeTextGlyph, NativeTextLayout};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub(crate) type Layouts = BTreeMap<String, Arc<NativeTextLayout>>;
#[derive(Default)]
pub(crate) struct TextResources {
    pub layouts: Layouts,
    pub faces: BTreeMap<String, NativeFontName>,
}
type Read<T> = Result<T, &'static str>;
#[derive(Clone, Copy, Default, Hash, Eq, PartialEq)]
pub(super) struct TextVersion {
    pub user: Option<u32>,
    pub explicit: Option<u32>,
    pub tracking_bits: Option<u32>,
}
const INVALID: &str = "invalid native text display geometry was skipped";
const LIMIT: &str = "native text display geometry beyond the import budget was skipped";
const MAX_PLACEMENTS: usize = 1_000_000;
const MAX_BLOB_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Reader {
    pub layouts: Layouts,
    pub faces: BTreeMap<String, NativeFontName>,
    face_bytes: usize,
    outlines: HashMap<usize, Arc<[u8]>>,
    // The decoder's immutable message lives for this walk. Reused underlying records, including
    // expanded component layers, use the same layout at the same scale.
    records: HashMap<(usize, usize, u64, TextVersion), Arc<NativeTextLayout>>,
    placements: usize,
    blob_bytes: usize,
    text_bytes: usize,
    attachment_bytes: usize,
}

fn uint(v: &Kv) -> Read<u32> {
    match v {
        Kv::Uint(n) => u32::try_from(*n).map_err(|_| INVALID),
        _ => Err(INVALID),
    }
}

fn scalar(v: &Kv, scale: f64) -> Read<f32> {
    let Kv::Float(f) = v else { return Err(INVALID) };
    let value = (f64::from(*f) * scale) as f32;
    value.is_finite().then_some(value).ok_or(INVALID)
}

fn number(v: &Kv, key: &str, scale: f64) -> Read<Option<f32>> {
    v.get(key).map(|v| scalar(v, scale)).transpose()
}

fn vector(v: &Kv, key: &str, scale: f64) -> Read<Option<[f32; 2]>> {
    v.get(key).map(|v| Ok([scalar(v.get("x").ok_or(INVALID)?, scale)?, scalar(v.get("y").ok_or(INVALID)?, scale)?])).transpose()
}

fn array<'a, 's>(v: &'a Kv<'s>, key: &str) -> Read<&'a [Kv<'s>]> {
    match v.get(key) {
        Some(Kv::Array(values)) => Ok(values),
        None => Ok(&[]),
        _ => Err(INVALID),
    }
}

fn index(v: &Kv, key: &str) -> Read<Option<u32>> {
    v.get(key).map(uint).transpose()
}

fn baseline(v: &Kv, scale: f64) -> Read<NativeTextBaseline> {
    if !matches!(v, Kv::Object(_)) {
        return Err(INVALID);
    }
    Ok(NativeTextBaseline {
        position: vector(v, "position", scale)?,
        width: number(v, "width", scale)?,
        line_y: number(v, "lineY", scale)?,
        line_height: number(v, "lineHeight", scale)?,
        line_ascent: number(v, "lineAscent", scale)?,
        ignore_leading_trim: number(v, "ignoreLeadingTrim", scale)?,
        first_character: index(v, "firstCharacter")?,
        end_character: index(v, "endCharacter")?,
    })
}

fn font_name(v: &Kv) -> Read<NativeFontName> {
    let field = |k| v.get(k).and_then(Kv::str).filter(|s| s.len() <= 256 && !s.chars().any(char::is_control)).map(str::to_owned).ok_or(INVALID);
    let name = NativeFontName { family: field("family")?, style: field("style")?, postscript: field("postscript")? };
    name.is_valid().then_some(name).ok_or(INVALID)
}

fn font_meta(v: &Kv) -> Read<NativeFontMeta> {
    let digest = v.get("fontDigest").map(|b| b.bytes().filter(|d| d.len() <= 128).map(<[u8]>::to_vec).ok_or(INVALID)).transpose()?;
    let style = v.get("fontStyle").map(|x| x.str().filter(|s| matches!(*s, "NORMAL" | "ITALIC")).map(str::to_owned).ok_or(INVALID)).transpose()?;
    let weight = v
        .get("fontWeight")
        .map(|x| match x {
            Kv::Int(n) => i32::try_from(*n).map_err(|_| INVALID),
            _ => Err(INVALID),
        })
        .transpose()?;
    let meta = NativeFontMeta {
        key: v.get("key").map(font_name).transpose()?,
        font_line_height: number(v, "fontLineHeight", 1.0)?,
        font_digest: digest,
        font_style: style,
        font_weight: weight,
    };
    meta.is_valid().then_some(meta).ok_or(INVALID)
}

impl Reader {
    pub fn capture_face(&mut self, id: &str, source: &Kv) -> Read<()> {
        let name = font_name(source)?;
        self.face_bytes = self
            .face_bytes
            .saturating_add(id.len())
            .saturating_add(name.family.len())
            .saturating_add(name.style.len())
            .saturating_add(name.postscript.len());
        if self.face_bytes > MAX_TEXT_BYTES || self.faces.len() >= MAX_PLACEMENTS {
            return Err("native font names beyond the import budget were skipped");
        }
        self.faces.insert(id.to_owned(), name);
        Ok(())
    }

    pub fn capture(&mut self, id: &str, chars: &str, text: &Kv, data: &Kv, scale: f64, blobs: &[Kv], version: TextVersion) -> Read<()> {
        let glyphs = array(data, "glyphs")?;
        if glyphs.is_empty() {
            return Ok(());
        }
        if self.placements >= MAX_PLACEMENTS {
            return Err(LIMIT);
        }
        let baselines = array(data, "baselines")?;
        let mut records = glyphs.len().saturating_add(baselines.len());
        for key in ["logicalIndexToCharacterOffsetMap", "derivedLines", "fontMetaData"] {
            records = records.saturating_add(array(data, key)?.len());
        }
        let decorations = array(data, "decorations")?;
        if decorations.len() > NativeTextLayout::MAX_GLYPHS {
            return Err(LIMIT);
        }
        self.placements = self.placements.saturating_add(records).saturating_add(decorations.len());
        self.text_bytes = self.text_bytes.saturating_add(chars.len()).saturating_add(id.len());
        if self.placements > MAX_PLACEMENTS
            || self.text_bytes > MAX_TEXT_BYTES
            || glyphs.len() > NativeTextLayout::MAX_GLYPHS
            || baselines.len() > NativeTextLayout::MAX_GLYPHS
            || chars.len() > NativeTextLayout::MAX_GLYPHS * 4
        {
            return Err(LIMIT);
        }
        for row in decorations {
            self.placements = self.placements.saturating_add(array(row, "rects")?.len());
            if self.placements > MAX_PLACEMENTS {
                return Err(LIMIT);
            }
        }
        let record = (std::ptr::from_ref(text) as usize, std::ptr::from_ref(data) as usize, scale.to_bits(), version);
        if let Some(layout) = self.records.get(&record) {
            self.attachment_bytes = self.attachment_bytes.saturating_add(layout.outlines.iter().map(|b| b.len()).sum::<usize>());
            if self.attachment_bytes > 2 * MAX_BLOB_BYTES {
                return Err(LIMIT);
            }
            self.layouts.insert(id.to_owned(), layout.clone());
            return Ok(());
        }
        let ids = array(text, "characterStyleIDs")?;
        if ids.len() > NativeTextLayout::MAX_GLYPHS * 2 {
            return Err(LIMIT);
        }
        let mut style_units = BTreeMap::new();
        for (i, v) in ids.iter().enumerate() {
            style_units.entry(uint(v)?).or_insert(i as u32);
        }
        let logical_offsets = data
            .get("logicalIndexToCharacterOffsetMap")
            .map(|_| {
                let values = array(data, "logicalIndexToCharacterOffsetMap")?;
                if values.len() > 2 * NativeTextLayout::MAX_GLYPHS + 1 {
                    return Err(LIMIT);
                }
                values.iter().map(|x| scalar(x, scale)).collect::<Read<Vec<_>>>()
            })
            .transpose()?;
        let derived_lines = data
            .get("derivedLines")
            .map(|_| {
                let lines = array(data, "derivedLines")?;
                if lines.len() > NativeTextLayout::MAX_GLYPHS {
                    return Err(LIMIT);
                }
                lines
                    .iter()
                    .map(|x| x.get("directionality").and_then(Kv::str).filter(|s| matches!(*s, "LTR" | "RTL")).map(str::to_owned).ok_or(INVALID))
                    .collect::<Read<Vec<_>>>()
            })
            .transpose()?;
        let truncation_start_index = data
            .get("truncationStartIndex")
            .map(|x| match x {
                Kv::Int(i) => i32::try_from(*i).map_err(|_| INVALID),
                _ => Err(INVALID),
            })
            .transpose()?;
        let truncated_height =
            data.get("truncatedHeight").map(|v| if matches!(v, Kv::Float(x) if *x == -1.0) { Ok(-1.0) } else { scalar(v, scale) }).transpose()?;
        let decorations = data
            .get("decorations")
            .map(|_| {
                let rows = array(data, "decorations")?;
                if rows.len() > NativeTextLayout::MAX_GLYPHS {
                    return Err(LIMIT);
                }
                let mut count = 0usize;
                rows.iter()
                    .map(|d| {
                        let style_id = index(d, "styleID")?;
                        let style_character = match style_id {
                            Some(style) if style != 0 => Some(*style_units.get(&style).ok_or(INVALID)?),
                            _ => None,
                        };
                        let rects = array(d, "rects")?;
                        count = count.saturating_add(rects.len());
                        if count > NativeTextLayout::MAX_GLYPHS {
                            return Err(LIMIT);
                        }
                        let rects = rects
                            .iter()
                            .map(|rect| {
                                let length = |k| scalar(rect.get(k).ok_or(INVALID)?, scale);
                                Ok([length("x")?, length("y")?, length("w")?, length("h")?])
                            })
                            .collect::<Read<Vec<_>>>()?;
                        Ok(NativeTextDecoration { rects, style_id, style_character })
                    })
                    .collect::<Read<Vec<_>>>()
            })
            .transpose()?;
        let fonts = array(data, "fontMetaData").and_then(|rows| {
            if rows.len() > 256 {
                return Err(LIMIT);
            }
            rows.iter().map(font_meta).collect::<Read<Vec<_>>>()
        });
        let invalid_fonts = fonts.is_err();
        let mut layout = NativeTextLayout {
            user_layout_version: version.user,
            explicit_layout_version: version.explicit,
            text_tracking: version.tracking_bits.map(f32::from_bits),
            logical_offsets,
            derived_lines,
            truncation_start_index,
            truncated_height,
            decorations,
            font_meta_data: fonts.unwrap_or_default(),
            layout_size: vector(data, "layoutSize", scale)?,
            glyphs: Vec::with_capacity(glyphs.len()),
            outlines: Vec::new(),
            baselines: baselines.iter().map(|b| baseline(b, scale)).collect::<Read<Vec<_>>>()?,
        };
        let mut indices = HashMap::new();
        for g in glyphs {
            if !matches!(g, Kv::Object(fields) if !fields.is_empty()) {
                return Err(INVALID);
            }
            let outline = match index(g, "commandsBlob")? {
                Some(id) => {
                    let id = id as usize;
                    if let Some(&local) = indices.get(&id) {
                        Some(local)
                    } else {
                        let bytes = blobs.get(id).and_then(|b| b.get("bytes")).and_then(Kv::bytes).ok_or(INVALID)?;
                        let retained = match self.outlines.get(&id) {
                            Some(outline) => outline.clone(),
                            None => {
                                self.blob_bytes = self.blob_bytes.saturating_add(bytes.len());
                                if self.blob_bytes > MAX_BLOB_BYTES {
                                    return Err(LIMIT);
                                }
                                if !NativeTextLayout::valid_outline(bytes) {
                                    return Err(INVALID);
                                }
                                let outline: Arc<[u8]> = bytes.into();
                                self.outlines.insert(id, outline.clone());
                                outline
                            }
                        };
                        let local = layout.outlines.len();
                        indices.insert(id, local);
                        layout.outlines.push(retained);
                        Some(local)
                    }
                }
                None => None,
            };
            let first_character = index(g, "firstCharacter")?;
            let style_id = index(g, "styleID")?;
            let style_character = match style_id {
                Some(style) if style != 0 => first_character.or_else(|| style_units.get(&style).copied()).ok_or(INVALID).map(Some)?,
                _ => None,
            };
            let codes = array(g, "emojiCodePoints")?;
            if codes.len() > 64 {
                return Err(INVALID);
            }
            let emoji_image_set =
                g.get("emojiImageSet").map(|x| x.str().filter(|s| matches!(*s, "APPLE" | "NOTO")).ok_or(INVALID).map(str::to_owned)).transpose()?;
            layout.glyphs.push(NativeTextGlyph {
                outline,
                position: vector(g, "position", scale)?,
                style_id,
                style_character,
                font_size: number(g, "fontSize", scale)?,
                first_character,
                advance: number(g, "advance", scale)?,
                rotation: number(g, "rotation", 1.0)?,
                emoji_code_points: codes.iter().map(uint).collect::<Read<Vec<_>>>()?,
                emoji_image_set,
            });
        }
        if !layout.is_valid(chars) {
            return Err(INVALID);
        }
        self.attachment_bytes = self.attachment_bytes.saturating_add(layout.outlines.iter().map(|b| b.len()).sum::<usize>());
        if self.attachment_bytes > 2 * MAX_BLOB_BYTES {
            return Err(LIMIT);
        }
        let layout = Arc::new(layout);
        self.layouts.insert(id.to_owned(), layout.clone());
        self.records.insert(record, layout);
        if invalid_fonts { Err("invalid native font metadata was skipped") } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_text_skips_missing_or_damaged_blob_and_respects_the_whole_import_budget() {
        let text = Kv::Object(vec![("characters", Kv::Str("a".into()))]);
        let glyph = Kv::Object(vec![("commandsBlob", Kv::Uint(0)), ("firstCharacter", Kv::Uint(0))]);
        let data = Kv::Object(vec![("glyphs", Kv::Array(vec![glyph]))]);
        for blobs in [vec![], vec![Kv::Object(vec![])], vec![Kv::Object(vec![("bytes", Kv::Bytes(vec![4]))])]] {
            let mut reader = Reader::default();
            assert_eq!(reader.capture("1:2", "a", &text, &data, 1.0, &blobs, Default::default()), Err(INVALID));
            assert!(reader.layouts.is_empty());
        }
        let mut reader = Reader { placements: MAX_PLACEMENTS, ..Default::default() };
        assert_eq!(reader.capture("1:2", "a", &text, &data, 1.0, &[], Default::default()), Err(LIMIT));
        assert!(reader.outlines.is_empty());
        let companion = Kv::Object(vec![
            ("glyphs", Kv::Array(vec![Kv::Object(vec![("firstCharacter", Kv::Uint(0))])])),
            ("logicalIndexToCharacterOffsetMap", Kv::Array(vec![Kv::Float(0.0), Kv::Float(3.0)])),
        ]);
        let mut reader = Reader { placements: MAX_PLACEMENTS - 2, ..Default::default() };
        assert_eq!(reader.capture("1:2", "a", &text, &companion, 1.0, &[], Default::default()), Err(LIMIT));
        assert!(reader.layouts.is_empty());
    }
}
