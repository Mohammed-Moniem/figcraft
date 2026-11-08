//! Original synthetic #1970 layout regressions, using existing installed/embedded fonts.
use super::*;
use figcraft_doc::{StyleRun, TextAlignH, TextAutoResize, TextData, TextDecoration, WordBreak};

fn text(s: &str) -> Option<TextData> {
    let families = ["BIZ UDPGothic", "Noto Sans CJK SC", "NanumGothic", "PingFang SC", "Hiragino Sans", "Noto Sans SC"];
    let probe = if s.chars().next().is_some_and(|c| ('가'..='힣').contains(&c)) { '한' } else { '文' };
    let face = face_with_char(probe, &families).or_else(|| {
        scan_system_fonts();
        face_with_char(probe, &families)
    });
    let Some(face) = face else {
        eprintln!("CJK-JUSTIFY-SKIP: no CJK face in this optional-font build");
        return None;
    };
    eprintln!("CJK-JUSTIFY-COVERAGE: {} {}", face.family, face.style);
    let mut t = TextData::new(s);
    t.style.font_family = face.family.clone();
    t.style.font_size = 24.0;
    t.auto_resize = TextAutoResize::Height;
    Some(t)
}

fn pair(t: &TextData, width: f64) -> (TextLayout, TextLayout) {
    let mut t = t.clone();
    t.align_h = TextAlignH::Left;
    let left = layout(&t, Some(width), None);
    t.align_h = TextAlignH::Justified;
    let justified = layout(&t, Some(width), None);
    assert_eq!(
        left.lines.iter().map(|l| (l.char_start, l.char_end)).collect::<Vec<_>>(),
        justified.lines.iter().map(|l| (l.char_start, l.char_end)).collect::<Vec<_>>()
    );
    assert_eq!(left.glyphs.len(), justified.glyphs.len());
    for (a, b) in left.glyphs.iter().zip(&justified.glyphs) {
        assert_eq!(
            (a.gid, a.char_start, a.char_end, a.advance, a.y),
            (b.gid, b.char_start, b.char_end, b.advance, b.y),
            "justification moves shaped glyphs without reshaping"
        );
    }
    (left, justified)
}

fn right(l: &TextLayout, line: usize, source: &str) -> f64 {
    let line = &l.lines[line];
    let chars: Vec<char> = source.chars().collect();
    let mut end = line.char_end;
    while end > line.char_start && chars.get(end - 1).is_some_and(|c| crate::breaks::is_space(*c) || matches!(c, '\n' | '\u{2028}')) {
        end -= 1;
    }
    l.glyphs[line.glyph_start..line.glyph_end].iter().filter(|g| g.char_start < end).map(|g| g.x + g.advance).fold(f64::NEG_INFINITY, f64::max)
}

fn shift(left: &TextLayout, justified: &TextLayout, at: usize) -> f64 {
    let k = left.glyphs.iter().position(|g| g.char_start <= at && at < g.char_end).unwrap();
    justified.glyphs[k].x - left.glyphs[k].x
}

fn aligned_first(left: &TextLayout, justified: &TextLayout, width: f64, why: &str) {
    assert!(left.lines.len() >= 2 && !left.lines[0].para_end && !left.lines[0].break_after, "fixture must contain a wrapped nonfinal line: {why}");
    if width - right(left, 0, why) <= 1.0 {
        let line = &left.lines[0];
        eprintln!(
            "CJK-SPARE-DIAGNOSTIC: {why} line={line:?} glyphs={:?}",
            left.glyphs[line.glyph_start..line.glyph_end].iter().map(|g| (g.char_start, g.char_end, g.x, g.advance)).collect::<Vec<_>>()
        );
    }
    assert!(width - right(left, 0, why) > 1.0, "fixture needs spare width: {why}");
    assert!((right(justified, 0, why) - width).abs() < 1e-6, "{why}: right edge {} must reach {width}", right(justified, 0, why));
    let last = left.lines.len() - 1;
    assert!(left.lines[last].para_end);
    assert_eq!(right(left, last, why), right(justified, last, why), "paragraph final line stays ragged: {why}");
}

#[test]
fn cjk_wrapped_nonfinal_lines_reach_the_box_edge() {
    for (s, width) in
        [("中文排版字距調整測試中文段落排列", 181.0), ("日本語文章組版文字間隔調整確認", 181.0), ("한글문자배치정렬검사한글문장검사", 193.0)]
    {
        let Some(t) = text(s) else { return };
        let (left, justified) = pair(&t, width);
        assert!(left.glyphs.iter().all(|g| g.gid != 0 && g.x.is_finite() && g.advance.is_finite()), "real glyph coverage required, never tofu: {s}");
        aligned_first(&left, &justified, width, s);
        assert!(shift(&left, &justified, 1) > 0.0, "a later character actually moves");
    }
}

#[test]
fn cjk_spacing_preserves_punctuation_and_grapheme_clusters() {
    for (s, same_shift) in
        [("中文（排版）調整測試中文段落", (2, 3)), ("中文排版。調整測試中文段落", (3, 4)), ("中文\u{20DD}排版調整測試中文段落", (1, 2))]
    {
        let Some(t) = text(s) else { return };
        let (left, justified) = pair(&t, 181.0);
        aligned_first(&left, &justified, 181.0, s);
        assert!(
            (shift(&left, &justified, same_shift.0) - shift(&left, &justified, same_shift.1)).abs() < 1e-6,
            "no new gap after opening/before closing/inside grapheme: {s}"
        );
        if s.contains('\u{20DD}') {
            let delta = shift(&left, &justified, 1);
            for (a, b) in left.glyphs.iter().zip(&justified.glyphs).filter(|(g, _)| (1..3).contains(&g.char_start)) {
                assert!((b.x - a.x - delta).abs() < 1e-6, "every mark/base glyph in the grapheme shares its translation");
            }
        }
    }
}

#[test]
fn cjk_mixed_bidi_keeps_words_whole_and_fills_the_line_once() {
    for (case, (s, word)) in
        [("中文אבגד測試中文排版字距調整中文", 2..6), ("中文\u{202B}אב中文גד\u{202C}測試中文排版字距調整", 3..5)].into_iter().enumerate()
    {
        let Some(mut t) = text(s) else { return };
        let families = ["Noto Sans Hebrew", "Arial Hebrew", "Arial", "DejaVu Sans"];
        let face = face_with_char('א', &families).or_else(|| {
            scan_system_fonts();
            face_with_char('א', &families)
        });
        let Some(face) = face else {
            eprintln!("CJK-JUSTIFY-SKIP: no Hebrew face for the mixed-script control");
            return;
        };
        // Explicit rich styles test bidi spacing independently of fallback-family policy.
        for (at, _) in s.chars().enumerate().filter(|(_, c)| ('א'..='ת').contains(c)) {
            let mut style = t.style.clone();
            style.font_family = face.family.clone();
            t.runs.push(StyleRun { start: at, end: at + 1, style });
        }
        let (left, justified) = pair(&t, 253.0);
        eprintln!(
            "CJK-BIDI-COVERAGE: {:?}",
            left.glyphs
                .iter()
                .filter(|g| word.contains(&g.char_start))
                .map(|g| (g.char_start, g.gid, g.face.as_ref().map(|f| f.family.as_str())))
                .collect::<Vec<_>>()
        );
        assert!(left.glyphs.iter().filter(|g| word.contains(&g.char_start)).all(|g| g.gid != 0), "RTL word needs real font coverage");
        aligned_first(&left, &justified, 253.0, s);
        let delta = shift(&left, &justified, word.start);
        for at in word {
            assert!((shift(&left, &justified, at) - delta).abs() < 1e-6, "RTL word receives one translation, never internal CJK gaps: {s}");
        }
        // Explicit counts describe these two strings in visual order, independently of the
        // production helper. A whole RTL word has one count; the nested LTR pair has its own.
        let count = |at| match (case, at) {
            (_, 0) => Some(0),
            (_, 1) => Some(1),
            (0, 2..=5) => Some(2),
            (0, at) if at >= 6 => Some(at - 3),
            (1, 3 | 4) => Some(5),
            (1, 5) => Some(3),
            (1, 6) => Some(4),
            (1, 7 | 8) => Some(2),
            (1, at) if at >= 10 => Some(at - 4),
            _ => None,
        };
        let first = &left.lines[0];
        assert!(first.char_end > 10, "fixture contains both word boundaries and the nested pair");
        let glyphs = &left.glyphs[first.glyph_start..first.glyph_end];
        let gaps = glyphs.iter().filter_map(|g| count(g.char_start)).max().unwrap();
        let extra = (253.0 - right(&left, 0, s)) / gaps as f64;
        for (a, b) in glyphs.iter().zip(&justified.glyphs[first.glyph_start..first.glyph_end]) {
            let Some(n) = count(a.char_start) else { continue };
            assert!((b.x - a.x - n as f64 * extra).abs() < 1e-6, "visual boundary for source char{}: {s}", a.char_start);
            let (caret, line) = justified.caret(b.char_start);
            let expected = if b.rtl { b.x + b.advance } else { b.x };
            assert_eq!(line, 0);
            assert!((caret - expected).abs() < 1e-6, "caret uses adjusted visual position");
            if !b.rtl {
                assert_eq!(
                    justified.hit(b.x + b.advance * 0.25, first.top + first.height * 0.5),
                    b.char_start,
                    "CJK hit uses adjusted glyph position"
                );
            }
        }
    }
}

#[test]
fn cjk_final_hard_break_and_latin_only_controls_keep_existing_spacing() {
    for s in ["中文排版", "中文排版\n字距調整", "中文排版\u{2028}字距調整"] {
        let Some(t) = text(s) else { return };
        let (left, justified) = pair(&t, 240.0);
        assert_eq!(
            left.glyphs.iter().map(|g| g.x).collect::<Vec<_>>(),
            justified.glyphs.iter().map(|g| g.x).collect::<Vec<_>>(),
            "short final or explicit break: {s}"
        );
    }
    let Some(t) = text("alpha beta gamma delta epsilon zeta eta theta") else { return };
    let (left, justified) = pair(&t, 181.0);
    aligned_first(&left, &justified, 181.0, &t.characters);
    for at in 1..5 {
        assert_eq!(shift(&left, &justified, at), 0.0, "first Latin word is never letter-spaced");
    }
}

#[test]
fn cjk_directional_formatting_does_not_duplicate_character_spacing() {
    let plain = "中文排版字距調整測試中文段落";
    let formatted = "中文\u{202A}排版字\u{202C}距調整測試中文段落";
    let Some(t) = text(plain) else { return };
    let (_, plain_layout) = pair(&t, 181.0);
    let Some(t) = text(formatted) else { return };
    let (_, formatted_layout) = pair(&t, 181.0);
    let visible = |source: &str, l: &TextLayout| {
        let chars: Vec<char> = source.chars().collect();
        l.glyphs
            .iter()
            .filter(|g| !matches!(chars.get(g.char_start), Some('\u{202A}' | '\u{202C}')))
            .map(|g| (g.gid, g.x, g.y, g.advance))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        visible(plain, &plain_layout),
        visible(formatted, &formatted_layout),
        "direction-only controls do not add typographic units or spacing opportunities"
    );
}

#[test]
fn cjk_real_spaces_expand_once_at_mixed_script_boundaries() {
    let Some(t) = text("中文 alpha 中文 beta 中文 gamma 中文") else { return };
    let width = layout(&t, None, None).caret(11).0 + 4.0;
    let (left, justified) = pair(&t, width);
    aligned_first(&left, &justified, width, &t.characters);
    assert!((11..=12).contains(&left.lines[0].char_end), "first line contains the original two CJK pairs and Latin word");
    let extra = (width - right(&left, 0, &t.characters)) / 4.0;
    for at in 0..11 {
        let n = match at {
            0 => 0,
            1 | 2 => 1,
            3..=8 => 2,
            9 => 3,
            _ => 4,
        };
        assert!((shift(&left, &justified, at) - n as f64 * extra).abs() < 1e-6, "one gap per real space, never another at its CJK edge: char{at}");
    }
}

#[test]
fn cjk_keep_all_keeps_korean_words_whole_while_spaces_justify() {
    let Some(mut t) = text("한글문자 배치정렬 검사문장 한글문자") else { return };
    t.word_break = WordBreak::KeepAll;
    let width = layout(&t, None, None).caret(9).0 + 4.0;
    let (left, justified) = pair(&t, width);
    aligned_first(&left, &justified, width, &t.characters);
    assert!((9..=10).contains(&left.lines[0].char_end), "two intact Korean words on the first line");
    let extra = width - right(&left, 0, &t.characters);
    for at in 0..4 {
        assert_eq!(shift(&left, &justified, at), 0.0);
    }
    for at in 5..9 {
        assert!((shift(&left, &justified, at) - extra).abs() < 1e-6, "KeepAll word translates as a whole");
    }
}

#[test]
fn cjk_underlines_and_first_line_indent_follow_the_adjusted_glyphs() {
    let Some(mut t) = text("中文排版字距調整測試中文段落排列") else { return };
    t.style.decoration = TextDecoration::Underline;
    let (left, justified) = pair(&t, 181.0);
    aligned_first(&left, &justified, 181.0, &t.characters);
    assert_eq!(justified.styles, left.styles);
    assert_eq!(justified.decorations.len(), left.decorations.len());
    let underline = &justified.decorations[0];
    assert_eq!(underline.rect.x0, 0.0);
    assert!((underline.rect.x1 - 181.0).abs() < 1e-6, "underline reaches the justified advance edge");
    // Existing paragraph-indent eligibility is Left-only. Justify keeps the supplied value
    // without applying it; a test must not assume the two alignment modes wrap identically.
    t.paragraph_indent = 13.0;
    t.align_h = TextAlignH::Justified;
    let kept = layout(&t, Some(181.0), None);
    assert_eq!(kept.glyphs.iter().map(|g| (g.x, g.y)).collect::<Vec<_>>(), justified.glyphs.iter().map(|g| (g.x, g.y)).collect::<Vec<_>>());
    t.align_h = TextAlignH::Left;
    assert_eq!(layout(&t, Some(181.0), None).caret(0).0, 13.0);
}
