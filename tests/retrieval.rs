use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn run(root: &std::path::Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn explain_returns_the_whole_function_with_guards_and_else_beyond_three_lines() {
    let d = TempDir::new().unwrap();
    let source = "fn unrelated() { wrong(); }\npub fn receive(input: Input) {\n    // } a comment is not the end\n    let label = r#\"{ \\\" }\"#;\n    if input.stale {\n        reject();\n    } else {\n        let first = 1;\n        let second = 2;\n        let third = 3;\n        accept_target();\n    }\n    persist();\n}\nfn later() { other(); }\n";
    fs::write(d.path().join("x.rs"), source).unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--regex",
            "--intent",
            "explain",
            "accept_target",
            "x.rs",
        ],
        "",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(
        s.contains("pub fn receive")
            && s.contains("if input.stale")
            && s.contains("reject();")
            && s.contains("persist();"),
        "{s}"
    );
    assert!(!s.contains("unrelated") && !s.contains("fn later"), "{s}");
    assert!(s.contains("11:        accept_target();"), "{s}");
}

#[test]
fn python_and_typescript_function_bodies_keep_branch_results() {
    let d = TempDir::new().unwrap();
    for (name, source, needle, expected) in [
        (
            "x.py",
            "def stage(source):\n    if source:\n        first = 1\n        second = 2\n        third = 3\n        target_call()\n    else:\n        raise ValueError()\n    return 'done'\n\ndef other():\n    unrelated()\n",
            "target_call",
            "return 'done'",
        ),
        (
            "x.ts",
            "class Bridge {\n  private async cancel(\n    id: string\n  ): Promise<void> {\n    if (id) {\n      const s = `} {`;\n      const a = 1;\n      const b = 2;\n      target_call();\n    } else {\n      reject();\n    }\n    finish();\n  }\n  other() { unrelated(); }\n}\n",
            "target_call",
            "finish();",
        ),
    ] {
        fs::write(d.path().join(name), source).unwrap();
        let out = run(
            d.path(),
            &["--smart", "--regex", "--intent", "explain", needle, name],
            "",
        );
        assert!(out.status.success());
        let s = text(&out);
        assert!(s.contains(expected) && s.contains("else"), "{s}");
        assert!(!s.contains("unrelated"), "{s}");
    }
}

#[test]
fn explicit_context_and_stdin_keep_the_existing_line_contract() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "fn x() {\n  target();\n}\n").unwrap();
    let out = run(
        d.path(),
        &[
            "--smart", "--regex", "--intent", "explain", "-C0", "target", "x.rs",
        ],
        "",
    );
    assert_eq!(text(&out), "2:  target();\n");
    let out = run(
        d.path(),
        &["--smart", "--intent", "explain", "target", "-"],
        "before\ntarget\nafter\n",
    );
    assert!(text(&out).contains("before") && text(&out).contains("after"));
}

#[test]
fn batch_search_and_overlapping_reads_are_one_numbered_evidence_set() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"),"fn alpha() {\n  if enabled {\n    selected();\n  } else {\n    rejected();\n  }\n}\nfn beta() { other(); }\n").unwrap();
    let request = r#"{"operations":[{"search":["--regex","--intent","explain","selected","x.rs"]},{"read":{"path":"x.rs","start":2,"end":5}},{"read":{"path":"x.rs","start":4,"end":7}},{"search":["--regex","--intent","explain","missing","x.rs"]}]}"#;
    let out = run(d.path(), &["--smart", "--batch", "-"], request);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(
        s.contains("fn alpha") && s.contains("rejected();") && s.contains("no matches"),
        "{s}"
    );
    assert_eq!(s.matches("3:    selected();").count(), 1, "{s}");
    assert!(!s.contains("fn beta"), "{s}");
}

#[test]
fn batch_reports_errors_and_does_not_invoke_a_shell_or_hide_later_results() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "safe\nsecond\n").unwrap();
    let request = r#"{"operations":[{"read":{"path":"absent","start":1,"end":2}},{"read":{"path":"x.txt","start":1,"end":2}}]}"#;
    let out = run(d.path(), &["--smart", "--batch", "-"], request);
    assert_eq!(out.status.code(), Some(2));
    let s = text(&out);
    assert!(
        s.contains("error") && s.contains("1:safe") && s.contains("2:second"),
        "{s}"
    );
    for request in [
        r#"{"operations":[{"shell":"touch should-not-exist"}]}"#,
        r#"{"operations":[{"read":{"path":"x.txt","start":0,"end":2}}]}"#,
        r#"{"operations":[{"search":["--batch","-"]}]}"#,
    ] {
        assert_eq!(
            run(d.path(), &["--smart", "--batch", "-"], request)
                .status
                .code(),
            Some(2)
        );
    }
    assert!(!d.path().join("should-not-exist").exists());
}

#[test]
fn whole_ranges_are_deferred_with_locations_instead_of_cut_mid_branch() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        format!(
            "fn large() {{\n{}\n}}\n",
            (0..30)
                .map(|i| format!("    target_{i}();"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--regex",
            "--intent",
            "explain",
            "--max-tokens",
            "64",
            "target_0",
            "x.rs",
        ],
        "",
    );
    assert!(out.status.success());
    let s = text(&out);
    assert!(s.contains("deferred") && s.contains("x.rs:1-32"), "{s}");
    assert!(!s.contains("target_0();"), "{s}");
}

#[test]
fn large_functions_keep_the_signature_and_both_sides_of_the_local_branch() {
    let d = TempDir::new().unwrap();
    let padding = (0..300)
        .map(|i| format!("    let padding_{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(d.path().join("long.rs"),format!("fn long<'a>(value: &'a str) {{\n{padding}\n    if valid {{\n        target();\n    }} else {{\n        reject();\n    }}\n}}\n")).unwrap();
    let out = run(
        d.path(),
        &[
            "--smart", "--regex", "--intent", "explain", "target", "long.rs",
        ],
        "",
    );
    let s = text(&out);
    assert!(
        out.status.success()
            && s.contains("fn long<'a>")
            && s.contains("if valid")
            && s.contains("reject();"),
        "{s}"
    );
    assert!(!s.contains("padding_0"), "{s}");
}

#[test]
fn batch_deduplicates_file_aliases_and_preserves_global_budget() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "first\nsecond\nthird\n").unwrap();
    let input = r#"{"operations":[{"read":{"path":"x.txt","start":1,"end":2}},{"read":{"path":"./x.txt","start":2,"end":3}}]}"#;
    let out = run(d.path(), &["--smart", "--batch", "-"], input);
    let s = text(&out);
    assert_eq!(s.matches("2:second").count(), 1, "{s}");
    let tokenizer = tiktoken_rs::o200k_base().unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-", "--max-tokens", "64"],
        input,
    );
    assert!(tokenizer.encode_ordinary(&text(&out)).len() <= 64);
}

#[test]
fn batch_errors_remain_visible_even_with_a_tiny_output_budget() {
    let d = TempDir::new().unwrap();
    let ops = (0..64)
        .map(|i| serde_json::json!({"read":{"path":format!("missing_{i}"),"start":1,"end":2}}))
        .collect::<Vec<_>>();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-", "--max-tokens", "64"],
        &serde_json::json!({"operations":ops}).to_string(),
    );
    assert_eq!(out.status.code(), Some(2));
    let s = text(&out);
    assert!(s.contains("errors=64"), "{s}");
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 64);
}

#[test]
fn batch_read_ranges_can_extend_to_eof_without_a_second_query_for_line_count() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "one\ntwo\n").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-"],
        r#"{"operations":[{"read":{"path":"x.txt","start":1,"end":500}}]}"#,
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("@ x.txt:1-2") && text(&out).contains("2:two"));
}

#[test]
fn batch_search_without_paths_searches_the_directory_not_consumed_json_stdin() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "needle\n").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-"],
        r#"{"operations":[{"search":["needle"]}]}"#,
    );
    assert!(
        out.status.success() && text(&out).contains("1:needle"),
        "{}",
        text(&out)
    );
}

#[test]
fn bounded_capture_never_loses_its_incompleteness_notice() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.txt"),
        (0..5000).map(|i| format!("hit_{i}\n")).collect::<String>(),
    )
    .unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-", "--max-tokens", "64"],
        r#"{"operations":[{"search":["hit","x.txt"]}]}"#,
    );
    let s = text(&out);
    assert!(
        out.status.success() && s.contains("capture_omitted=904"),
        "{s}"
    );
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 64);
}

#[test]
fn a_wide_overlapping_read_does_not_starve_a_complete_small_function() {
    let d = TempDir::new().unwrap();
    let mut source="fn important() {\n    if valid {\n        target();\n    } else {\n        reject();\n    }\n}\n".to_owned();
    source.push_str(
        &(0..180)
            .map(|i| format!("// unrelated padding_{i} repeated to exceed the budget\n"))
            .collect::<String>(),
    );
    fs::write(d.path().join("x.rs"), source).unwrap();
    let request = r#"{"operations":[{"search":["--regex","--intent","explain","target","x.rs"]},{"read":{"path":"x.rs","start":1,"end":187}}]}"#;
    let out = run(
        d.path(),
        &["--smart", "--batch", "-", "--max-tokens", "200"],
        request,
    );
    let s = text(&out);
    assert!(
        out.status.success()
            && s.contains("fn important")
            && s.contains("reject();")
            && s.contains("deferred=1"),
        "{s}"
    );
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 200);
}

#[test]
fn invalid_search_arguments_do_not_discard_unrelated_valid_operations() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "valid evidence\n").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-"],
        r#"{"operations":[{"search":["--unsupported-flag","target","x.txt"]},{"read":{"path":"x.txt","start":1,"end":1}}]}"#,
    );
    assert_eq!(out.status.code(), Some(2));
    let s = text(&out);
    assert!(
        s.contains("1:valid evidence") && s.contains("errors=1"),
        "{s}"
    );
}
