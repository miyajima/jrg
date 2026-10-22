use jrg::{
    adapters,
    selection::{SelectionRequest, select},
};
use serde_json::{Value, json};

fn request(value: Value) -> SelectionRequest {
    serde_json::from_value(value).unwrap()
}

#[test]
fn english_query_expansion_preserves_primary_intent_and_original_evidence() {
    let mut value = json!({"query":"解約の違約金と例外", "max_tokens":700,"documents":[
        {"id":"terms","url":"https://example.test/terms","title":"Terms","blocks":[
            {"id":"h","kind":"heading","text":"## Termination"},
            {"id":"p","kind":"paragraph","text":"Cancellation fees are waived.","parent_id":"h","related_ids":["n"]},
            {"id":"n","kind":"note","text":"Except for business contracts."}]},
        {"id":"weather","url":"https://example.test/weather","title":"Weather","blocks":[
            {"id":"w","kind":"paragraph","text":"Tomorrow will be sunny."}]}]});
    let legacy = select(&request(value.clone())).unwrap();
    assert_eq!(legacy.status, "no_lexical_overlap");
    value["query_expansions"] = json!(["cancellation fees"]);
    let expanded = select(&request(value)).unwrap();
    assert_eq!(expanded.documents.len(), 1);
    assert_eq!(expanded.documents[0].document.id, "terms");
    assert_eq!(
        expanded.documents[0]
            .document
            .blocks
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        ["h", "p", "n"]
    );
    assert_eq!(
        expanded.documents[0].document.blocks[1].text,
        "Cancellation fees are waived."
    );
    assert_eq!(
        expanded.documents[0].document.blocks[2].text,
        "Except for business contracts."
    );
}

#[test]
fn expansions_are_optional_bounded_and_cannot_replace_an_empty_primary_query() {
    let base = json!({"query":"fee","max_tokens":500,"documents":[]});
    let legacy = select(&request(base.clone())).unwrap();
    let mut empty = base.clone();
    empty["query_expansions"] = json!([]);
    assert_eq!(legacy, select(&request(empty)).unwrap());
    for expansion in [
        json!(["a", "b", "c", "d", "e"]),
        json!([""]),
        json!(["  \n "]),
        json!(["あ".repeat(1366)]),
    ] {
        let mut invalid = base.clone();
        invalid["query_expansions"] = expansion;
        assert!(select(&request(invalid)).is_err());
    }
    let mut missing_primary = base.clone();
    missing_primary["query"] = json!(" ");
    missing_primary["query_expansions"] = json!(["fee"]);
    assert!(select(&request(missing_primary)).is_err());
    let mut limit = base.clone();
    limit["query_expansions"] = json!(["x".repeat(4092)]);
    assert!(select(&request(limit.clone())).is_ok());
    limit["query_expansions"] = json!(["x".repeat(4093)]);
    assert!(select(&request(limit)).is_err());
    for invalid_type in [json!(null), json!("fees"), json!([null]), json!([7])] {
        let mut value = base.clone();
        value["query_expansions"] = invalid_type;
        assert!(serde_json::from_value::<SelectionRequest>(value).is_err());
    }
}

#[test]
fn expansions_add_matching_terms_without_discarding_the_primary_query() {
    let request = request(
        json!({"query":"解約","query_expansions":["penalty"],"max_tokens":700,"documents":[
        {"id":"ja","url":"https://example.test/ja","title":"規約","blocks":[{"id":"p","kind":"paragraph","text":"解約の条件を確認してください。"}]},
        {"id":"en","url":"https://example.test/en","title":"Terms","blocks":[{"id":"p","kind":"paragraph","text":"No penalty applies."}]}]}),
    );
    let selected = select(&request).unwrap();
    let ids = selected
        .documents
        .iter()
        .map(|d| d.document.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids, ["en", "ja"].into());
}

fn fixture(budget: usize) -> SelectionRequest {
    request(
        json!({"query":"解約 違約金", "max_tokens":budget,"documents":[
      {"id":"terms","url":"https://example.test/terms","title":"利用規約","blocks":[
        {"id":"h","kind":"heading","text":"## 契約の終了"},
        {"id":"p","kind":"paragraph","parent_id":"h","related_ids":["n"],"text":"解約時の違約金は1万円です。[^1]"},
        {"id":"n","kind":"note","text":"[^1]: 初月は請求しません。"},
        {"id":"other","kind":"paragraph","text":"UNRELATED_WEATHER_SENTINEL 晴れのち雨。"}]},
      {"id":"weather","url":"https://example.test/weather","title":"天気","blocks":[
        {"id":"forecast","kind":"paragraph","text":"UNRELATED_FORECAST_SENTINEL 明日は晴れ。"}]}]}),
    )
}

#[test]
fn selection_preserves_original_sources_and_required_context() {
    let out = select(&fixture(1000)).unwrap();
    assert_eq!(out.documents.len(), 1);
    let doc = &out.documents[0];
    assert_eq!(doc.document.url, "https://example.test/terms");
    assert_eq!(
        doc.document
            .blocks
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        ["h", "p", "n"]
    );
    assert_eq!(
        doc.document.blocks[1].text,
        "解約時の違約金は1万円です。[^1]"
    );
    assert_eq!(doc.source_hash.len(), 64);
    assert!(!serde_json::to_string(&out).unwrap().contains("UNRELATED"));
    assert_eq!(out.omitted_count, 2);
}

#[test]
fn serialized_metadata_is_inside_the_budget_and_results_are_deterministic() {
    for limit in [128, 220, 400, 1000] {
        let req = fixture(limit);
        let out = select(&req).unwrap();
        let serialized = serde_json::to_string(&out).unwrap();
        assert_eq!(
            out.tokens,
            tiktoken_rs::o200k_base_singleton()
                .encode_ordinary(&serialized)
                .len()
        );
        assert!(out.tokens <= limit, "{serialized}");
        assert_eq!(out, select(&req).unwrap());
        if out.documents.is_empty() {
            assert_eq!(out.status, "budget_exhausted");
        } else {
            assert_eq!(out.documents[0].document.blocks.len(), 3);
        }
    }
}

#[test]
fn normalization_changes_ranking_without_changing_original_text() {
    let req = request(
        json!({"query":"API ガッコウ","max_tokens":1000,"documents":[
        {"id":"a","url":"https://example.test/a","title":"ＡＰＩ","blocks":[
          {"id":"b","kind":"paragraph","text":"ｶﾞｯｺｳ の ＡＰＩ"}]}]}),
    );
    let out = select(&req).unwrap();
    assert_eq!(out.documents[0].document.blocks[0].text, "ｶﾞｯｺｳ の ＡＰＩ");
}

#[test]
fn no_overlap_is_explicit_and_not_an_absence_claim() {
    let mut req = fixture(1000);
    req.query = "astronomy spacecraft".into();
    let out = select(&req).unwrap();
    assert_eq!(out.status, "no_lexical_overlap");
    assert!(out.documents.is_empty());
    assert_eq!(out.omitted_count, 5);
}

#[test]
fn rejects_invalid_queries_ids_and_context_links() {
    let mut req = fixture(1000);
    req.query = " ?! ".into();
    assert!(select(&req).is_err());
    req = fixture(1);
    assert!(select(&req).is_err());
    req = fixture(1000);
    req.documents[0].blocks[1].parent_id = Some("missing".into());
    assert!(select(&req).is_err());
    req = fixture(1000);
    req.documents[0].blocks[0].parent_id = Some("p".into());
    assert!(select(&req).is_err());
    req = fixture(1000);
    req.documents[1].id = "terms".into();
    assert!(select(&req).is_err());
}

#[test]
fn huge_atomic_block_does_not_hide_a_smaller_relevant_result() {
    let mut req = fixture(400);
    req.documents[0].blocks[1].text = "解約 違約金 HUGE_BLOCK ".repeat(2000);
    req.documents[1].blocks[0].text = "解約の違約金は無料です。".into();
    let out = select(&req).unwrap();
    assert_eq!(out.documents.len(), 1);
    assert_eq!(out.documents[0].document.id, "weather");
    assert!(!serde_json::to_string(&out).unwrap().contains("HUGE_BLOCK"));
}

#[test]
fn exact_duplicate_documents_share_one_selection() {
    let mut req = fixture(1000);
    let mut duplicate = req.documents[0].clone();
    duplicate.id = "mirror".into();
    req.documents.push(duplicate);
    let out = select(&req).unwrap();
    assert_eq!(out.documents.len(), 1);
    assert_eq!(out.duplicate_count, 4);
}

#[test]
fn search_adapter_keeps_citations_and_rejects_malformed_results() {
    let docs = adapters::search_results(&json!({"results":[
      {"id":"r1","url":"https://example.test/r1","title":"料金","text":"解約時の違約金。"}
    ]}))
    .unwrap();
    assert_eq!(docs[0].id, "r1");
    assert_eq!(docs[0].blocks[0].text, "解約時の違約金。");
    assert!(adapters::search_results(&json!({"results":[{"title":"missing fields"}]})).is_err());
}

#[test]
fn markdown_adapter_preserves_whole_table_heading_and_footnote() {
    let markdown = "# 規約\n\n## 解約\n\n| 期間 | 違約金 |\n|---|---|\n| 初月 | 無料[^a] |\n\n[^a]: 法人契約を除く。\n\n## その他\n\nWEATHER_NOISE 晴れ。\n";
    let docs = adapters::markdown_page(&json!({"id":"terms","url":"https://example.test/terms","title":"規約","markdown":markdown})).unwrap();
    let out = select(&SelectionRequest {
        query: "違約金".into(),
        query_expansions: Vec::new(),
        max_tokens: 1000,
        documents: docs,
    })
    .unwrap();
    let text = serde_json::to_string(&out).unwrap();
    assert!(text.contains("法人契約を除く"));
    assert!(!text.contains("WEATHER_NOISE"));
    let table = out.documents[0]
        .document
        .blocks
        .iter()
        .find(|b| b.kind == "table")
        .unwrap();
    assert_eq!(
        table.text,
        "| 期間 | 違約金 |\n|---|---|\n| 初月 | 無料[^a] |\n"
    );
    assert_eq!((table.start_line, table.end_line), (Some(5), Some(7)));
}

#[test]
fn different_sources_are_not_deduplicated_and_references_are_checked() {
    let mut req = fixture(1000);
    let mut mirror = req.documents[0].clone();
    mirror.id = "other-publisher".into();
    mirror.url = "https://another.test/terms".into();
    req.documents.push(mirror);
    let out = select(&req).unwrap();
    assert_eq!(out.documents.len(), 2);
    assert_eq!(out.duplicate_count, 0);
    req.documents[0].blocks[1]
        .related_ids
        .push("missing-note".into());
    assert!(select(&req).is_err());
    req = fixture(1000);
    req.documents[0].url.clear();
    assert!(select(&req).is_err());
}

#[test]
fn missing_markdown_footnotes_are_reported_but_code_is_literal() {
    let page = |markdown| json!({"id":"p","url":"https://example.test/p","title":"page","markdown":markdown});
    assert!(adapters::markdown_page(&page("違約金は無料[^missing]。\n")).is_err());
    let docs = adapters::markdown_page(&page(
        "# Sample\n\n```text\n[^literal]\n# not a heading\n```\n",
    ))
    .unwrap();
    assert_eq!(docs[0].blocks.len(), 2);
    assert_eq!(docs[0].blocks[1].kind, "code");
    assert!(docs[0].blocks[1].text.contains("# not a heading"));
}

#[test]
fn shared_heading_expansion_is_bounded_before_ranking() {
    let mut req = fixture(1000);
    req.query = "astronomy".into();
    let doc = &mut req.documents[0];
    doc.blocks.truncate(1);
    doc.blocks[0].text = "x".repeat(17_000);
    let child: jrg::selection::Block = serde_json::from_value(json!({
        "id":"p","kind":"paragraph","text":"short text","parent_id":"h"
    }))
    .unwrap();
    for i in 0..520 {
        let mut block = child.clone();
        block.id = format!("p-{i}");
        doc.blocks.push(block);
    }
    assert!(select(&req).unwrap_err().contains("expanded ranking"));
}
