//! Query-dependent ranking within an already retrieved candidate set.
use crate::normalize::{Options, normalize};
use std::collections::{BTreeMap, BTreeSet};

pub fn tokens(text: &str) -> Vec<String> {
    let normalized = normalize(
        text,
        Options {
            nfkc: true,
            kana: true,
        },
    );
    let cjk = |c: char| {
        matches!(c as u32,
        0x3040..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x3134f)
    };
    let mut result = Vec::new();
    let mut word = Vec::new();
    let flush = |word: &mut Vec<char>, result: &mut Vec<String>| {
        if word.is_empty() {
            return;
        }
        let whole: String = word.iter().collect::<String>().to_lowercase();
        let mut forms = BTreeSet::new();
        if word.iter().any(|c| c.is_alphanumeric()) {
            forms.insert(whole);
        }
        if cjk(word[0]) {
            for pair in word.windows(2) {
                forms.insert(pair.iter().collect());
            }
        } else {
            let mut start = 0;
            for i in 0..=word.len() {
                let boundary = i == word.len()
                    || word[i] == '_'
                    || (i > start
                        && (word[i - 1] == '_'
                            || word[i].is_uppercase()
                                && (word[i - 1].is_lowercase()
                                    || word.get(i + 1).is_some_and(|c| c.is_lowercase()))
                            || word[i].is_numeric() != word[i - 1].is_numeric()));
                if boundary {
                    if start < i {
                        forms.insert(word[start..i].iter().collect::<String>().to_lowercase());
                    }
                    start = if i < word.len() && word[i] == '_' {
                        i + 1
                    } else {
                        i
                    };
                }
            }
        }
        result.extend(forms);
        word.clear();
    };
    for c in normalized.chars() {
        if !c.is_alphanumeric() && c != '_' && c != '\u{3099}' && c != '\u{309a}' {
            flush(&mut word, &mut result);
        } else {
            if word.last().is_some_and(|last| cjk(*last) != cjk(c)) {
                flush(&mut word, &mut result);
            }
            word.push(c);
        }
    }
    flush(&mut word, &mut result);
    result
}

pub fn bm25_scores(documents: &[Vec<String>], query: &[String]) -> Vec<f64> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let terms: BTreeSet<&str> = query.iter().map(String::as_str).collect();
    let total: usize = documents.iter().map(Vec::len).sum();
    if total == 0 || terms.is_empty() {
        return vec![0.0; documents.len()];
    }
    let average = total as f64 / documents.len() as f64;
    let counts: Vec<BTreeMap<&str, usize>> = documents
        .iter()
        .map(|document| {
            let mut counts = BTreeMap::new();
            for token in document {
                if terms.contains(token.as_str()) {
                    *counts.entry(token.as_str()).or_default() += 1;
                }
            }
            counts
        })
        .collect();
    let idfs: BTreeMap<&str, f64> = terms
        .into_iter()
        .map(|term| {
            let df = counts
                .iter()
                .filter(|counts| counts.contains_key(term))
                .count() as f64;
            (
                term,
                (1.0 + (documents.len() as f64 - df + 0.5) / (df + 0.5)).ln(),
            )
        })
        .collect();
    documents
        .iter()
        .zip(counts)
        .map(|(document, counts)| {
            let norm = K1 * (1.0 - B + B * document.len() as f64 / average);
            counts
                .into_iter()
                .map(|(term, count)| {
                    let tf = count as f64;
                    idfs[term] * tf / (tf + norm)
                })
                .sum()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_whole_forms_and_split_acronyms_camel_and_snake() {
        let ts = tokens("HTTPServer cancelAudio cancel_audio");
        for term in [
            "httpserver",
            "http",
            "server",
            "cancelaudio",
            "cancel_audio",
            "cancel",
            "audio",
        ] {
            assert!(ts.contains(&term.to_string()), "missing {term}: {ts:?}");
        }
        assert_eq!(tokens("cancel"), vec!["cancel"]);
    }

    #[test]
    fn kana_width_and_composition_fold_without_changing_source() {
        assert_eq!(tokens("ＡＰＩ ｶﾞｯｺｳ"), tokens("api がっこう"));
        assert_eq!(tokens("ガッコウ"), tokens("か\u{3099}っこう"));
        assert!(tokens("音声取消処理").contains(&"取消".to_owned()));
    }

    #[test]
    fn lucene_scores_match_hand_computed_reference_and_query_duplicates_do_not_boost() {
        let docs = [vec!["a", "a", "b"], vec!["a"], vec!["b", "c"]]
            .map(|d| d.into_iter().map(str::to_owned).collect::<Vec<_>>());
        let scores = bm25_scores(&docs, &["a".into()]);
        // N=3, df=2, avgdl=2, k1=1.2, b=.75. Same TF scaling as BM25S Lucene.
        let idf = (1.0_f64 + 1.5 / 2.5).ln();
        assert!((scores[0] - idf * 2.0 / 3.65).abs() < 1e-12);
        assert!((scores[1] - idf / 1.75).abs() < 1e-12);
        assert_eq!(scores[2], 0.0);
        assert_eq!(scores, bm25_scores(&docs, &["a".into(), "a".into()]));
        assert_eq!(bm25_scores(&docs, &["unknown".into()]), vec![0.0; 3]);
        assert_eq!(bm25_scores(&docs, &[]), vec![0.0; 3]);
        assert!(bm25_scores(&[], &["a".into()]).is_empty());
    }

    #[test]
    fn rare_terms_and_shorter_documents_win_over_boilerplate() {
        let docs = [
            tokens("cancel common common common common"),
            tokens("cancel rare"),
            tokens("cancel common"),
        ];
        let scores = bm25_scores(&docs, &tokens("cancel rare"));
        assert!(scores[1] > scores[2] && scores[2] > scores[0]);
    }
}
