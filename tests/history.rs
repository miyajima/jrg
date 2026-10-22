use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn invoke(root: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .env_remove("JRG_HISTORY")
        .env("JRG_STATE_DIR", state)
        .args(args)
        .output()
        .unwrap()
}
fn json(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn fixture() -> (TempDir, TempDir) {
    let root = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    std::fs::write(
        root.path().join("x.rs"),
        "fn alpha() {\n    println!(\"PRIVATE_BODY_123\");\n}\n",
    )
    .unwrap();
    (root, state)
}

#[test]
fn smart_search_records_once_without_persisting_source_text() {
    let (root, state) = fixture();
    let out = invoke(root.path(), state.path(), &["--smart", "alpha", "x.rs"]);
    assert!(out.status.success());
    let list = json(invoke(root.path(), state.path(), &["--history", "list"]));
    assert_eq!(list["requests"].as_array().unwrap().len(), 1);
    let id = list["requests"][0]["request_id"].as_str().unwrap();
    let detail = json(invoke(
        root.path(),
        state.path(),
        &["--history", "show", id],
    ));
    assert_eq!(detail["request"]["status"], "emitted");
    assert!(!detail["request"]["results"].as_array().unwrap().is_empty());
    let export = invoke(root.path(), state.path(), &["--history", "export"]);
    assert!(export.status.success());
    assert!(!String::from_utf8_lossy(&export.stdout).contains("PRIVATE_BODY_123"));
}

#[test]
fn ordinary_search_and_explicit_off_do_not_create_history() {
    let (root, state) = fixture();
    assert!(
        invoke(root.path(), state.path(), &["alpha", "x.rs"])
            .status
            .success()
    );
    assert!(
        invoke(
            root.path(),
            state.path(),
            &["--smart", "--history-mode", "off", "alpha", "x.rs"]
        )
        .status
        .success()
    );
    assert_eq!(std::fs::read_dir(state.path()).unwrap().count(), 0);
}

#[test]
fn smart_explain_records_a_no_match_without_a_feedback_target() {
    let (root, state) = fixture();
    let out = invoke(
        root.path(),
        state.path(),
        &["--smart", "--intent", "explain", "absent_symbol", "x.rs"],
    );
    assert_eq!(out.status.code(), Some(1));
    let record = last_request(root.path(), state.path());
    assert_eq!(record["status"], "emitted");
    assert_eq!(record["exit_code"], 1);
    assert_eq!(record["results"], serde_json::json!([]));
}

#[test]
fn review_internal_searches_are_one_request_and_feedback_is_idempotent() {
    let (root, state) = fixture();
    assert!(
        invoke(
            root.path(),
            state.path(),
            &["--smart", "--intent", "review", "alpha", "."]
        )
        .status
        .success()
    );
    let list = json(invoke(root.path(), state.path(), &["--history", "list"]));
    assert_eq!(list["requests"].as_array().unwrap().len(), 1);
    let id = list["requests"][0]["request_id"].as_str().unwrap();
    let detail = json(invoke(
        root.path(),
        state.path(),
        &["--history", "show", id],
    ));
    let result = detail["request"]["results"][0]["result_id"]
        .as_str()
        .unwrap();
    let args = [
        "--history",
        "feedback",
        id,
        result,
        "helpful",
        "--event-id",
        "repeatable-event",
    ];
    let a = json(invoke(root.path(), state.path(), &args));
    let b = json(invoke(root.path(), state.path(), &args));
    assert_eq!(a["event_id"], b["event_id"]);
    let conflict = invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            id,
            result,
            "not_helpful",
            "--event-id",
            "repeatable-event",
        ],
    );
    assert!(!conflict.status.success());
    let invalid = invoke(
        root.path(),
        state.path(),
        &["--history", "feedback", id, "not-returned", "helpful"],
    );
    assert!(!invalid.status.success());
}

fn last_request(root: &Path, state: &Path) -> Value {
    let list = json(invoke(root, state, &["--history", "list"]));
    let id = list["requests"][0]["request_id"].as_str().unwrap();
    json(invoke(root, state, &["--history", "show", id]))["request"].clone()
}

#[test]
fn feedback_rebinds_after_external_edit_but_not_body_change() {
    let (root, state) = fixture();
    let args = [
        "--smart",
        "--intent",
        "review",
        "--rank-query",
        "alpha",
        "alpha",
        ".",
    ];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let first = last_request(root.path(), state.path());
    json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            first["request_id"].as_str().unwrap(),
            first["results"][0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    let source = std::fs::read_to_string(root.path().join("x.rs")).unwrap();
    std::fs::write(root.path().join("x.rs"), format!("// unrelated\n{source}")).unwrap();
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let moved = last_request(root.path(), state.path());
    assert!(moved["history_applied"].as_u64().unwrap() > 0, "{moved}");
    assert_eq!(
        first["results"][0]["anchor_hash"],
        moved["results"][0]["anchor_hash"]
    );
    assert_ne!(first["results"][0]["start"], moved["results"][0]["start"]);
    std::fs::write(
        root.path().join("x.rs"),
        source.replace("PRIVATE_BODY_123", "changed"),
    )
    .unwrap();
    assert!(invoke(root.path(), state.path(), &args).status.success());
    assert_eq!(
        last_request(root.path(), state.path())["history_applied"],
        0
    );
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-c")
        .arg("commit.gpgsign=false")
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sibling_and_nested_worktrees_share_only_current_matching_source() {
    let (root, state) = fixture();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["add", "x.rs"]);
    git(root.path(), &["commit", "-qm", "fixture"]);
    let args = [
        "--smart",
        "--intent",
        "review",
        "--rank-query",
        "alpha",
        "alpha",
        ".",
    ];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let first = last_request(root.path(), state.path());
    json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            first["request_id"].as_str().unwrap(),
            first["results"][0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    let sibling = TempDir::new().unwrap();
    let path = sibling.path().join("checkout");
    git(
        root.path(),
        &["worktree", "add", "--detach", path.to_str().unwrap()],
    );
    assert!(invoke(&path, state.path(), &args).status.success());
    let shared = last_request(&path, state.path());
    assert_eq!(first["repo_id"], shared["repo_id"]);
    assert_ne!(first["worktree_id"], shared["worktree_id"]);
    assert!(shared["history_applied"].as_u64().unwrap() > 0);
    let nested = root.path().join("nested");
    git(
        root.path(),
        &["worktree", "add", "--detach", nested.to_str().unwrap()],
    );
    assert!(invoke(&nested, state.path(), &args).status.success());
    assert!(
        last_request(&nested, state.path())["history_applied"]
            .as_u64()
            .unwrap()
            > 0
    );
    std::fs::write(nested.join("x.rs"), "fn alpha() { different(); }\n").unwrap();
    assert!(invoke(&nested, state.path(), &args).status.success());
    assert_eq!(last_request(&nested, state.path())["history_applied"], 0);
}

#[test]
fn record_mode_keeps_evaluation_without_applying_it_and_disable_veto_wins() {
    let (root, state) = fixture();
    let args = [
        "--smart",
        "--history-mode",
        "record",
        "--intent",
        "review",
        "alpha",
        ".",
    ];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let first = last_request(root.path(), state.path());
    json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            first["request_id"].as_str().unwrap(),
            first["results"][0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    assert!(invoke(root.path(), state.path(), &args).status.success());
    assert_eq!(
        last_request(root.path(), state.path())["history_applied"],
        0
    );
    let count = json(invoke(root.path(), state.path(), &["--history", "list"]))["requests"]
        .as_array()
        .unwrap()
        .len();
    let out = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root.path())
        .env("JRG_STATE_DIR", state.path())
        .env("JRG_HISTORY", "off")
        .args(["--smart", "--history-mode", "auto", "alpha", "."])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        json(invoke(root.path(), state.path(), &["--history", "list"]))["requests"]
            .as_array()
            .unwrap()
            .len(),
        count
    );
}

#[test]
fn requirement_rows_record_literal_item_and_file_operation_links() {
    let (root, state) = fixture();
    std::fs::write(
        root.path().join("x.rs"),
        "fn alpha(a: bool, b: bool) {\n    if a\n        && b\n    { return; }\n}\n",
    )
    .unwrap();
    let plan = serde_json::json!({"operations":[{"read":{"path":"x.rs","start":2,"end":3}}],
        "requirements":[{"id":"guard","description":"guard clauses","patterns":["if a"]}]});
    std::fs::write(root.path().join("plan.json"), plan.to_string()).unwrap();
    let out = invoke(
        root.path(),
        state.path(),
        &["--smart", "--batch", "plan.json"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("3:        && b"), "{text}");
    let record = last_request(root.path(), state.path());
    let result = &record["results"][0];
    assert_eq!(result["requirement_ids"], serde_json::json!(["guard"]));
    assert_eq!(result["file_operation_ids"], serde_json::json!([1]));
    assert!(
        result["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0].as_u64().unwrap() <= 2 && r[1].as_u64().unwrap() > 2)
    );
    assert_eq!(result["partial"], true);
}

#[test]
fn batch_history_controls_belong_on_the_outer_request() {
    let (root, state) = fixture();
    for search in [
        serde_json::json!(["--history-mode", "off", "alpha", "x.rs"]),
        serde_json::json!(["--batch", "-"]),
        serde_json::json!(["--mcp", "--root", "."]),
        serde_json::json!(["--resume", "--session", "unused.json"]),
    ] {
        let plan = serde_json::json!({"operations":[
            {"search":search},
            {"read":{"path":"x.rs","start":1,"end":3}}
        ]});
        std::fs::write(root.path().join("plan.json"), plan.to_string()).unwrap();
        let out = invoke(
            root.path(),
            state.path(),
            &["--smart", "--batch", "plan.json"],
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "{search}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("outer"), "{text}");
        assert!(text.contains("PRIVATE_BODY_123"), "{text}");
    }
}

#[test]
fn mcp_returns_feedback_ids_and_forces_agent_provenance() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let (root, state) = fixture();
    git(root.path(), &["init", "-q"]);
    let mcp_root = root.path().join("sub");
    std::fs::create_dir(&mcp_root).unwrap();
    std::fs::rename(root.path().join("x.rs"), mcp_root.join("x.rs")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(&mcp_root)
        .env_remove("JRG_HISTORY")
        .env("JRG_STATE_DIR", state.path())
        .args(["--smart", "--mcp", "--root"])
        .arg(&mcp_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut exchange = |request: Value| -> Value {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    exchange(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
    // The ready notification does not return a response, so send it with the next request.
    let ready = serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"});
    let retrieve = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"retrieve","arguments":{
        "context_id":"same-task","operations":[{"search":{"patterns":["alpha"],"paths":["x.rs"],"intent":"review"}}]}}});
    let request = format!("{ready}\n{retrieve}");
    // Write both protocol messages directly before reading the retrieve response.
    writeln!(input, "{request}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    let meta = &reply["result"]["structuredContent"]["history"];
    assert_eq!(meta["results"][0]["path"], "x.rs", "{reply}");
    let request_id = meta["request_id"]
        .as_str()
        .unwrap_or_else(|| panic!("history request id missing: {reply}"));
    let result_id = meta["results"][0]["result_id"]
        .as_str()
        .expect("history result id");
    let feedback = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"feedback","arguments":{
        "request_id":request_id,"result_id":result_id,"event_id":"mcp-event","context_id":"same-task","label":"helpful"}}});
    writeln!(input, "{feedback}").unwrap();
    input.flush().unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    let feedback: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        feedback["result"]["structuredContent"]["source"], "agent",
        "{feedback}"
    );
    assert_eq!(
        feedback["result"]["structuredContent"]["context_id"],
        "same-task"
    );
    let invalid = serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"feedback","arguments":{
        "request_id":request_id,"result_id":result_id,"label":"helpful"}}});
    writeln!(input, "{invalid}").unwrap();
    input.flush().unwrap();
    line.clear();
    output.read_line(&mut line).unwrap();
    let invalid: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(invalid["result"]["isError"], true, "{invalid}");
    drop(input);
    assert!(child.wait_with_output().unwrap().status.success());
    let record = last_request(root.path(), state.path());
    assert_eq!(record["status"], "emitted");
}

#[test]
fn duplicate_scopes_and_different_conditions_do_not_reuse_feedback() {
    let (root, state) = fixture();
    let args = ["--smart", "--intent", "review", "alpha", "."];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let first = last_request(root.path(), state.path());
    json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            first["request_id"].as_str().unwrap(),
            first["results"][0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    assert!(
        invoke(
            root.path(),
            state.path(),
            &["--smart", "--intent", "review", "--hidden", "alpha", "."]
        )
        .status
        .success()
    );
    assert_eq!(
        last_request(root.path(), state.path())["history_applied"],
        0
    );
    let source = std::fs::read_to_string(root.path().join("x.rs")).unwrap();
    std::fs::write(root.path().join("x.rs"), format!("{source}\n{source}")).unwrap();
    assert!(invoke(root.path(), state.path(), &args).status.success());
    assert_eq!(
        last_request(root.path(), state.path())["history_applied"],
        0
    );
}

#[test]
fn history_failure_preserves_search_and_new_files_are_discovered() {
    let (root, state) = fixture();
    let args = ["--smart", "--intent", "review", "alpha", "."];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let first = last_request(root.path(), state.path());
    json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            first["request_id"].as_str().unwrap(),
            first["results"][0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    std::fs::write(
        root.path().join("new.rs"),
        "fn alpha_new() { unique_new_source(); }\n",
    )
    .unwrap();
    let out = invoke(root.path(), state.path(), &args);
    assert!(String::from_utf8_lossy(&out.stdout).contains("unique_new_source"));
    let invalid = state.path().join("file");
    std::fs::write(&invalid, "not a directory").unwrap();
    let out = invoke(root.path(), &invalid, &args);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("unique_new_source"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("history unavailable"));
}

#[test]
fn absolute_batch_paths_share_across_worktrees() {
    let (root, state) = fixture();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["add", "x.rs"]);
    git(root.path(), &["commit", "-qm", "fixture"]);
    let sibling = TempDir::new().unwrap();
    let checkout = sibling.path().join("checkout");
    git(
        root.path(),
        &["worktree", "add", "--detach", checkout.to_str().unwrap()],
    );
    for (index, dir) in [root.path(), checkout.as_path()].iter().enumerate() {
        let plan = serde_json::json!({"operations":[{"search":["--intent","review","alpha",dir.join("x.rs").to_str().unwrap()]}]});
        std::fs::write(dir.join("plan.json"), plan.to_string()).unwrap();
        assert!(
            invoke(dir, state.path(), &["--smart", "--batch", "plan.json"])
                .status
                .success()
        );
        let record = last_request(dir, state.path());
        if index == 0 {
            json(invoke(
                dir,
                state.path(),
                &[
                    "--history",
                    "feedback",
                    record["request_id"].as_str().unwrap(),
                    record["results"][0]["result_id"].as_str().unwrap(),
                    "helpful",
                ],
            ));
        } else {
            assert!(record["history_applied"].as_u64().unwrap() > 0, "{record}");
        }
    }
}

#[test]
fn standard_worktrees_share_history_without_a_git_executable() {
    let (root, state) = fixture();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["add", "x.rs"]);
    git(root.path(), &["commit", "-qm", "fixture"]);
    let args = [
        "--smart", "--engine", "native", "--intent", "review", "alpha", ".",
    ];
    assert!(invoke(root.path(), state.path(), &args).status.success());
    let original = last_request(root.path(), state.path());
    let sibling = TempDir::new().unwrap();
    let checkout = sibling.path().join("checkout");
    git(
        root.path(),
        &["worktree", "add", "--detach", checkout.to_str().unwrap()],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(&checkout)
        .env("PATH", "")
        .env_remove("JRG_HISTORY")
        .env("JRG_STATE_DIR", state.path())
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = json(invoke(&checkout, state.path(), &["--history", "list"]));
    assert_eq!(list["requests"].as_array().unwrap().len(), 2, "{list}");
    assert_eq!(
        original["repo_id"],
        last_request(&checkout, state.path())["repo_id"]
    );
}

#[test]
fn mcp_history_rejects_a_request_that_also_searched_outside_its_root() {
    use std::io::Write;
    use std::process::Stdio;
    let (root, state) = fixture();
    let sub = root.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::rename(root.path().join("x.rs"), sub.join("x.rs")).unwrap();
    std::fs::write(root.path().join("outside.rs"), "fn unrelated() {}\n").unwrap();
    git(root.path(), &["init", "-q"]);
    assert!(
        invoke(
            root.path(),
            state.path(),
            &[
                "--smart",
                "--intent",
                "review",
                "alpha",
                "sub/x.rs",
                "outside.rs"
            ]
        )
        .status
        .success()
    );
    let record = last_request(root.path(), state.path());
    assert_eq!(record["results"][0]["path"], "sub/x.rs");
    let messages = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"history","arguments":{"action":"show","request_id":record["request_id"]}}}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .env_remove("JRG_HISTORY")
        .env("JRG_STATE_DIR", state.path())
        .args(["--smart", "--mcp", "--root"])
        .arg(&sub)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for msg in messages {
        writeln!(input, "{msg}").unwrap();
    }
    drop(input);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let responses: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(responses[1]["result"]["isError"], true, "{}", responses[1]);
}

#[test]
fn history_regression_resume_keeps_mcp_alive_with_empty_and_pending_sessions() {
    use std::io::Write;
    use std::process::Stdio;
    let (root, state) = fixture();
    let source: String = (1..=100)
        .map(|n| format!("// pending_row_{n:03}\n"))
        .collect();
    std::fs::write(root.path().join("pending.txt"), source).unwrap();
    let messages = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"retrieve","arguments":{"operations":[],"resume":true,"history_mode":"auto"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"retrieve","arguments":{"operations":[{"read":{"path":"pending.txt","start":1,"end":100}}],"max_tokens":128}}}),
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"retrieve","arguments":{"operations":[],"resume":true,"max_tokens":8000,"history_mode":"auto"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"retrieve","arguments":{"operations":[{"read":{"path":"x.rs","start":1,"end":3}}]}}}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root.path())
        .env_remove("JRG_HISTORY")
        .env_remove("JRG_EMBEDDING_SOCKET")
        .env("JRG_STATE_DIR", state.path())
        .args(["--smart", "--mcp", "--root"])
        .arg(root.path())
        .args([
            "--session",
            state.path().join("seen.json").to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for message in messages {
        writeln!(input, "{message}").unwrap();
    }
    drop(input);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 5);
    for response in &responses[1..] {
        assert_eq!(response["result"]["isError"], false, "{response}");
    }
    assert!(
        responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("100:// pending_row_100"),
        "{}",
        responses[3]
    );
    assert!(
        responses[4]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("PRIVATE_BODY_123")
    );
    let list = json(invoke(root.path(), state.path(), &["--history", "list"]));
    assert_eq!(list["requests"].as_array().unwrap().len(), 4);
    let mut resumes = 0;
    for request in list["requests"].as_array().unwrap() {
        let record = json(invoke(
            root.path(),
            state.path(),
            &["--history", "show", request["request_id"].as_str().unwrap()],
        ));
        if record["request"]["conditions"]["resume"] == true {
            resumes += 1;
            assert_eq!(record["request"]["status"], "emitted");
            assert_eq!(record["request"]["history_applied"], 0);
            for result in record["request"]["results"].as_array().unwrap() {
                assert_eq!(result["file_operation_ids"], serde_json::json!([]));
            }
        }
    }
    assert_eq!(resumes, 2);
}

#[test]
fn history_regression_large_search_saves_bounded_feedback_targets_without_changing_source() {
    let (root, state) = fixture();
    let source: String = (1..=400)
        .map(|n| format!("needle unique_{n:04} 日本語\n"))
        .collect();
    std::fs::write(root.path().join("bulk.txt"), source).unwrap();
    let out = invoke(
        root.path(),
        state.path(),
        &[
            "--smart",
            "--engine",
            "native",
            "--max-tokens",
            "20000",
            "needle",
            "bulk.txt",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("history unavailable"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let off = invoke(
        root.path(),
        state.path(),
        &[
            "--smart",
            "--engine",
            "native",
            "--max-tokens",
            "20000",
            "--history-mode",
            "off",
            "needle",
            "bulk.txt",
        ],
    );
    assert!(off.status.success());
    let rendered = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        rendered.split("# history request=").next().unwrap(),
        String::from_utf8(off.stdout).unwrap()
    );
    let record = last_request(root.path(), state.path());
    assert_eq!(record["status"], "emitted");
    assert!(serde_json::to_vec(&record).unwrap().len() <= 64 * 1024);
    let kept = record["results"].as_array().unwrap();
    assert!(!kept.is_empty() && kept.len() < 400, "{record}");
    assert_eq!(record["metadata_omitted"]["results"], 400 - kept.len());
    for result in kept {
        assert_eq!(result["path"], "bulk.txt");
        assert!(result["end"].as_u64().unwrap() <= 400);
    }
    let accepted = json(invoke(
        root.path(),
        state.path(),
        &[
            "--history",
            "feedback",
            record["request_id"].as_str().unwrap(),
            kept[0]["result_id"].as_str().unwrap(),
            "helpful",
        ],
    ));
    assert_eq!(accepted["accepted"], true);
    let export = invoke(root.path(), state.path(), &["--history", "export"]);
    assert!(!String::from_utf8_lossy(&export.stdout).contains("needle unique_0001"));
}

#[test]
fn history_regression_mcp_only_exposes_persisted_ids_after_metadata_reduction() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    let (root, state) = fixture();
    let source: String = (1..=400)
        .map(|n| format!("needle unique_{n:04}\n"))
        .collect();
    std::fs::write(root.path().join("bulk.txt"), source).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root.path())
        .env_remove("JRG_HISTORY")
        .env_remove("JRG_EMBEDDING_SOCKET")
        .env("JRG_STATE_DIR", state.path())
        .args(["--smart", "--mcp", "--root"])
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut exchange = |message: Value| -> Value {
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response = serde_json::from_str(&line).unwrap();
        if message["method"] == "initialize" {
            writeln!(
                input,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .unwrap();
            input.flush().unwrap();
        }
        response
    };
    exchange(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
    let reply = exchange(
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"retrieve","arguments":{
        "operations":[{"read":{"path":"bulk.txt","start":1,"end":400}}],"max_tokens":20000}}}),
    );
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let meta = &reply["result"]["structuredContent"]["history"];
    let targets = meta["results"].as_array().expect("bounded history IDs");
    assert!(!targets.is_empty() && targets.len() < 400);
    assert_eq!(meta["metadata_omitted"]["results"], 400 - targets.len());
    let accepted = exchange(
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"feedback","arguments":{
        "request_id":meta["request_id"], "result_id":targets[0]["result_id"], "event_id":"bounded-mcp", "label":"helpful"}}}),
    );
    assert_eq!(
        accepted["result"]["structuredContent"]["accepted"], true,
        "{accepted}"
    );
    drop(input);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let record = json(invoke(
        root.path(),
        state.path(),
        &["--history", "show", meta["request_id"].as_str().unwrap()],
    ));
    let saved_ids: Vec<_> = record["request"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["result_id"])
        .collect();
    let advertised_ids: Vec<_> = targets.iter().map(|r| &r["result_id"]).collect();
    assert_eq!(advertised_ids, saved_ids);
    assert!(serde_json::to_vec(&record["request"]).unwrap().len() <= 64 * 1024);
}
