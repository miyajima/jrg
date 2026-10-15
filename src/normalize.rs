use std::{borrow::Cow, ops::Range};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub struct Options {
    pub nfkc: bool,
    pub kana: bool,
}

fn push_kana(c: char, out: &mut String) {
    match c {
        '\u{30a1}'..='\u{30f6}' | '\u{30fd}'..='\u{30fe}' => {
            out.push(char::from_u32(c as u32 - 0x60).unwrap());
        }
        // ヷ..ヺ have no precomposed hiragana counterparts.
        '\u{30f7}'..='\u{30fa}' => {
            out.push(['わ', 'ゐ', 'ゑ', 'を'][(c as u32 - 0x30f7) as usize]);
            out.push('\u{3099}');
        }
        _ => out.push(c),
    }
}

pub fn normalize(text: &str, options: Options) -> Cow<'_, str> {
    if text.is_ascii() || (!options.nfkc && !options.kana) {
        return Cow::Borrowed(text);
    }
    let normalized: String = if options.nfkc {
        text.nfkc().collect()
    } else {
        text.to_owned()
    };
    if options.kana {
        let mut folded = String::with_capacity(normalized.len());
        for c in normalized.chars() {
            push_kana(c, &mut folded);
        }
        Cow::Owned(folded)
    } else {
        Cow::Owned(normalized)
    }
}

/// Map normalized UTF-8 byte ranges to complete graphemes in the original line.
/// Compatibility expansions (e.g. ㍿ → 株式会社) point back to their source glyph.
pub fn original_ranges(
    source: &str,
    matches: impl Iterator<Item = Range<usize>>,
    options: Options,
) -> Vec<Range<usize>> {
    if source.is_ascii() || (!options.nfkc && !options.kana) {
        return merge_ranges(matches.collect());
    }
    let mut map = Vec::new();
    let mut offset = 0;
    for (start, grapheme) in source.grapheme_indices(true) {
        let len = normalize(grapheme, options).len();
        map.push((offset..offset + len, start..start + grapheme.len()));
        offset += len;
    }
    let ranges = matches.map(|m| {
        let first = map.partition_point(|(normalized, _)| normalized.end <= m.start);
        if m.is_empty() {
            let start = map.get(first).map_or(source.len(), |(_, r)| r.start);
            return start..start;
        }
        let last = map.partition_point(|(normalized, _)| normalized.start < m.end);
        let start = map.get(first).map_or(source.len(), |(_, r)| r.start);
        let end = map
            .get(last.saturating_sub(1))
            .map_or(start, |(_, r)| r.end);
        start..end
    });
    merge_ranges(ranges.collect())
}

fn merge_ranges(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start < last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    const JP: Options = Options {
        nfkc: true,
        kana: true,
    };

    #[test]
    fn width_kana_and_combining_marks() {
        for source in ["がっこう", "ガッコウ", "ｶﾞｯｺｳ", "か\u{3099}っこう"] {
            assert_eq!(normalize(source, JP), "がっこう");
        }
        assert_eq!(normalize("ＡＢＣ１２３ ㍿", JP), "ABC123 株式会社");
        assert_eq!(normalize("ヽヾヴ", JP), "ゝゞゔ");
        assert_eq!(
            normalize("ヷヸヹヺ", JP),
            "わ\u{3099}ゐ\u{3099}ゑ\u{3099}を\u{3099}"
        );
    }

    #[test]
    fn source_spans_survive_composition_and_expansion() {
        let source = "前ｶﾞ㍿後";
        let text = normalize(source, JP);
        let start = text.find('が').unwrap();
        let ranges = original_ranges(source, std::iter::once(start..start + 3), JP);
        assert_eq!(&source[ranges[0].clone()], "ｶﾞ");
        let start = text.find('社').unwrap();
        let ranges = original_ranges(source, std::iter::once(start..start + 3), JP);
        assert_eq!(&source[ranges[0].clone()], "㍿");
    }

    #[test]
    fn per_grapheme_mapping_agrees_with_whole_string() {
        for source in [
            "ｶﾞｯｺｳ",
            "か\u{3099}",
            "ヷ",
            "㍿",
            "é",
            "e\u{301}",
            "가",
            "👩‍💻",
        ] {
            let joined: String = source.graphemes(true).map(|g| normalize(g, JP)).collect();
            assert_eq!(joined, normalize(source, JP));
        }
    }
}
