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
    for request in requests {
        writeln!(input, "{request}").unwrap();
    }
    drop(input);
    let out = c.wait_with_output().unwrap();
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
