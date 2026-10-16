use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;
fn run(root: &Path, args: &[&str], input: &str) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}
fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
#[test]
fn default_batch_keeps_a_substantial_known_read_without_a_budget_round_trip() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        (1..=800)
            .map(|n| format!("let evidence_{n:04} = validate_condition_{n:04}();\n"))
            .collect::<String>(),
    )
    .unwrap();
    let request = r#"{"operations":[{"read":{"path":"x.rs","start":1,"end":800}}]}"#;
    for session in [false, true] {
        let mut args = vec!["--smart", "--batch", "-"];
        if session {
            args.extend(["--session", "state.json"]);
        }
        let out = run(d.path(), &args, request);
        let s = text(&out);
        assert!(
            out.status.success() && s.contains("800:let evidence_0800"),
            "{s}"
        );
        let n = tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len();
        assert!(n > 6000 && n <= 24000, "tokens={n}");
    }
    let limited = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "limited.json",
            "--batch",
            "-",
            "--max-tokens",
            "128",
        ],
        request,
    );
    assert!(limited.status.success());
    assert!(
        tiktoken_rs::o200k_base()
            .unwrap()
            .encode_ordinary(&text(&limited))
            .len()
            <= 128
    );
}
#[test]
fn critical_locate_hits_are_not_starved_by_a_wide_document_read() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("src")).unwrap();
    fs::write(
        d.path().join("README.md"),
        format!("{}\n", "noise_label ".repeat(55)),
    )
    .unwrap();
    fs::write(
        d.path().join("src/important.rs"),
        "TARGET validate_this_specific_guard_and_return_the_original_result();\n",
    )
    .unwrap();
    let request = r#"{"operations":[{"read":{"path":"README.md","start":1,"end":1}},{"search":["--regex","TARGET","src/important.rs"]}]}"#;
    for session in [false, true] {
        let mut args = vec!["--smart", "--batch", "-", "--max-tokens", "200"];
        if session {
            args.extend(["--session", "state.json"]);
        }
        let out = run(d.path(), &args, request);
        let s = text(&out);
        assert!(
            out.status.success() && s.contains("TARGET validate_this_specific_guard"),
            "{s}"
        );
    }
}
#[test]
fn adjacent_search_hits_share_a_single_original_range_header() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        (1..=80)
            .map(|n| format!("hit_{n:03}();\n"))
            .collect::<String>(),
    )
    .unwrap();
    for session in [false, true] {
        let mut args = vec!["--smart", "--batch", "-"];
        if session {
            args.extend(["--session", "state.json"]);
        }
        let out = run(
            d.path(),
            &args,
            r#"{"operations":[{"search":["--regex","hit","x.rs"]}]}"#,
        );
        let s = text(&out);
        assert!(out.status.success() && s.contains("80:hit_080"));
        assert_eq!(s.matches("@ x.rs:").count(), 1, "{s}");
        assert!(s.contains("@ x.rs:1-80"));
    }
}
#[test]
fn complete_session_evidence_is_packed_once_before_splitting_into_priority_units() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        "fn first() {\n    target();\n}\nfn second() {\n    other();\n}\n",
    )
    .unwrap();
    let out = run(
        d.path(),
        &["--smart", "--session", "state.json", "--batch", "-"],
        r#"{"operations":[{"search":["--regex","--intent","explain","target","x.rs"]},{"read":{"path":"x.rs","start":1,"end":6}}]}"#,
    );
    let s = text(&out);
    assert!(out.status.success() && s.contains("5:    other();"));
    assert_eq!(s.matches("@ x.rs:").count(), 1, "{s}");
    assert!(s.contains("pending=0"));
}
#[test]
fn default_smart_locate_has_room_for_a_useful_complete_code_preview() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        (1..=400)
            .map(|n| format!("target unique_evidence_{n:04} important_condition_{n:04}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let out = run(d.path(), &["--smart", "--regex", "target", "x.rs"], "");
    let s = text(&out);
    assert!(out.status.success() && s.contains("unique_evidence_0400"));
    let n = tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len();
    assert!(n > 2000 && n <= 8000, "tokens={n}");
}

#[test]
fn merging_adjacent_hits_never_discards_all_preview_rows_at_a_small_budget() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        (1..=200)
            .map(|n| format!("hit_{n:03}();\n"))
            .collect::<String>(),
    )
    .unwrap();
    let out = run(
        d.path(),
        &["--smart", "--batch", "-", "--max-tokens", "200"],
        r#"{"operations":[{"search":["--regex","hit","x.rs"]}]}"#,
    );
    let s = text(&out);
    assert!(out.status.success() && s.contains("1:hit_001"), "{s}");
    assert!(s.contains("pending=") && !s.contains("pending=0"));
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 200);
}

#[test]
fn sparse_hits_share_the_path_without_implying_that_gaps_were_returned() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        "target_first();\nunselected_gap();\ntarget_last();\n",
    )
    .unwrap();
    let out = run(
        d.path(),
        &["--smart", "--session", "state.json", "--batch", "-"],
        r#"{"operations":[{"search":["--regex","target","x.rs"]}]}"#,
    );
    let s = text(&out);
    assert!(
        out.status.success()
            && s.contains("1:target_first")
            && s.contains("3:target_last")
            && !s.contains("unselected_gap")
    );
    assert_eq!(s.matches("@ x.rs:").count(), 1, "{s}");
    assert!(s.contains("@ x.rs:1-1,3-3"), "{s}");
}

#[test]
fn broad_explain_keeps_every_hit_and_branch_without_expanding_all_function_bodies() {
    let d = TempDir::new().unwrap();
    let mut source = String::new();
    for i in 0..6 {
        source.push_str(&format!("fn action_{i}() {{\n"));
        for j in 0..70 {
            source.push_str(&format!("    unrelated_{i}_{j}();\n"));
        }
        source.push_str(&format!("    if enabled {{\n        generic_event_{i}();\n    }} else {{\n        reject_{i}();\n    }}\n}}\n"));
    }
    fs::write(d.path().join("x.rs"), source).unwrap();
    for session in [false, true] {
        let mut args = vec![
            "--smart",
            "--regex",
            "--intent",
            "explain",
            "generic_event",
            "x.rs",
        ];
        if session {
            args.extend(["--session", "state.json"]);
        }
        let out = run(d.path(), &args, "");
        let s = text(&out);
        assert!(out.status.success(), "{s}");
        for i in 0..6 {
            assert!(
                s.contains(&format!("fn action_{i}"))
                    && s.contains(&format!("generic_event_{i}"))
                    && s.contains(&format!("reject_{i}")),
                "{s}"
            );
        }
        assert!(!s.contains("unrelated_0_0"), "{s}");
        assert!(
            s.contains("scope_limited=6") && s.contains("scope_read x.rs:1-77"),
            "{s}"
        );
        assert!(s.contains("deferred=0"), "{s}");
        if session {
            assert!(s.contains("pending=0"), "{s}");
        }
    }
}

#[test]
fn full_scope_override_and_batch_inheritance_keep_requested_function_bodies() {
    let d = TempDir::new().unwrap();
    let source = (0..6)
        .map(|i| {
            format!(
                "fn action_{i}() {{\n{}    target_{i}();\n}}\n",
                "    unrelated();\n".repeat(70)
            )
        })
        .collect::<String>();
    fs::write(d.path().join("x.rs"), source).unwrap();
    let request = r#"{"operations":[{"search":["--regex","--intent","explain","target","x.rs"]}]}"#;
    for batch in [false, true] {
        let args = if batch {
            vec!["--smart", "--explain-scope", "full", "--batch", "-"]
        } else {
            vec![
                "--smart",
                "--regex",
                "--intent",
                "explain",
                "--explain-scope",
                "full",
                "target",
                "x.rs",
            ]
        };
        let out = run(d.path(), &args, if batch { request } else { "" });
        let s = text(&out);
        assert!(
            out.status.success() && s.matches("unrelated();").count() == 420,
            "{s}"
        );
        assert!(!s.contains("scope_limited="), "{s}");
    }
}

#[test]
fn scope_narrowing_is_visible_even_when_tiny_budget_drops_optional_notes() {
    let d = TempDir::new().unwrap();
    let source = (0..6)
        .map(|i| {
            format!(
                "fn action_{i}() {{\n{}    target_{i}();\n}}\n",
                "    unrelated();\n".repeat(70)
            )
        })
        .collect::<String>();
    fs::write(d.path().join("x.rs"), source).unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--regex",
            "--intent",
            "explain",
            "--max-tokens",
            "64",
            "target",
            "x.rs",
        ],
        "",
    );
    let s = text(&out);
    assert!(out.status.success() && s.contains("scope_limited=6"), "{s}");
    assert!(
        tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 64,
        "{s}"
    );
}

#[test]
fn dense_matches_and_explicit_context_do_not_gain_a_scope_limit() {
    let d = TempDir::new().unwrap();
    let source = (0..6)
        .map(|i| format!("fn action_{i}() {{\n{} }}\n", "    target();\n".repeat(70)))
        .collect::<String>();
    fs::write(d.path().join("x.rs"), source).unwrap();
    for context in [false, true] {
        let mut args = vec![
            "--smart", "--regex", "--intent", "explain", "target", "x.rs",
        ];
        if context {
            args.push("-C0");
        }
        let out = run(d.path(), &args, "");
        let s = text(&out);
        assert!(out.status.success(), "{s}");
        if context {
            assert!(
                s.contains("matched=420 shown=420 omitted=0 clipped=0"),
                "{s}"
            );
        } else {
            assert_eq!(s.matches("target();").count(), 420, "{s}");
        }
        assert!(!s.contains("scope_limited="), "{s}");
    }
}

#[test]
fn definition_search_keeps_definition_but_not_entire_callers() {
    let d = TempDir::new().unwrap();
    let source = format!(
        "def assert_lease(value):\n    if value is None:\n        return False\n    return True\n\ndef large_caller(value):\n{}    assert_lease(value)\n{}    return value\n",
        "    before_padding = 0\n".repeat(30),
        "    after_padding = 0\n".repeat(30)
    );
    fs::write(d.path().join("x.py"), source).unwrap();
    let auto = text(&run(
        d.path(),
        &["--smart", "--intent", "explain", "assert_lease", "x.py"],
        "",
    ));
    assert!(
        auto.contains("return False")
            && auto.contains("return True")
            && auto.contains("assert_lease(value)"),
        "{auto}"
    );
    assert!(
        !auto.contains("before_padding") || auto.matches("before_padding").count() <= 3,
        "{auto}"
    );
    assert!(auto.contains("scope_limited="), "{auto}");
    let full = text(&run(
        d.path(),
        &[
            "--smart",
            "--intent",
            "explain",
            "--explain-scope",
            "full",
            "assert_lease",
            "x.py",
        ],
        "",
    ));
    assert_eq!(full.matches("before_padding").count(), 30, "{full}");
}

#[test]
fn matched_definition_in_another_file_keeps_callers_short() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("definition.py"),
        "def assert_lease(value):\n    return value is not None\n",
    )
    .unwrap();
    fs::write(
        d.path().join("caller.py"),
        format!(
            "def caller(value):\n{}    assert_lease(value)\n",
            "    padding = 1\n".repeat(30)
        ),
    )
    .unwrap();
    let out = text(&run(
        d.path(),
        &["--smart", "--intent", "explain", "assert_lease", "."],
        "",
    ));
    assert!(
        out.contains("return value is not None") && out.contains("assert_lease(value)"),
        "{out}"
    );
    assert!(out.matches("padding = 1").count() <= 3, "{out}");
}
