//! Native layout geometry from a still-current imported text. Blob indices belong to the
//! document being written. Reusing source indices would point at unrelated vector outlines.

use figcraft_doc::text::{NativeTextBaseline, NativeTextLayout};

use super::{Exporter, Fields, Kv, arr, e, o, u};

fn point([x, y]: [f32; 2]) -> Kv<'static> {
    o(vec![("x", Kv::Float(x)), ("y", Kv::Float(y))])
}

fn baseline(line: &NativeTextBaseline) -> Kv<'static> {
    let mut row = Fields::new();
    row.extend(line.position.map(|p| ("position", point(p))));
    for (field, value) in [
        ("width", line.width),
        ("lineY", line.line_y),
        ("lineHeight", line.line_height),
        ("lineAscent", line.line_ascent),
        ("ignoreLeadingTrim", line.ignore_leading_trim),
    ] {
        row.extend(value.map(|v| (field, Kv::Float(v))));
    }
    row.extend(line.first_character.map(|n| ("firstCharacter", u(n))));
    row.extend(line.end_character.map(|n| ("endCharacter", u(n))));
    o(row)
}

impl Exporter<'_> {
    pub(super) fn native_text_layout(&mut self, layout: &NativeTextLayout, style_ids: &[Kv<'static>]) -> Option<Kv<'static>> {
        if !self.budget.items(layout.glyphs.len(), 768, 16)
            || !self.budget.items(layout.baselines.len(), 640, 12)
            || !self.budget.items(layout.outlines.len(), 16, 2)
        {
            return None;
        }
        let mut indices = Vec::with_capacity(layout.outlines.len());
        for bytes in &layout.outlines {
            let key = (bytes.as_ptr() as usize, bytes.len());
            if !self.budget.reserve(0, self.glyph_blobs.len().saturating_add(1).ilog2() as usize + 1) {
                return None;
            }
            if let Some((_, index)) = self.glyph_blobs.get(&key) {
                indices.push(*index);
                continue;
            }
            // A blob lookup compares byte slices, as the encoder compares strings. Charge
            // 64-byte blocks for the search, including the logarithmic growth of the table.
            let comparisons = 3 * (self.blobs.len().saturating_add(1).ilog2() as usize + 1);
            if !self.budget.items(bytes.len().div_ceil(64).saturating_add(1), 0, comparisons) {
                return None;
            }
            let index = if let Some(&index) = self.blobs.get(bytes.as_ref()) {
                index
            } else {
                if !self.budget.reserve(bytes.len(), 0) {
                    return None;
                }
                self.blob(bytes.to_vec())?
            };
            if !self.budget.keep(128) {
                return None;
            }
            self.glyph_blobs.insert(key, (bytes.clone(), index));
            indices.push(index);
        }
        let mut glyphs = Vec::with_capacity(layout.glyphs.len());
        for glyph in &layout.glyphs {
            let mut row = Fields::new();
            if let Some(index) = glyph.outline {
                row.push(("commandsBlob", u(*indices.get(index)?)));
            }
            row.extend(glyph.position.map(|p| ("position", point(p))));
            if let Some(id) = glyph.style_id {
                // The export builds a fresh run style table. The source's ID alone cannot name
                // it. Preserve the style of the corresponding UTF-16 unit and the optionality
                // of firstCharacter (generated glyphs such as ellipses need not have one).
                let id = if id == 0 { u(0) } else { style_ids.get(glyph.style_character? as usize)?.clone() };
                row.push(("styleID", id));
            }
            row.extend(glyph.first_character.map(|n| ("firstCharacter", u(n))));
            for (name, value) in [("fontSize", glyph.font_size), ("advance", glyph.advance), ("rotation", glyph.rotation)] {
                row.extend(value.map(|v| (name, Kv::Float(v))));
            }
            if !glyph.emoji_code_points.is_empty() {
                if !self.budget.items(glyph.emoji_code_points.len(), size_of::<Kv>(), 1) {
                    return None;
                }
                row.push(("emojiCodePoints", arr(glyph.emoji_code_points.iter().copied().map(u).collect())));
            }
            if let Some(set) = glyph.emoji_image_set.as_deref() {
                row.push((
                    "emojiImageSet",
                    e(match set {
                        "APPLE" => "APPLE",
                        "NOTO" => "NOTO",
                        _ => return None,
                    }),
                ));
            }
            glyphs.push(o(row));
        }
        let mut fields = Fields::new();
        fields.extend(layout.layout_size.map(|p| ("layoutSize", point(p))));
        fields.push(("baselines", arr(layout.baselines.iter().map(baseline).collect())));
        fields.push(("glyphs", arr(glyphs)));
        self.native_companions(layout, style_ids, &mut fields)?;
        Some(o(fields))
    }

    fn native_companions(&mut self, layout: &NativeTextLayout, style_ids: &[Kv<'static>], fields: &mut Fields) -> Option<()> {
        fields.extend(layout.truncation_start_index.map(|i| ("truncationStartIndex", Kv::Int(i64::from(i)))));
        fields.extend(layout.truncated_height.map(|h| ("truncatedHeight", Kv::Float(h))));
        if let Some(offsets) = &layout.logical_offsets {
            if !self.budget.items(offsets.len(), size_of::<Kv>(), 1) {
                return None;
            }
            fields.push(("logicalIndexToCharacterOffsetMap", arr(offsets.iter().copied().map(Kv::Float).collect())));
        }
        if let Some(lines) = &layout.derived_lines {
            if !self.budget.items(lines.len(), 96, 3) {
                return None;
            }
            let mut rows = Vec::with_capacity(lines.len());
            for direction in lines {
                let dir = match direction.as_str() {
                    "LTR" => "LTR",
                    "RTL" => "RTL",
                    _ => return None,
                };
                rows.push(o(vec![("directionality", e(dir))]));
            }
            fields.push(("derivedLines", arr(rows)));
        }
        if let Some(decoration) = &layout.decorations {
            if !self.budget.items(decoration.len(), 192, 3) {
                return None;
            }
            let mut rows = Vec::with_capacity(decoration.len());
            for d in decoration {
                if !self.budget.items(d.rects.len(), 256, 8) {
                    return None;
                }
                let rects = d
                    .rects
                    .iter()
                    .map(|&[x, y, w, h]| o(vec![("x", Kv::Float(x)), ("y", Kv::Float(y)), ("w", Kv::Float(w)), ("h", Kv::Float(h))]))
                    .collect();
                let mut row = vec![("rects", arr(rects))];
                if let Some(id) = d.style_id {
                    row.push(("styleID", if id == 0 { u(0) } else { style_ids.get(d.style_character? as usize)?.clone() }));
                }
                rows.push(o(row));
            }
            fields.push(("decorations", arr(rows)));
        }
        if !layout.font_meta_data.is_empty() {
            if !self.budget.items(layout.font_meta_data.len(), 512, 8) {
                return None;
            }
            let mut fonts = Vec::with_capacity(layout.font_meta_data.len());
            for font in &layout.font_meta_data {
                let mut row = Fields::new();
                if let Some(key) = &font.key {
                    let size = key.family.len().saturating_add(key.style.len()).saturating_add(key.postscript.len());
                    if !self.budget.reserve(size, size.div_ceil(64)) {
                        return None;
                    }
                    row.push(("key", crate::fig::export::common::native_face(key)));
                }
                row.extend(font.font_line_height.map(|v| ("fontLineHeight", Kv::Float(v))));
                if let Some(digest) = &font.font_digest {
                    if !self.budget.reserve(digest.len(), digest.len().div_ceil(64)) {
                        return None;
                    }
                    row.push(("fontDigest", Kv::Bytes(digest.clone())));
                }
                if let Some(style) = font.font_style.as_deref() {
                    row.push((
                        "fontStyle",
                        e(match style {
                            "NORMAL" => "NORMAL",
                            "ITALIC" => "ITALIC",
                            _ => return None,
                        }),
                    ));
                }
                row.extend(font.font_weight.map(|v| ("fontWeight", Kv::Int(i64::from(v)))));
                fonts.push(o(row));
            }
            fields.push(("fontMetaData", arr(fonts)));
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fig::export::budget::{Budget, Limits};
    use figcraft_doc::{Document, NativeTextGlyph};

    #[test]
    fn native_glyph_export_charges_placements_before_registering_outline_blobs() {
        let doc = Document::new("Budget control");
        let mut writer = Exporter::new(&doc);
        writer.budget = Budget::new(Limits { work: 16, ..Default::default() });
        let layout = NativeTextLayout {
            outlines: vec![vec![0].into()],
            glyphs: vec![NativeTextGlyph { outline: Some(0), ..Default::default() }; 2],
            ..Default::default()
        };
        assert!(writer.native_text_layout(&layout, &[]).is_none());
        assert!(writer.budget.check().is_err());
        assert!(writer.blobs.is_empty());
    }
}
