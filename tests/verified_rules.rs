use jrg::{
    query_expansion,
    selection::{QueryRuleMode, SelectionRequest, select, select_with_query_rules},
};
use serde_json::json;

#[test]
fn standard_selection_does_not_apply_unapproved_numeric_rules() {
    let request: SelectionRequest = serde_json::from_value(json!({
        "query":"ゼロ負数時の扱い", "query_expansions":["zero or negative"], "max_tokens":700,
        "documents":[{"id":"x","url":"https://example.test/new","title":"Limits",
            "blocks":[{"id":"p","kind":"paragraph","text":"Non-positive durations delete immediately."}]}]
    })).unwrap();
    let standard = select(&request).unwrap();
    let off = select_with_query_rules(&request, QueryRuleMode::Off).unwrap();
    assert_eq!(standard, off);
    assert_eq!(standard.status, "no_lexical_overlap");
}

#[test]
fn rule_subsets_do_not_enable_other_rules_and_reject_unknown_ids() {
    let query = "S3 regular buckets with zero or negative duration";
    let filtered = query_expansion::expand_only(query, &["nonpositive".to_string()]).unwrap();
    assert_eq!(filtered.applied_rule_ids, ["nonpositive"]);
    assert!(!filtered.query.contains("general purpose"));
    assert_eq!(
        query_expansion::expand_only(query, &[]).unwrap().query,
        query
    );
    assert!(query_expansion::expand_only(query, &["unreviewed".to_string()]).is_err());
}

#[test]
fn approval_is_bound_to_question_english_budget_and_complete_document() {
    let questions: serde_json::Value = serde_json::from_str(include_str!(
        "../experiments/2026-10-22-web-selection-heldout/questions.json"
    ))
    .unwrap();
    let english: serde_json::Value = serde_json::from_str(include_str!(
        "../experiments/2026-10-22-web-selection-heldout/generated_english.json"
    ))
    .unwrap();
    for (key, rule, text) in [
        (
            "redis",
            "nonpositive",
            include_str!("../experiments/2026-10-22-web-selection-heldout/redis-documents.json"),
        ),
        (
            "s3",
            "s3-general-purpose",
            include_str!("../experiments/2026-10-22-web-selection-heldout/s3-documents.json"),
        ),
    ] {
        let question = questions["questions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["id"] == key)
            .unwrap();
        let en = english
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["id"] == key)
            .unwrap();
        let mut value: serde_json::Value = serde_json::from_str(text).unwrap();
        value["query"] = question["question_ja"].clone();
        value["query_expansions"] = json!([en["query_en"]]);
        value["max_tokens"] = json!(1024);
        let request: SelectionRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            jrg::verified_query_rules::approved_rule_ids(&request).unwrap(),
            [rule]
        );
        for field in ["query", "english", "budget", "source"] {
            let mut changed = request.clone();
            match field {
                "query" => changed.query.push(' '),
                "english" => changed.query_expansions[0].push(' '),
                "budget" => changed.max_tokens += 1,
                _ => changed.documents[0].blocks[0].text.push(' '),
            }
            assert!(
                jrg::verified_query_rules::approved_rule_ids(&changed)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
