use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
use tempfile::TempDir;
fn exchange(root: &std::path::Path, requests: Vec<Value>) -> Vec<Value> {
    let mut c = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .args(["--smart", "--mcp", "--root"])
        .arg(root)
        .args(["--session"])
        .arg(root.join("state.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = c.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        for request in requests {
            writeln!(input, "{request}").unwrap();
        }
    });
    let out = c.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn init() -> Vec<Value> {
    vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}
fn call(id: u32, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"retrieve","arguments":args}})
}

#[test]
fn requirements_link_items_to_entire_predicates_and_report_unresolved_items() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("guard.rs"), "fn receive(status: &str, candidates: &[u8], version: u32) {\n    if status != \"completed\"\n        || !candidates.is_empty()\n        || version == 0\n    {\n        return;\n    }\n}\n").unwrap();
    let args = json!({"operations":[{"read":{"path":"guard.rs","start":1,"end":8}}],"requirements":[{"id":"R4","description":"状態と候補の受信条件","patterns":["status !="]},{"id":"R5","description":"未取得の条件","patterns":["next_offset"]}]});
    let mut req = init();
    req.push(call(2, args.clone()));
    req.push(call(3, args));
    let out = exchange(d.path(), req);
    for response in &out[1..] {
        assert_ne!(response["result"]["isError"], true, "{response}");
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("# item R4 状態と候補の受信条件 state=matched"),
            "{text}"
        );
        assert!(
            text.contains("# condition R4 ") && text.contains(":2-4"),
            "{text}"
        );
        assert!(
            text.contains("3:        || !candidates.is_empty()")
                && text.contains("4:        || version == 0"),
            "{text}"
        );
        assert!(
            text.contains("# item R5 未取得の条件 state=unresolved"),
            "{text}"
        );
        assert!(text.contains("not semantic coverage"), "{text}");
    }
}

#[test]
fn requirements_do_not_claim_a_partial_read_contains_the_whole_condition() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("guard.rs"),
        "fn f(status: bool, more: bool) {\n if status\n    || more\n { return; }\n}\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2, json!({"operations":[{"read":{"path":"guard.rs","start":2,"end":2}}],"requirements":[{"id":"R1","description":"条件","patterns":["status"]}]})));
    let out = exchange(d.path(), req);
    let text = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("conditions_incomplete=1") && text.contains("not_fully_retrieved"),
        "{text}"
    );
    assert!(!text.contains("3:    || more"), "{text}");
}

#[test]
fn invalid_requirements_and_insufficient_map_budget_do_not_change_seen_state() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x"), "evidence\n").unwrap();
    let mut req = init();
    for (i, requirements, budget) in [
        (
            2,
            json!([{"id":"R1","description":"x","patterns":[]}]),
            24000,
        ),
        (
            3,
            json!([{"id":"R1","description":"x","patterns":["x"]},{"id":"R1","description":"y","patterns":["y"]}]),
            24000,
        ),
        (
            4,
            json!([{"id":"R1","description":"x","patterns":["x"]}]),
            64,
        ),
    ] {
        req.push(call(i, json!({"operations":[{"read":{"path":"x","start":1,"end":1}}],"requirements":requirements,"max_tokens":budget})));
    }
    let out = exchange(d.path(), req);
    for response in &out[1..] {
        assert_eq!(response["result"]["isError"], true, "{response}");
    }
    assert!(!d.path().join("state.json").exists());
}

#[test]
fn requirement_links_do_not_treat_else_body_as_a_true_branch() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        "fn f(allowed: bool) {\n if allowed {\n  accept();\n } else {\n  reject();\n }\n}\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"read":{"path":"x.rs","start":1,"end":7}}],"requirements":[{"id":"R1","description":"拒否処理","patterns":["reject()"]}]})));
    let out = exchange(d.path(), req);
    let text = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("conditions=0") && !text.contains("# condition R1"),
        "{text}"
    );
}

#[test]
fn requirement_budget_omissions_remain_pending_and_source_rows_are_exact() {
    let d = TempDir::new().unwrap();
    let source = format!(
        "fn f(status: bool) {{\n if status\n{} {{\n  return;\n }}\n}}\n",
        (0..90)
            .map(|n| format!("    || flag_{n:03}\n"))
            .collect::<String>()
    );
    fs::write(d.path().join("条件.rs"), &source).unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"read":{"path":"条件.rs","start":1,"end":96}}],"requirements":[{"id":"R1","description":"長い条件","patterns":["if status"]}],"max_tokens":512})));
    req.push(call(
        3,
        json!({"operations":[],"resume":true,"max_tokens":26000}),
    ));
    let out = exchange(d.path(), req);
    let text = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("omitted=1") && !text.contains("# condition R1"),
        "{text}"
    );
    assert!(
        tiktoken_rs::o200k_base()
            .unwrap()
            .encode_ordinary(text)
            .len()
            <= 512,
        "{text}"
    );
    assert!(text.contains("条件.rs:"), "{text}");
    let resumed = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        resumed.contains("flag_089") && resumed.contains("pending=0"),
        "{resumed}"
    );
}

#[test]
fn requirement_predicates_support_python_ts_js_and_label_unsupported_syntax() {
    for (ext, source) in [
        (
            "py",
            "def f(status, more):\n    if (status\n        or more):\n        return\n",
        ),
        (
            "ts",
            "function f(status: boolean, more: boolean) {\n if (status\n     || more) { return; }\n}\n",
        ),
        (
            "js",
            "function f(status, more) {\n if (status\n     || more) { return; }\n}\n",
        ),
        ("txt", "status\nmore\n"),
        ("rs", "fn broken( {\nstatus\nmore\n"),
    ] {
        let d = TempDir::new().unwrap();
        let path = format!("source.{ext}");
        fs::write(d.path().join(&path), source).unwrap();
        let mut req = init();
        req.push(call(2,json!({"operations":[{"read":{"path":path,"start":1,"end":source.lines().count()}}],"requirements":[{"id":"R1","description":"条件","patterns":["status"]}]})));
        let out = exchange(d.path(), req);
        let text = out[1]["result"]["content"][0]["text"].as_str().unwrap();
        if ["py", "ts", "js"].contains(&ext) {
            assert!(
                text.contains("# condition R1") && text.contains("syntax=ast"),
                "{ext}: {text}"
            );
        } else {
            assert!(text.contains("syntax=unavailable"), "{ext}: {text}");
        }
    }
}

#[test]
fn cli_batch_accepts_the_same_requirement_map_contract() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("guard.rs"),
        "fn f(a: bool) {\n if a { return; }\n}\n",
    )
    .unwrap();
    let request = json!({"operations":[{"read":{"path":d.path().join("guard.rs"),"start":1,"end":3}}],"requirements":[{"id":"R1","description":"条件","patterns":["if a"]}]});
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .args(["--smart", "--batch", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let result = child.wait_with_output().unwrap();
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(
        result.status.success() && text.contains("# condition R1"),
        "{text}"
    );
}

#[test]
fn requirement_map_gives_each_item_a_chance_to_show_its_whole_predicate() {
    let d = TempDir::new().unwrap();
    let noise = (0..24)
        .map(|n| format!(" if noise_{n} {{ return; }}\n"))
        .collect::<String>();
    let clauses = (0..40)
        .map(|n| format!("     || unique_flag_{n:03}\n"))
        .collect::<String>();
    let source = format!("fn f() {{\n{noise} if status\n{clauses} {{ return; }}\n}}\n");
    fs::write(d.path().join("x.rs"), &source).unwrap();
    let requirements: Vec<_> = (1..=6).map(|n| json!({"id":format!("R{n}"),"description":"依頼項目","patterns":[if n==4 {"status"} else {"noise_"}]})).collect();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"read":{"path":"x.rs","start":1,"end":source.lines().count()}}],"requirements":requirements})));
    let out = exchange(d.path(), req);
    let text = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    let record = text
        .split("# condition R4 ")
        .nth(1)
        .expect(text)
        .split('#')
        .next()
        .unwrap();
    assert!(record.contains("unique_flag_039"), "{text}");
}
#[test]
fn typed_retrieval_batches_literal_regex_and_reads_with_shared_seen_state() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.txt"),
        "alpha(value)\n日本語\n$(touch NEVER)\n",
    )
    .unwrap();
    let mut req = init();
    req.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    req.push(call(3,json!({"operations":[{"search":{"patterns":[r"alpha\(value\)"],"paths":["x.txt"],"regex":true}},{"read":{"path":"x.txt","start":2,"end":3}}]})));
    req.push(call(
        4,
        json!({"operations":[{"read":{"path":"x.txt","start":1,"end":3}}]}),
    ));
    let out = exchange(d.path(), req);
    assert_eq!(out.len(), 4);
    assert_eq!(out[0]["result"]["protocolVersion"], "2025-11-25");
    let instructions = out[0]["result"]["instructions"].as_str().unwrap();
    assert!(instructions.len() <= 4096);
    let lead: String = instructions.chars().take(512).collect();
    assert!(lead.contains("Prefer jrg") && lead.contains("rg"));
    assert!(lead.contains("exhaustive") && lead.contains("machine"));
    assert!(instructions.contains("unavailable") && instructions.contains("configured root"));
    assert_eq!(out[1]["result"]["tools"][0]["name"], "retrieve");
    let first = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        first.contains("alpha(value)")
            && first.contains("日本語")
            && first.contains("$(touch NEVER)"),
        "{first}"
    );
    let second = out[3]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        second.contains("seen=3") && !second.contains("alpha(value)"),
        "{second}"
    );
    assert!(!d.path().join("NEVER").exists());
}
#[test]
fn invalid_plan_and_symlink_escape_are_rejected_before_any_read_or_state_change() {
    let d = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(d.path().join("x"), "allowed\n").unwrap();
    fs::write(outside.path().join("secret"), "PRIVATE\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret"), d.path().join("escape")).unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"read":{"path":"x","start":1,"end":1}},{"read":{"path":outside.path().join("secret"),"start":1,"end":1}}]})));
    req.push(call(
        3,
        json!({"operations":[{"read":{"path":"escape","start":1,"end":1}}]}),
    ));
    req.push(call(
        4,
        json!({"operations":[{"search":{"patterns":["allowed"],"paths":["x"],"unknown":true}}]}),
    ));
    let out = exchange(d.path(), req);
    for result in &out[1..] {
        assert_eq!(result["result"]["isError"], true, "{result}");
        assert!(!result.to_string().contains("PRIVATE"));
    }
    assert!(!d.path().join("state.json").exists());
}
#[test]
fn protocol_errors_do_not_crash_server() {
    let d = TempDir::new().unwrap();
    let mut req = init();
    req.push(json!({"jsonrpc":"2.0","id":2,"method":"unknown"}));
    req.push(json!({"jsonrpc":"2.0","id":3,"method":"ping"}));
    let out = exchange(d.path(), req);
    assert_eq!(out[1]["error"]["code"], -32601);
    assert_eq!(out[2]["result"], json!({}));
}

#[test]
fn japanese_normalization_explicit_context_and_pending_resume_work_in_typed_tool() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("ja.txt"), "ｶﾞｯｺｳ\nガッコウ\n").unwrap();
    fs::write(
        d.path().join("long.txt"),
        (1..=120)
            .map(|n| format!("evidence_line_{n:03} unique_payload_{n:03}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut req = init();
    req.push(call(
        2,
        json!({"operations":[{"search":{"patterns":["がっこう"],"paths":["ja.txt"],"context":0}}]}),
    ));
    req.push(call(
        3,
        json!({"operations":[{"read":{"path":"long.txt","start":1,"end":120}}],"max_tokens":64}),
    ));
    req.push(call(4, json!({"operations":[],"resume":true})));
    let out = exchange(d.path(), req);
    let text = |i: usize| out[i]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text(1).contains("ｶﾞｯｺｳ") && text(1).contains("ガッコウ"),
        "{}",
        text(1)
    );
    assert!(
        text(2).contains("pending=") && text(2).contains("partial=1"),
        "{}",
        text(2)
    );
    assert!(
        tiktoken_rs::o200k_base()
            .unwrap()
            .encode_ordinary(text(2))
            .len()
            <= 64
    );
    assert!(
        text(3).contains("120:evidence_line_120") && text(3).contains("pending=0"),
        "{}",
        text(3)
    );
    for row in text(2)
        .lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()))
    {
        assert!(!text(3).lines().any(|l| l == row));
    }
}

#[test]
fn unlimited_outer_rank_query_preserves_legacy_search_and_read_ranking() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "hit ordinary unrelated\n").unwrap();
    fs::write(d.path().join("z.txt"), "hit cancelAudio staleEffect\n").unwrap();
    fs::write(d.path().join("excluded.txt"), "cancelAudio staleEffect\n").unwrap();
    let operations = json!([{"search":{"patterns":["hit"],"paths":["a.txt","z.txt"]}},{"read":{"path":"a.txt","start":1,"end":1}}]);
    let mut req = init();
    req.push(call(
        2,
        json!({"operations":operations,"rank_query":"cancel audio stale effect","discovery_limit":0}),
    ));
    req.push(call(3, json!({"operations":operations,"repeat":true})));
    let out = exchange(d.path(), req);
    let ranked = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        ranked.contains("rank=bm25")
            && ranked.contains("hit ordinary unrelated")
            && ranked.contains("hit cancelAudio staleEffect"),
        "{ranked}"
    );
    assert!(
        ranked.find("z.txt").unwrap() < ranked.find("a.txt").unwrap(),
        "{ranked}"
    );
    assert!(!ranked.contains("excluded.txt"));
    let normal = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        !normal.contains("rank=bm25")
            && normal.find("a.txt").unwrap() < normal.find("z.txt").unwrap(),
        "{normal}"
    );
}

#[test]
fn invalid_typed_rank_query_is_rejected_before_session_updates() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x"), "hit\n").unwrap();
    let mut req = init();
    for (id, query) in [json!("!!!"), json!(5), json!("")].into_iter().enumerate() {
        req.push(call(id as u32+2,json!({"operations":[{"search":{"patterns":["hit"],"paths":["x"]}}],"rank_query":query})));
    }
    let out = exchange(d.path(), req);
    for r in &out[1..] {
        assert_eq!(r["result"]["isError"], true, "{r}");
    }
    assert!(!d.path().join("state.json").exists());
}

#[test]
fn discovery_limits_broad_candidates_but_protects_reads_and_function_scopes() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("candidates")).unwrap();
    for n in 0..20 {
        fs::write(
            d.path().join(format!("candidates/a{n:02}.txt")),
            format!("hit ordinary candidate {n}\n"),
        )
        .unwrap();
    }
    fs::write(
        d.path().join("candidates/z.txt"),
        "hit cancelAudio staleEffect\n",
    )
    .unwrap();
    fs::write(
        d.path().join("required.txt"),
        (1..=300)
            .map(|n| format!("required row {n}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        d.path().join("function.rs"),
        "fn required_guard() {\n    if unrelated {\n        inspect();\n    }\n    finish();\n}\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2, json!({"rank_query":"cancel audio stale effect", "discovery_limit":2,"max_tokens":26000,"operations":[
        {"search":{"patterns":["hit"],"paths":["candidates"]}},
        {"read":{"path":"required.txt","start":1,"end":300}},
        {"search":{"patterns":["required_guard"],"paths":["function.rs"],"intent":"explain"}}
    ]})));
    req.push(call(
        3,
        json!({"operations":[{"read":{"path":"candidates/a19.txt","start":1,"end":1}}]}),
    ));
    req.push(call(4,json!({"rank_query":"cancel audio stale effect","discovery_limit":2,"operations":[{"search":{"patterns":["hit"],"paths":["candidates"]}}]})));
    let out = exchange(d.path(), req);
    let body = |i: usize| out[i]["result"]["content"][0]["text"].as_str().unwrap();
    let first = body(1);
    assert!(!out[1]["result"]["isError"].as_bool().unwrap(), "{first}");
    assert_eq!(
        first.lines().filter(|l| l.starts_with("1:hit ")).count(),
        2,
        "{first}"
    );
    assert!(first.contains("1:hit cancelAudio staleEffect"), "{first}");
    assert!(
        first.contains("300:required row 300") && first.contains("5:    finish();"),
        "{first}"
    );
    assert!(
        first.find("300:required row 300").unwrap() < first.find("1:hit cancelAudio").unwrap(),
        "{first}"
    );
    assert!(
        first.contains("discovery_selected=2 discovery_omitted=19") && first.contains("pending=0"),
        "{first}"
    );
    assert!(first.contains("# unselected candidates/a"), "{first}");
    assert!(
        body(2).contains("1:hit ordinary candidate 19"),
        "{}",
        body(2)
    );
    let next = body(3);
    assert_eq!(
        next.lines().filter(|l| l.starts_with("1:hit ")).count(),
        2,
        "{next}"
    );
    assert!(
        !next.contains("1:hit cancelAudio") && !next.contains("1:hit ordinary candidate 19"),
        "{next}"
    );
    assert!(next.contains("pending=0"), "{next}");
}

#[test]
fn discovery_default_is_twelve_and_narrow_or_explain_requests_are_not_capped() {
    let d = TempDir::new().unwrap();
    for n in 0..15 {
        fs::write(
            d.path().join(format!("x{n:02}.txt")),
            format!("hit value {n}\n"),
        )
        .unwrap();
    }
    fs::write(d.path().join("all.txt"), "needle value\n\n".repeat(20)).unwrap();
    let mut req = init();
    req.push(call(2,json!({"rank_query":"value","operations":[{"search":{"patterns":["hit"],"paths":["."],"globs":["x*.txt"]}}]})));
    req.push(call(3,json!({"rank_query":"value","operations":[{"search":{"patterns":["needle"],"paths":["all.txt"]}}]})));
    req.push(call(4,json!({"rank_query":"value","repeat":true,"operations":[{"search":{"patterns":["hit"],"paths":["."],"intent":"explain","context":0,"globs":["x*.txt"]}}]})));
    req.push(call(5,json!({"rank_query":"value","discovery_limit":0,"repeat":true,"operations":[{"search":{"patterns":["hit"],"paths":["."],"globs":["x*.txt"]}}]})));
    let out = exchange(d.path(), req);
    let body = |i: usize| out[i]["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        body(1).lines().filter(|l| l.starts_with("1:hit ")).count(),
        12,
        "{}",
        body(1)
    );
    assert_eq!(
        body(2)
            .lines()
            .filter(|l| l.contains(":needle value"))
            .count(),
        20,
        "{}",
        body(2)
    );
    assert_eq!(
        body(3).lines().filter(|l| l.starts_with("1:hit ")).count(),
        15,
        "{}",
        body(3)
    );
    assert_eq!(
        body(4).lines().filter(|l| l.starts_with("1:hit ")).count(),
        15,
        "{}",
        body(4)
    );
}

#[test]
fn invalid_discovery_limit_is_rejected_before_session_updates() {
    for limit in [json!(-1), json!(257), json!(1.5), json!("2")] {
        let d = TempDir::new().unwrap();
        fs::write(d.path().join("x.txt"), "hit\n").unwrap();
        let mut req = init();
        req.push(call(2,json!({"rank_query":"hit","discovery_limit":limit,"operations":[{"search":{"patterns":["hit"]}}]})));
        let out = exchange(d.path(), req);
        assert_eq!(out[1]["result"]["isError"], true, "{out:?}");
        assert!(!d.path().join("state.json").exists());
    }
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "hit\n").unwrap();
    let mut req = init();
    req.push(call(
        2,
        json!({"discovery_limit":2,"operations":[{"search":{"patterns":["hit"]}}]}),
    ));
    let out = exchange(d.path(), req);
    assert_eq!(out[1]["result"]["isError"], true);
    assert!(!d.path().join("state.json").exists());
}

#[test]
fn review_collects_functions_guards_constants_helpers_and_related_tests_once() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::create_dir(d.path().join("tests")).unwrap();
    fs::write(d.path().join("src/audio.ts"),"const MAX_AUDIO_MS = 120000;\nfunction isStale(effect: string) {\n  return effect === 'old';\n}\nfunction cancelAudio(effect: string) {\n  if (isStale(effect)) {\n    return MAX_AUDIO_MS;\n  } else {\n    return 0;\n  }\n}\nfunction unrelated() { return 'UNRELATED_BODY'; }\n").unwrap();
    fs::write(d.path().join("tests/audio.test.ts"),"test('old effect cancellation remains bounded', () => {\n  const value = cancelAudio('old');\n  assert.equal(value, 120000);\n});\ntest('unrelated behavior', () => {\n  assert.equal('UNRELATED_TEST', 'UNRELATED_TEST');\n});\n").unwrap();
    let mut req = init();
    req.push(call(2,json!({"max_tokens":26000,"rank_query":"cancel audio stale effect","operations":[{"search":{"patterns":["cancelAudio"],"paths":["src"],"intent":"review"}}]})));
    req.push(call(3,json!({"operations":[{"search":{"patterns":["cancelAudio"],"paths":["src"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!out[1]["result"]["isError"].as_bool().unwrap(), "{s}");
    for expected in [
        "const MAX_AUDIO_MS",
        "function isStale",
        "function cancelAudio",
        "return 0;",
        "assert.equal(value, 120000)",
    ] {
        assert!(s.contains(expected), "missing {expected}: {s}");
    }
    assert!(
        !s.contains("UNRELATED_BODY") && !s.contains("UNRELATED_TEST"),
        "{s}"
    );
    assert!(s.contains("# bundle ") && s.contains("pending=0"), "{s}");
    let next = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        !next.contains("function cancelAudio") && !next.contains("assert.equal(value"),
        "{next}"
    );
}

#[test]
fn review_finds_rust_inline_and_python_tests_and_keeps_branch_siblings() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("src")).unwrap();
    fs::create_dir(d.path().join("tests")).unwrap();
    fs::write(d.path().join("src/normalize.rs"),"fn normalize(value: &str) -> String {\n if value.is_empty() {\n   String::new()\n } else {\n   value.to_owned()\n }\n}\n#[test]\nfn normalization_maps_original() {\n assert_eq!(normalize(\"x\"), \"x\");\n}\n").unwrap();
    fs::write(d.path().join("src/decoder.py"),"def decode_line(value):\n    if value:\n        return value.decode('utf-8')\n    return ''\n\ndef unrelated():\n    return 'UNRELATED_BODY'\n").unwrap();
    fs::write(
        d.path().join("tests/test_compatibility.py"),
        "def test_legacy_decode():\n    assert decode_line(b'x') == 'x'\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"search":{"patterns":["normalize","decode_line"],"paths":["src"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    for expected in [
        "value.to_owned()",
        "assert_eq!(normalize",
        "return ''",
        "assert decode_line",
    ] {
        assert!(s.contains(expected), "missing {expected}: {s}");
    }
    assert!(!s.contains("UNRELATED_BODY"), "{s}");
}

#[test]
fn review_ignores_context_hint_and_marks_heuristic_test_gaps() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "fn cancel_audio() { finish(); }\n").unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"search":{"patterns":["cancel_audio"],"paths":["x.rs"],"intent":"review","context":0}}]})));
    req.push(call(3,json!({"repeat":true,"operations":[{"search":{"patterns":["cancel_audio"],"paths":["x.rs"],"intent":"review","context":3}}]})));
    req.push(call(4,json!({"repeat":true,"operations":[{"search":{"patterns":["cancel_audio"],"paths":["x.rs"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let mut sources = Vec::new();
    for response in &out[1..] {
        assert_eq!(response["result"]["isError"], false, "{response}");
        let s = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(s.contains("tests=0") && s.contains("not proof"), "{s}");
        sources.push(
            s.lines()
                .filter(|line| {
                    line.starts_with('@') || line.chars().next().is_some_and(|c| c.is_ascii_digit())
                })
                .collect::<Vec<_>>(),
        );
    }
    assert!(
        sources[0]
            .iter()
            .any(|line| line.contains("fn cancel_audio"))
    );
    assert_eq!(sources[0], sources[1]);
    assert_eq!(sources[1], sources[2]);
}

#[test]
fn mcp_guidance_examples_execute_and_errors_support_recovery() {
    let d = TempDir::new().unwrap();
    let mut req = init();
    req.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    let out = exchange(d.path(), req);
    let examples = out[1]["result"]["tools"][0]["inputSchema"]["examples"]
        .as_array()
        .expect("retrieve must publish executable usage examples");
    assert!(examples.len() >= 3);
    for example in examples {
        let fixture = TempDir::new().unwrap();
        fs::create_dir(fixture.path().join("src")).unwrap();
        fs::write(
            fixture.path().join("src/lib.rs"),
            "fn identifier() { marker(); }\n",
        )
        .unwrap();
        let mut req = init();
        req.push(call(2, example.clone()));
        let result = exchange(fixture.path(), req);
        assert_eq!(result[1]["result"]["isError"], false, "{result:?}");
        assert!(
            result[1]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("identifier")
        );
    }

    fs::create_dir(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/lib.rs"), "fn identifier() {}\n").unwrap();
    let mut req = init();
    req.push(call(2, json!({"query":"identifier"})));
    req.push(call(
        3,
        json!({"operations":[{"read":{"path":"src/missing.rs","start":1,"end":1}}]}),
    ));
    req.push(call(
        4,
        json!({"operations":[{"search":{"patterns":["identifier"],"paths":[]}}]}),
    ));
    req.push(call(5, json!({"operations":[{"search":{"patterns":[]}}]})));
    req.push(call(6, json!({"operations":[{"search":{"patterns":["identifier"],"paths":["src"],"intent":"review","context":129}}]})));
    req.push(call(7, json!({"operations":[{"search":{"patterns":["identifier"],"paths":["src"],"intent":"review","context":2}}]})));
    let result = exchange(d.path(), req);
    for response in &result[1..6] {
        assert_eq!(response["result"]["isError"], true, "{response}");
    }
    let messages: Vec<_> = result[1..6]
        .iter()
        .map(|r| r["result"]["content"][0]["text"].as_str().unwrap())
        .collect();
    assert!(
        messages[0].contains("query") && messages[0].contains("operations"),
        "{}",
        messages[0]
    );
    assert!(
        messages[1].contains("src/missing.rs")
            && messages[1].contains(&d.path().to_string_lossy().to_string())
            && messages[1].contains("read.path")
            && messages[1].contains("returned path"),
        "{}",
        messages[1]
    );
    assert!(
        messages[2].contains("search.paths") && messages[2].to_lowercase().contains("omit"),
        "{}",
        messages[2]
    );
    assert!(messages[3].contains("search.patterns"), "{}", messages[3]);
    assert!(messages[4].contains("0..128"), "{}", messages[4]);
    assert_eq!(result[6]["result"]["isError"], false, "{:?}", result[6]);
    assert!(
        result[6]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("fn identifier")
    );
}

#[test]
fn review_keeps_typescript_object_parameter_function_body_together() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("writer.ts"),"class Writer {\n private enqueueOutputWrite(\n   task: () => Promise<void>,\n   identity: { effectId?: string } = {}\n ): void {\n   const generation = 7;\n   if (identity.effectId === 'old') {\n     return;\n   }\n   task();\n   record(generation);\n }\n unrelated() { neverReadMe(); }\n}\n").unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"search":{"patterns":["effectId"],"paths":["writer.ts"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    for expected in [
        "private enqueueOutputWrite",
        "const generation = 7;",
        "task();",
        "record(generation);",
    ] {
        assert!(s.contains(expected), "missing {expected}: {s}");
    }
    assert!(!s.contains("neverReadMe"), "{s}");
}

#[test]
fn review_advances_past_seen_scopes_and_invalidates_changed_sources() {
    let d = TempDir::new().unwrap();
    for n in 0..10 {
        fs::write(
            d.path().join(format!("a{n}.rs")),
            format!("fn cancel_audio_{n}() {{ result({n}); }}\n"),
        )
        .unwrap();
    }
    let invoke = || {
        let mut req = init();
        req.push(call(2,json!({"operations":[{"search":{"patterns":["cancel_audio"],"paths":(0..10).map(|n|format!("a{n}.rs")).collect::<Vec<_>>(),"intent":"review"}}]})));
        let out = exchange(d.path(), req);
        out[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let first = invoke();
    assert_eq!(
        first
            .lines()
            .filter(|l| l.contains(":fn cancel_audio_"))
            .count(),
        8,
        "{first}"
    );
    let second = invoke();
    assert_eq!(
        second
            .lines()
            .filter(|l| l.contains(":fn cancel_audio_"))
            .count(),
        2,
        "{second}"
    );
    fs::write(
        d.path().join("a0.rs"),
        "fn cancel_audio_0() { result(99); }\n",
    )
    .unwrap();
    let third = invoke();
    assert!(
        third.contains("result(99)") && third.contains("changed=1"),
        "{third}"
    );
}

#[test]
fn review_selects_files_before_scopes_and_does_not_link_generic_test_names() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("scripts")).unwrap();
    fs::create_dir(d.path().join("tests")).unwrap();
    fs::write(d.path().join("scripts/portable_install.py"), "def verify(manifest):\n    # portable manifest staging symlink traversal checksum\n    return 'EXPECTED_VERIFY'\n\ndef stage(target):\n    return verify(target)\n").unwrap();
    fs::write(d.path().join("tests/test_portable_distribution.py"), "from scripts.portable_install import stage, verify\n\ndef test_manifest():\n    assert verify('manifest') == 'EXPECTED_VERIFY'\n\ndef test_staging():\n    assert stage('target') == 'EXPECTED_VERIFY'\n").unwrap();
    for n in 0..12 {
        fs::write(
            d.path().join(format!("scripts/noise{n}.py")),
            format!("def stage(target):\n    return 'NOISE_SOURCE_{n}'\n"),
        )
        .unwrap();
        fs::write(
            d.path().join(format!("tests/test_noise{n}.py")),
            format!("def test_verify():\n    assert verify('NOISE_TEST_{n}')\n"),
        )
        .unwrap();
    }
    let mut req = init();
    req.push(call(2,json!({"max_tokens":26000,"rank_query":"portable manifest staging symlink traversal checksum","operations":[{"search":{"patterns":["stage","verify"],"paths":["."],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        s.contains("EXPECTED_VERIFY")
            && s.contains("def test_manifest")
            && s.contains("def test_staging"),
        "{s}"
    );
    assert!(
        !s.contains("NOISE_SOURCE") && !s.contains("NOISE_TEST"),
        "{s}"
    );
    assert!(
        s.contains("# review_files selected=") && s.contains("pending=0"),
        "{s}"
    );
}

#[test]
fn review_shares_scope_slots_across_operations_and_test_only_does_not_expand_root() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("tests")).unwrap();
    for prefix in ['a', 'b'] {
        let body = (0..10)
            .map(|n| format!("fn {prefix}_audio_{n}() {{ output({n}); }}\n"))
            .collect::<String>();
        fs::write(d.path().join(format!("{prefix}.rs")), body).unwrap();
    }
    fs::write(
        d.path().join("tests/known.py"),
        "def test_stage():\n    assert stage('KNOWN_ONLY')\n",
    )
    .unwrap();
    fs::write(
        d.path().join("tests/other.py"),
        "def test_stage():\n    assert stage('DO_NOT_EXPAND_TEST_ONLY')\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"search":{"patterns":["a_audio"],"paths":["a.rs"],"intent":"review"}},{"search":{"patterns":["b_audio"],"paths":["b.rs"],"intent":"review"}}]})));
    req.push(call(3,json!({"operations":[{"search":{"patterns":["stage"],"paths":["tests/known.py"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(s.lines().filter(|l| l.contains(":fn ")).count(), 8, "{s}");
    assert!(s.contains("source=8"), "{s}");
    let s = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        s.contains("KNOWN_ONLY") && !s.contains("DO_NOT_EXPAND_TEST_ONLY"),
        "{s}"
    );
}

#[test]
fn dense_ascii_output_respects_transport_bytes_and_can_resume_undelivered_rows() {
    let d = TempDir::new().unwrap();
    let line = " const very_long_audio_identifier = ".to_owned()
        + &"abcdefghijklmnopqrstuvwxyz".repeat(100)
        + ";\n";
    fs::write(d.path().join("audio.ts"), line.repeat(50)).unwrap();
    let mut req = init();
    req.push(call(
        2,
        json!({"operations":[{"read":{"path":"audio.ts","start":1,"end":50}}],"max_tokens":26000}),
    ));
    req.push(call(
        3,
        json!({"operations":[],"resume":true,"max_tokens":26000}),
    ));
    let out = exchange(d.path(), req);
    let first = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(first.len() <= 96000, "transport bytes={}", first.len());
    assert!(!first.contains("pending=0"), "{first}");
    let second = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        second.len() <= 96000 && second.contains("pending=0"),
        "second bytes={}",
        second.len()
    );
    let numbers = |s: &str| {
        s.lines()
            .filter_map(|l| l.split_once(':').and_then(|(n, _)| n.parse::<usize>().ok()))
            .collect::<std::collections::BTreeSet<_>>()
    };
    let a = numbers(first);
    let b = numbers(second);
    assert!(a.is_disjoint(&b));
    assert_eq!(a.union(&b).count(), 50);
}

#[test]
fn review_file_preview_reaches_late_files_and_advances_past_seen_files() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("a_huge.rs"),
        "fn stage() {\n".to_owned() + &" consume(\"stage\");\n".repeat(5000) + "}\n",
    )
    .unwrap();
    for n in 0..5 {
        fs::write(
            d.path().join(format!("z_portable{n}.rs")),
            format!("fn stage_portable_{n}() {{ validate_manifest_symlink(\"FRESH_{n}\"); }}\n"),
        )
        .unwrap();
    }
    let invoke = || {
        let mut req = init();
        req.push(call(2,json!({"rank_query":"portable manifest symlink","operations":[{"search":{"patterns":["stage"],"paths":["."],"intent":"review"}}]})));
        let out = exchange(d.path(), req);
        out[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let a = invoke();
    let b = invoke();
    assert_eq!(
        a.lines()
            .filter(|l| l.contains(":fn stage_portable_"))
            .count(),
        3,
        "{a}"
    );
    assert_eq!(
        b.lines()
            .filter(|l| l.contains(":fn stage_portable_"))
            .count(),
        2,
        "{b}"
    );
    assert!(
        a.contains("capture_omitted=0") && !a.contains("preview_omitted=0"),
        "{a}"
    );
    for n in 0..5 {
        assert!(a.contains(&format!("FRESH_{n}")) || b.contains(&format!("FRESH_{n}")));
    }
}

#[test]
fn review_minimum_budget_preserves_counters_and_pending_within_limit() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("writer.rs"),
        "fn cancel_audio() {\n if stale() {\n  stop_audio();\n } else {\n  keep_audio();\n }\n}\n",
    )
    .unwrap();
    let mut req = init();
    req.push(call(2,json!({"operations":[{"search":{"patterns":["cancel_audio"],"paths":["."],"intent":"review"}}],"rank_query":"cancel audio stale","max_tokens":128})));
    req.push(call(
        3,
        json!({"operations":[],"resume":true,"max_tokens":26000}),
    ));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        tiktoken_rs::o200k_base().unwrap().encode_ordinary(s).len() <= 128,
        "{s}"
    );
    assert!(s.contains("pending=") && s.contains("errors=0"), "{s}");
}

#[test]
fn literal_dash_patterns_are_values_in_every_typed_search_mode() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("writer.ts"),
        "function stream() {\n return argv.includes('--stream');\n}\n// --glob=private\n",
    )
    .unwrap();
    let mut req = init();
    for (i, intent) in ["locate", "explain", "review"].iter().enumerate() {
        req.push(call(i as u32+2,json!({"repeat":true,"operations":[{"search":{"patterns":["--stream","--glob=private"],"paths":["writer.ts"],"intent":intent}}]})));
    }
    let out = exchange(d.path(), req);
    for r in &out[1..] {
        let s = r["result"]["content"][0]["text"].as_str().unwrap();
        assert_ne!(r["result"]["isError"], true, "{s}");
        assert!(
            s.contains("argv.includes('--stream')") && !s.contains("unexpected argument"),
            "{s}"
        );
    }
}

#[test]
fn document_review_returns_sections_with_seen_state_without_a_second_locate() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("PORTABLE.md"),"# Unrelated\nDO_NOT_FETCH_THIS_SECTION\n\n## Stage\nThe runtime_effect is unmeasured.\nRead back the active version before use.\nHost hooks stay unchanged.\n\n## Other\nDO_NOT_FETCH_OTHER_SECTION\n").unwrap();
    let mut req = init();
    req.push(call(2,json!({"rank_query":"runtime_effect stage host hooks","operations":[{"search":{"patterns":["runtime_effect"],"paths":["."],"globs":["*.md"],"intent":"review"}}]})));
    req.push(call(3,json!({"operations":[{"search":{"patterns":["runtime_effect"],"paths":["PORTABLE.md"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        s.contains("## Stage")
            && s.contains("Read back the active version")
            && s.contains("Host hooks"),
        "{s}"
    );
    assert!(
        !s.contains("DO_NOT_FETCH_THIS_SECTION") && !s.contains("DO_NOT_FETCH_OTHER_SECTION"),
        "{s}"
    );
    let s = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        !s.contains("The runtime_effect") && s.contains("pending=0"),
        "{s}"
    );
}

#[test]
fn broad_review_focuses_the_best_module_family_but_explicit_paths_remain_available() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join("server/src")).unwrap();
    fs::create_dir_all(d.path().join("desktop/src")).unwrap();
    fs::write(d.path().join("server/src/handler.rs"),"fn dispatch() {\n // audio cancellation queued drain generation provider effect\n return UNRELATED_DISPATCH;\n}\n".to_owned()+&"// boilerplate\n".repeat(20000)).unwrap();
    fs::write(d.path().join("desktop/src/audio.ts"),"function cancelAudio(effect: string) {\n // audio cancellation queued drain generation provider effect\n if (effect === 'old') return 'EXPECTED_CANCEL';\n return drainAudio();\n}\nfunction drainAudio() { return 'EXPECTED_DRAIN'; }\n").unwrap();
    let mut req = init();
    req.push(call(2,json!({"rank_query":"audio cancellation queued drain generation provider effect","operations":[{"search":{"patterns":["cancellation","drain"],"paths":["."],"intent":"review"}}]})));
    req.push(call(3,json!({"operations":[{"search":{"patterns":["dispatch"],"paths":["server/src/handler.rs"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        s.contains("EXPECTED_CANCEL")
            && s.contains("EXPECTED_DRAIN")
            && !s.contains("return UNRELATED_DISPATCH"),
        "{s}"
    );
    let s = out[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(s.contains("return UNRELATED_DISPATCH"), "{s}");
}

#[test]
fn json_review_collects_sibling_output_definition_and_schema_assertion_tests() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("src")).unwrap();
    fs::create_dir(d.path().join("tests")).unwrap();
    fs::write(
        d.path().join("src/search.rs"),
        "fn normalize(value: &str) -> String {\n value.to_owned()\n}\n",
    )
    .unwrap();
    fs::write(d.path().join("src/output.rs"),"fn emit_record(line: &Line) {\n let record = json!({\n  \"submatches\": line.ranges,\n  \"column\": line.text.chars().count() + 1,\n });\n serde_json::to_writer(record);\n}\n").unwrap();
    fs::write(d.path().join("tests/compat.rs"),"#[test]\nfn decoded_coordinates() {\n let value = run_cli(\"--json\");\n assert_eq!(value[\"submatches\"][0][\"column\"], 2);\n}\n").unwrap();
    let mut source = fs::read_to_string(d.path().join("src/search.rs")).unwrap();
    for n in 0..6 {
        source.push_str(&format!("#[test]\nfn normalize_case_{n}() {{\n assert!(normalize(\"Japanese normalize JSON submatches byte column coordinates\") != \"\");\n}}\n"));
    }
    fs::write(d.path().join("src/search.rs"), source).unwrap();
    let mut req = init();
    req.push(call(2,json!({"rank_query":"Japanese normalize JSON submatches byte column coordinates","operations":[{"search":{"patterns":["normalize"],"paths":["src"],"intent":"review"}}]})));
    let out = exchange(d.path(), req);
    let s = out[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        s.contains("fn normalize")
            && s.contains("fn emit_record")
            && s.contains("line.text.chars().count() + 1")
            && s.contains("fn decoded_coordinates")
            && s.contains("assert_eq!(value"),
        "{s}"
    );
}

#[test]
fn scoped_contract_retrieval_reduces_tokens_and_recovers_only_required_evidence() {
    let implementation = r#"pub const MAX_PAYLOAD_BYTES: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub authorized: bool,
    pub payload: &'a str,
}

pub struct DispatchState {
    pub available_slots: usize,
    pub fail_dispatch: bool,
    pub cleanup_calls: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DispatchError {
    Rejected,
    EmptyAfterNormalization,
    DispatchFailed,
}

struct Cleanup<'a> {
    calls: &'a mut usize,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        *self.calls += 1;
    }
}

fn normalize_payload(payload: &str) -> Result<String, DispatchError> {
    let normalized = payload.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(DispatchError::EmptyAfterNormalization);
    }
    Ok(normalized)
}

fn dispatch_normalized(payload: String, fail: bool) -> Result<String, DispatchError> {
    if fail {
        return Err(DispatchError::DispatchFailed);
    }
    Ok(payload)
}

pub fn authorize_and_dispatch(
    request: &Request<'_>,
    state: &mut DispatchState,
) -> Result<String, DispatchError> {
    if !request.authorized
        || state.available_slots == 0
        || request.payload.is_empty()
        || request.payload.len() > MAX_PAYLOAD_BYTES
    {
        return Err(DispatchError::Rejected);
    }

    let normalized = normalize_payload(request.payload)?;
    let fail = state.fail_dispatch;
    let _cleanup = Cleanup {
        calls: &mut state.cleanup_calls,
    };
    dispatch_normalized(normalized, fail)
}

"#;
    let unrelated_functions = r#"pub fn render_legacy_route(code: u32, label: &str) -> String {
    format!("{code}:{label}")
}

pub fn summarize_metrics(values: &[u64]) -> u64 {
    values.iter().copied().sum()
}

pub fn retry_dashboard_key(region: &str, service: &str) -> String {
    format!("{region}/{service}/retry")
}
"#;
    let tests = r#"use crate::{authorize_and_dispatch, DispatchError, DispatchState, Request};

#[test]
fn rejects_each_guard_boundary() {
    for (authorized, available_slots, payload) in [
        (false, 1, "JOB"),
        (true, 0, "JOB"),
        (true, 1, ""),
        (true, 1, "123456789"),
    ] {
        let request = Request { authorized, payload };
        let mut state = DispatchState {
            available_slots,
            fail_dispatch: false,
            cleanup_calls: 0,
        };
        assert_eq!(
            authorize_and_dispatch(&request, &mut state),
            Err(DispatchError::Rejected)
        );
        assert_eq!(state.cleanup_calls, 0);
    }
}

#[test]
fn accepts_exact_payload_limit_and_normalizes_before_dispatch() {
    let request = Request {
        authorized: true,
        payload: "  JOB-1 ",
    };
    let mut state = DispatchState {
        available_slots: 1,
        fail_dispatch: false,
        cleanup_calls: 0,
    };
    assert_eq!(
        authorize_and_dispatch(&request, &mut state),
        Ok("job-1".to_owned())
    );
    assert_eq!(state.cleanup_calls, 1);
}

#[test]
fn cleanup_runs_on_dispatch_error() {
    let request = Request {
        authorized: true,
        payload: "JOB-1",
    };
    let mut state = DispatchState {
        available_slots: 1,
        fail_dispatch: true,
        cleanup_calls: 0,
    };
    assert_eq!(
        authorize_and_dispatch(&request, &mut state),
        Err(DispatchError::DispatchFailed)
    );
    assert_eq!(state.cleanup_calls, 1);
}
"#;
    let unrelated_docs = format!(
        "# Routine dashboard notes\n\n{}",
        (0..24)
            .map(|n| format!(
                "Region {n:02}: monitor retry queues, cache refresh, scheduled exports, and weekly delivery totals.\n\n"
            ))
            .collect::<String>()
    );
    let broad = TempDir::new().unwrap();
    let targeted = TempDir::new().unwrap();
    for root in [broad.path(), targeted.path()] {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("src/dispatch.rs"), implementation).unwrap();
        fs::write(root.join("src/legacy.rs"), unrelated_functions).unwrap();
        fs::write(root.join("tests/dispatch.rs"), tests).unwrap();
        fs::write(root.join("docs/dashboard.md"), &unrelated_docs).unwrap();
    }

    let requirements = json!([
        {"id":"GUARD","description":"All authorization, capacity, and payload guard clauses","patterns":["if !request.authorized"]},
        {"id":"FLOW","description":"Normalization, dispatch, and cleanup dependency","patterns":["normalize_payload", "dispatch_normalized", "impl Drop for Cleanup"]},
        {"id":"BOUNDARIES","description":"Focused tests for rejected boundaries and cleanup","patterns":["rejects_each_guard_boundary", "accepts_exact_payload_limit_and_normalizes_before_dispatch", "cleanup_runs_on_dispatch_error"]}
    ]);
    let mut broad_requests = init();
    broad_requests.push(call(
        2,
        json!({
            "context_id":"contract-broad",
            "operations":[
                {"read":{"path":"src/dispatch.rs","start":1,"end":implementation.lines().count()}},
                {"read":{"path":"src/legacy.rs","start":1,"end":unrelated_functions.lines().count()}},
                {"read":{"path":"tests/dispatch.rs","start":1,"end":tests.lines().count()}},
                {"read":{"path":"docs/dashboard.md","start":1,"end":unrelated_docs.lines().count()}}
            ],
            "requirements":requirements.clone()
        }),
    ));
    let broad_out = exchange(broad.path(), broad_requests);
    let broad_text = broad_out[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();

    let guard_start = implementation
        .lines()
        .position(|line| line.contains("if !request.authorized"))
        .unwrap();
    let guard_end = implementation
        .lines()
        .position(|line| line.contains("let normalized = normalize_payload"))
        .unwrap();
    let guard_read_end = guard_end;
    let focused_operations = json!([
        {"search":{"intent":"review","paths":["src/dispatch.rs"],"patterns":["pub const MAX_PAYLOAD_BYTES: usize = 8;"]}},
        {"search":{"intent":"explain","paths":["src/dispatch.rs"],"patterns":["authorize_and_dispatch","normalize_payload","dispatch_normalized","Cleanup"]}},
        {"search":{"intent":"review","paths":["tests/dispatch.rs"],"patterns":["rejects_each_guard_boundary","accepts_exact_payload_limit_and_normalizes_before_dispatch","cleanup_runs_on_dispatch_error"]}}
    ]);
    let focused_request = json!({
        "context_id":"contract-focused",
        "operations":focused_operations.clone(),
        "requirements":requirements
    });
    let mut focused_requests = init();
    focused_requests.push(call(2, focused_request.clone()));
    focused_requests.push(call(
        3,
        json!({"context_id":"contract-focused","operations":focused_operations}),
    ));
    focused_requests.push(call(
        4,
        json!({
            "context_id":"contract-focused",
            "repeat":true,
            "operations":[{"read":{"path":"src/dispatch.rs","start":guard_start + 1,"end":guard_read_end}}],
            "requirements":[{"id":"GUARD","description":"Full authorization, capacity, and payload guard","patterns":["if !request.authorized"]}]
        }),
    ));
    let focused_out = exchange(targeted.path(), focused_requests);
    let focused_text = focused_out[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let suppressed_text = focused_out[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let recovered_guard = focused_out[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let compound_predicate = concat!(
        "51:    if !request.authorized\n",
        "52:        || state.available_slots == 0\n",
        "53:        || request.payload.is_empty()\n",
        "54:        || request.payload.len() > MAX_PAYLOAD_BYTES"
    );
    let normalization_empty_guard = concat!(
        "33:    let normalized = payload.trim().to_ascii_lowercase();\n",
        "34:    if normalized.is_empty() {\n",
        "35:        return Err(DispatchError::EmptyAfterNormalization);\n",
        "36:    }\n",
        "37:    Ok(normalized)\n",
        "38:}"
    );
    let dispatch_failure_body = concat!(
        "41:    if fail {\n",
        "42:        return Err(DispatchError::DispatchFailed);\n",
        "43:    }\n",
        "44:    Ok(payload)\n",
        "45:}"
    );

    let required_evidence = [
        "# item GUARD All authorization, capacity, and payload guard clauses state=matched",
        "# item FLOW Normalization, dispatch, and cleanup dependency state=matched",
        "# item BOUNDARIES Focused tests for rejected boundaries and cleanup state=matched",
        "pub const MAX_PAYLOAD_BYTES: usize = 8;",
        "return Err(DispatchError::Rejected);",
        "let normalized = normalize_payload(request.payload)?;",
        "payload.trim().to_ascii_lowercase()",
        "DispatchError::EmptyAfterNormalization",
        "Ok(normalized)",
        "fn dispatch_normalized(payload: String, fail: bool) -> Result<String, DispatchError>",
        "dispatch_normalized(normalized, fail)",
        "impl Drop for Cleanup<'_>",
        "fn drop(&mut self)",
        "*self.calls += 1;",
        "let _cleanup = Cleanup {",
        "calls: &mut state.cleanup_calls",
        "fn rejects_each_guard_boundary",
        "(false, 1, \"JOB\")",
        "(true, 0, \"JOB\")",
        "(true, 1, \"\")",
        "(true, 1, \"123456789\")",
        "Err(DispatchError::Rejected)",
        "state.cleanup_calls, 0",
        "fn accepts_exact_payload_limit_and_normalizes_before_dispatch",
        "payload: \"  JOB-1 \"",
        "Ok(\"job-1\".to_owned())",
        "state.cleanup_calls, 1",
        "fn cleanup_runs_on_dispatch_error",
        "fail_dispatch: true",
        "Err(DispatchError::DispatchFailed)",
    ];
    for required in required_evidence {
        assert!(
            focused_text.contains(required),
            "missing focused evidence {required:?}: {focused_text}"
        );
        assert!(
            broad_text.contains(required),
            "missing broad evidence {required:?}: {broad_text}"
        );
    }
    assert!(focused_text.contains(compound_predicate), "{focused_text}");
    assert!(
        focused_text.contains(normalization_empty_guard),
        "{focused_text}"
    );
    assert!(
        focused_text.contains(dispatch_failure_body),
        "{focused_text}"
    );
    assert!(broad_text.contains(compound_predicate), "{broad_text}");
    assert!(
        broad_text.contains(normalization_empty_guard),
        "{broad_text}"
    );
    assert!(broad_text.contains(dispatch_failure_body), "{broad_text}");
    assert!(
        focused_text.contains("conditions_incomplete=0"),
        "{focused_text}"
    );
    assert!(!focused_text.contains("state=unresolved"), "{focused_text}");
    assert!(focused_text.contains("pending=0"), "{focused_text}");
    assert!(!focused_text.contains("Region 00:"), "{focused_text}");
    assert!(
        !focused_text.contains("render_legacy_route"),
        "{focused_text}"
    );
    assert!(
        !focused_text.contains("retry_dashboard_key"),
        "{focused_text}"
    );

    let seen_count = suppressed_text
        .split_whitespace()
        .find_map(|field| field.strip_prefix("seen=")?.parse::<usize>().ok())
        .expect("normal repeat:false response should report seen rows");
    assert!(seen_count > 0, "{suppressed_text}");
    assert!(
        !suppressed_text.contains("if !request.authorized"),
        "{suppressed_text}"
    );
    assert!(
        !suppressed_text.contains("let normalized = normalize_payload"),
        "{suppressed_text}"
    );
    assert!(
        recovered_guard.contains("@ src/dispatch.rs:51-54"),
        "{recovered_guard}"
    );
    assert!(
        recovered_guard.contains(compound_predicate),
        "{recovered_guard}"
    );
    assert!(
        !recovered_guard.contains("render_legacy_route"),
        "{recovered_guard}"
    );

    let tokenizer = tiktoken_rs::o200k_base().unwrap();
    let broad_tokens = tokenizer.encode_ordinary(broad_text).len();
    let focused_tokens = [focused_text, suppressed_text, recovered_guard]
        .iter()
        .map(|text| tokenizer.encode_ordinary(text).len())
        .sum::<usize>();
    eprintln!("broad read: {broad_tokens} tokens; scoped review + seen suppression + narrow recovery: {focused_tokens} tokens");
    assert!(
        focused_tokens < broad_tokens,
        "focused={focused_tokens}, broad={broad_tokens}"
    );
}
