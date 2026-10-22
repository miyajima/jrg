use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(adapter: &str, input: &str) -> Output {
    run_args(adapter, input, &[])
}

fn run_args(adapter: &str, input: &str, extra: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg-select"))
        .args(["--adapter", adapter])
        .args(extra)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_owned();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    out
}

#[test]
fn frequent_numeric_aliases_work_for_all_adapters_and_can_be_disabled() {
    for (adapter, input) in [
        (
            "documents",
            json!({"documents":[{"id":"n","url":"https://example.test/n","title":"Limits","blocks":[{"id":"p","kind":"paragraph","text":"Non-positive durations delete immediately."}]}]}),
        ),
        (
            "search-results",
            json!({"input":{"results":[{"id":"n","url":"https://example.test/n","title":"Limits","text":"Non-positive durations delete immediately."}]}}),
        ),
        (
            "markdown-page",
            json!({"input":{"id":"n","url":"https://example.test/n","title":"Limits","markdown":"# Limits\n\nNon-positive durations delete immediately.\n"}}),
        ),
    ] {
        let mut input = input;
        input["query"] = json!("ゼロ負数時の扱い");
        input["query_expansions"] = json!(["zero or negative"]);
        input["max_tokens"] = json!(700);
        let enabled = run_args(adapter, &input.to_string(), &["--query-rules", "builtin"]);
        assert!(
            enabled.status.success(),
            "{}",
            String::from_utf8_lossy(&enabled.stderr)
        );
        let result: Value = serde_json::from_slice(&enabled.stdout).unwrap();
        assert_eq!(result["documents"][0]["id"], "n");
        assert!(result["tokens"].as_u64().unwrap() <= 700);
        let standard = run(adapter, &input.to_string());
        let disabled = run_args(adapter, &input.to_string(), &["--query-rules", "off"]);
        assert_eq!(standard.stdout, disabled.stdout);
        assert!(
            disabled.status.success(),
            "{}",
            String::from_utf8_lossy(&disabled.stderr)
        );
        let result: Value = serde_json::from_slice(&disabled.stdout).unwrap();
        assert_eq!(result["status"], "no_lexical_overlap");
    }
}

#[test]
fn search_results_are_selected_before_cli_output() {
    let input = json!({"query":"解約 違約金","max_tokens":500,"input":{"results":[
        {"id":"keep","url":"https://example.test/terms","title":"解約","text":"違約金は無料です。"},
        {"id":"drop","url":"https://example.test/weather","title":"天気","text":"RAW_NOISE_DO_NOT_RETURN 晴れ。"}
    ]}});
    let out = run("search-results", &input.to_string());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let response: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(response["documents"][0]["id"], "keep");
    assert_eq!(response["omitted_count"], 1);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("RAW_NOISE_DO_NOT_RETURN"));
}

#[test]
fn markdown_page_adapter_is_available_from_cli() {
    let input = json!({"query":"違約金","max_tokens":500,"input":{
        "id":"terms","url":"https://example.test/terms","title":"規約",
        "markdown":"# 規約\n\n違約金は無料です。\n\n# 天気\n\nRAW_NOISE_DO_NOT_RETURN 晴れ。\n"}});
    let out = run("markdown-page", &input.to_string());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("RAW_NOISE_DO_NOT_RETURN"));
}

#[test]
fn invalid_input_produces_no_evidence_on_stdout() {
    for input in [
        "not json",
        "{}",
        r#"{"query":"x","max_tokens":500,"input":{"results":[{}]}}"#,
    ] {
        let out = run("search-results", input);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

#[test]
fn bilingual_expansions_work_in_every_cli_adapter() {
    let inputs = [
        (
            "search-results",
            json!({"results":[{"id":"terms","url":"https://example.test/terms","title":"Terms","text":"Cancellation fees are waived."}]}),
        ),
        (
            "markdown-page",
            json!({"id":"terms","url":"https://example.test/terms","title":"Terms","markdown":"## Termination\n\nCancellation fees are waived.\n"}),
        ),
        (
            "documents",
            json!([{"id":"terms","url":"https://example.test/terms","title":"Terms","blocks":[{"id":"p","kind":"paragraph","text":"Cancellation fees are waived."}]}]),
        ),
    ];
    for (adapter, input) in inputs {
        let mut request = json!({"query":"解約の違約金","query_expansions":["cancellation fees"],"max_tokens":500});
        request[if adapter == "documents" {
            "documents"
        } else {
            "input"
        }] = input;
        let output = run(adapter, &request.to_string());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let selected: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(selected["documents"][0]["id"], "terms", "{adapter}");
        assert!(
            output
                .stdout
                .windows(b"Cancellation fees are waived.".len())
                .any(|w| w == b"Cancellation fees are waived.")
        );
        for invalid in [
            json!("fees"),
            json!([null]),
            json!([" "]),
            json!(["a", "b", "c", "d", "e"]),
        ] {
            request["query_expansions"] = invalid;
            let output = run(adapter, &request.to_string());
            assert!(!output.status.success(), "{adapter}");
            assert!(output.stdout.is_empty());
        }
    }
}

#[test]
fn rule_subset_is_explicit_and_unknown_rules_fail_closed() {
    let input = json!({"query":"zero or negative","max_tokens":700,"documents":[]}).to_string();
    assert!(
        !run_args("documents", &input, &["--rule-ids", "nonpositive"])
            .status
            .success()
    );
    assert!(
        !run_args(
            "documents",
            &input,
            &["--query-rules", "builtin", "--rule-ids", "unknown"]
        )
        .status
        .success()
    );
    assert!(
        run_args(
            "documents",
            &input,
            &["--query-rules", "builtin", "--rule-ids", "nonpositive"]
        )
        .status
        .success()
    );
}
