use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;
fn run(root: &Path, args: &[&str], input: &str) -> Output {
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
fn read(start: usize, end: usize) -> String {
    serde_json::json!({"operations":[{"read":{"path":"x.rs","start":start,"end":end}}]}).to_string()
}
#[test]
fn delivered_rows_are_shared_across_calls_and_path_aliases() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "first\nsecond\nthird\n").unwrap();
    let args = ["--smart", "--session", "state.json", "--batch", "-"];
    let first = run(d.path(), &args, &read(1, 2));
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run(
        d.path(),
        &args,
        r#"{"operations":[{"read":{"path":"./x.rs","start":2,"end":3}}]}"#,
    );
    assert!(second.status.success());
    let s = text(&second);
    assert!(!s.contains("2:second"), "{s}");
    assert!(s.contains("3:third") && s.contains("seen="), "{s}");
    let repeat = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--repeat",
            "--batch",
            "-",
        ],
        &read(1, 2),
    );
    assert!(repeat.status.success() && text(&repeat).contains("2:second"));
}
#[test]
fn final_budget_packs_partial_ranges_and_resume_returns_only_missing_rows() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        (1..=60)
            .map(|n| format!("evidence_row_{n:03}();\n"))
            .collect::<String>(),
    )
    .unwrap();
    let args = [
        "--smart",
        "--session",
        "state.json",
        "--batch",
        "-",
        "--max-tokens",
        "180",
    ];
    let first = run(d.path(), &args, &read(1, 60));
    let mut outputs = vec![text(&first)];
    assert!(
        first.status.success() && outputs[0].contains("1:evidence_row_001"),
        "{}",
        outputs[0]
    );
    let tokenizer = tiktoken_rs::o200k_base().unwrap();
    for _ in 0..30 {
        if outputs.last().unwrap().contains("pending=0") {
            break;
        }
        let out = run(
            d.path(),
            &[
                "--smart",
                "--session",
                "state.json",
                "--resume",
                "--max-tokens",
                "180",
            ],
            "",
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        outputs.push(text(&out));
    }
    assert!(outputs.last().unwrap().contains("pending=0"));
    for s in &outputs {
        assert!(tokenizer.encode_ordinary(s).len() <= 180, "{s}");
    }
    let all = outputs.join("");
    for n in 1..=60 {
        assert_eq!(
            all.matches(&format!("{n}:evidence_row_{n:03}();")).count(),
            1,
            "row {n}"
        );
    }
}
#[test]
fn content_changes_invalidate_seen_rows_and_stale_resume_ranges() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "old\n").unwrap();
    let args = ["--smart", "--session", "state.json", "--batch", "-"];
    assert!(run(d.path(), &args, &read(1, 1)).status.success());
    fs::write(d.path().join("x.rs"), "new\n").unwrap();
    let fresh = run(d.path(), &args, &read(1, 1));
    assert!(fresh.status.success() && text(&fresh).contains("1:new"));
    fs::write(
        d.path().join("x.rs"),
        (1..=80)
            .map(|n| format!("old_long_evidence_{n}();\n"))
            .collect::<String>(),
    )
    .unwrap();
    let limited = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--batch",
            "-",
            "--max-tokens",
            "100",
        ],
        &read(1, 80),
    );
    assert!(limited.status.success());
    fs::write(d.path().join("x.rs"), "different_file\n").unwrap();
    let resume = run(
        d.path(),
        &["--smart", "--session", "state.json", "--resume"],
        "",
    );
    assert!(resume.status.success());
    let s = text(&resume);
    assert!(
        s.contains("changed") && !s.contains("1:different_file"),
        "{s}"
    );
}
#[test]
fn standalone_search_uses_the_same_session_and_corruption_is_not_silently_reset() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "fn test() {\n    target();\n}\n").unwrap();
    let args = [
        "--smart",
        "--session",
        "state.json",
        "--regex",
        "--intent",
        "explain",
        "target",
        "x.rs",
    ];
    let one = run(d.path(), &args, "");
    assert!(one.status.success() && text(&one).contains("target();"));
    let two = run(d.path(), &args, "");
    assert!(two.status.success() && !text(&two).contains("target();"));
    fs::write(d.path().join("state.json"), "corrupt").unwrap();
    assert_eq!(run(d.path(), &args, "").status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(d.path().join("state.json")).unwrap(),
        "corrupt"
    );
}
#[test]
fn locked_or_cross_workspace_sessions_are_rejected_without_output_suppression() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.rs"), "target\n").unwrap();
    let state = d.path().join("state.json");
    let args = [
        "--smart",
        "--session",
        state.to_str().unwrap(),
        "--batch",
        "-",
    ];
    assert!(run(d.path(), &args, &read(1, 1)).status.success());
    let other = TempDir::new().unwrap();
    fs::write(other.path().join("x.rs"), "target\n").unwrap();
    assert_eq!(run(other.path(), &args, &read(1, 1)).status.code(), Some(2));
    fs::write(d.path().join("state.json.lock"), "busy").unwrap();
    assert_eq!(run(d.path(), &args, &read(1, 1)).status.code(), Some(2));
}

#[test]
fn a_single_oversized_line_stays_pending_until_a_larger_budget_and_errors_remain_visible() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        format!("{}\n", "long_original_evidence ".repeat(100)),
    )
    .unwrap();
    let first = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--batch",
            "-",
            "--max-tokens",
            "64",
        ],
        &read(1, 1),
    );
    assert!(first.status.success() && text(&first).contains("pending=1"));
    assert!(
        tiktoken_rs::o200k_base()
            .unwrap()
            .encode_ordinary(&text(&first))
            .len()
            <= 64
    );
    let resumed = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--resume",
            "--max-tokens",
            "2000",
        ],
        "",
    );
    assert!(
        resumed.status.success()
            && text(&resumed).contains("1:long_original_evidence")
            && text(&resumed).contains("pending=0")
    );
    fs::write(
        d.path().join("x.rs"),
        (0..60).map(|n| format!("row_{n}();\n")).collect::<String>(),
    )
    .unwrap();
    assert!(
        run(
            d.path(),
            &[
                "--smart",
                "--session",
                "state.json",
                "--batch",
                "-",
                "--max-tokens",
                "64"
            ],
            &read(1, 60)
        )
        .status
        .success()
    );
    fs::remove_file(d.path().join("x.rs")).unwrap();
    let failed = run(
        d.path(),
        &["--smart", "--session", "state.json", "--resume"],
        "",
    );
    assert_eq!(failed.status.code(), Some(2));
    assert!(text(&failed).contains("errors=1"));
}

#[test]
fn an_oversized_function_prioritizes_the_matching_guard_in_the_final_packing() {
    let d = TempDir::new().unwrap();
    let padding = (0..80)
        .map(|n| format!("    let unrelated_{n} = {n};\n"))
        .collect::<String>();
    fs::write(d.path().join("x.rs"),format!("fn large() {{\n{padding}    if allowed {{\n        target();\n    }} else {{\n        reject();\n    }}\n}}\n")).unwrap();
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
            "200",
            "target",
            "x.rs",
        ],
        "",
    );
    let s = text(&out);
    assert!(
        out.status.success()
            && s.contains("target();")
            && s.contains("if allowed")
            && s.contains("reject();"),
        "{s}"
    );
    assert!(!s.contains("pending=0"), "{s}");
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 200);
}
