use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn run(root: &std::path::Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .env("JRG_HISTORY", "off")
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
fn ranking_changes_order_without_adding_or_dropping_matches() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "hit ordinary unrelated\n").unwrap();
    fs::write(d.path().join("z.txt"), "hit cancelAudio staleEffect\n").unwrap();
    fs::write(d.path().join("excluded.txt"), "cancelAudio staleEffect\n").unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "cancel audio stale effect",
            "hit",
            ".",
        ],
        "",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(s.find("z.txt").unwrap() < s.find("a.txt").unwrap(), "{s}");
    assert!(s.contains("1:hit ordinary unrelated") && s.contains("1:hit cancelAudio staleEffect"));
    assert!(!s.contains("excluded.txt"));
    assert!(s.contains("rank=bm25"), "{s}");
    let normal = run(d.path(), &["--smart", "hit", "."], "");
    assert!(!text(&normal).contains("rank=bm25"));
}

#[test]
fn rank_uses_context_and_keeps_original_unicode_and_line_numbers() {
    let d = TempDir::new().unwrap();
    let input = "hit unrelated\nother\nspacer\nhit ｶﾞｯｺｳ\n取消音声 API\n";
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "がっこう 取消音声 api",
            "-A1",
            "hit",
        ],
        input,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(
        s.find("4:hit ｶﾞｯｺｳ").unwrap() < s.find("1:hit unrelated").unwrap(),
        "{s}"
    );
    assert!(s.contains("5:取消音声 API") && s.contains("2:other"), "{s}");
}

#[test]
fn score_ties_are_deterministic_and_oov_keeps_candidates() {
    let d = TempDir::new().unwrap();
    for path in ["z.txt", "a.txt"] {
        fs::write(d.path().join(path), "hit same\n").unwrap();
    }
    for query in ["same", "unseen"] {
        let mut first = None;
        for _ in 0..3 {
            let out = run(
                d.path(),
                &["--smart", "--rank-query", query, "hit", "."],
                "",
            );
            assert!(out.status.success());
            let s = text(&out);
            assert!(s.find("a.txt").unwrap() < s.find("z.txt").unwrap(), "{s}");
            if let Some(first) = &first {
                assert_eq!(first, &s);
            } else {
                first = Some(s);
            }
        }
    }
}

#[test]
fn ranked_budget_prefers_relevant_candidate_and_session_can_recover_the_rest() {
    let d = TempDir::new().unwrap();
    for n in 0..20 {
        fs::write(
            d.path().join(format!("a{n:02}.txt")),
            format!("hit ordinary unrelated example number {n}\n"),
        )
        .unwrap();
    }
    fs::write(d.path().join("z.txt"), "hit cancelAudio staleEffect\n").unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "cancel audio stale effect",
            "--max-tokens",
            "128",
            "--session",
            "state.json",
            "hit",
            ".",
        ],
        "",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(s.contains("1:hit cancelAudio staleEffect"), "{s}");
    let tok = tiktoken_rs::o200k_base().unwrap();
    assert!(tok.encode_ordinary(&s).len() <= 128, "{s}");
    let resumed = run(
        d.path(),
        &["--smart", "--session", "state.json", "--resume"],
        "",
    );
    assert!(resumed.status.success());
    assert!(text(&resumed).contains("ordinary unrelated"));
    assert!(!text(&resumed).contains("cancelAudio"));
    assert!(text(&resumed).contains("pending=0"));
}

#[test]
fn batch_applies_one_outer_ranking_query_to_all_candidates() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "hit ordinary\n").unwrap();
    fs::write(d.path().join("z.txt"), "hit important\n").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--rank-query", "important", "--batch", "-"],
        r#"{"operations":[{"search":["hit","a.txt"]},{"search":["hit","z.txt"]}]}"#,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = text(&out);
    assert!(s.find("z.txt").unwrap() < s.find("a.txt").unwrap(), "{s}");
}

#[test]
fn ranking_rejects_empty_query_and_requires_smart_but_preserves_no_match_exit() {
    let d = TempDir::new().unwrap();
    for args in [
        vec!["--rank-query", "word", "hit"],
        vec!["--smart", "--rank-query", "  ", "hit"],
        vec!["--smart", "--rank-query", "!!!", "hit"],
    ] {
        assert_eq!(run(d.path(), &args, "hit\n").status.code(), Some(2));
    }
    let out = run(
        d.path(),
        &["--smart", "--rank-query", "word", "missing"],
        "other\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(!text(&out).contains("1:other"));
}

#[test]
fn partial_passage_keeps_the_query_evidence_instead_of_only_the_literal_hit() {
    let d = TempDir::new().unwrap();
    let input = format!(
        "hit ordinary\n{}cancelAudio staleEffect\n",
        "irrelevant filler material\n".repeat(30)
    );
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "cancel audio stale effect",
            "-A40",
            "--max-tokens",
            "128",
            "hit",
        ],
        &input,
    );
    assert!(out.status.success());
    let s = text(&out);
    assert!(s.contains("32:cancelAudio staleEffect"), "{s}");
    assert!(s.contains("partial=1"), "{s}");
    assert!(tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 128);
}

#[test]
fn rank_preserves_globs_ignore_rules_and_the_minimum_output_budget() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "hit rare\n").unwrap();
    fs::write(d.path().join("hidden.txt"), "hit rare rare\n").unwrap();
    fs::write(d.path().join(".gitignore"), "hidden.txt\n").unwrap();
    fs::write(d.path().join("b.log"), "hit rare rare\n").unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "rare",
            "-g",
            "*.txt",
            "--max-tokens",
            "64",
            "hit",
            ".",
        ],
        "",
    );
    assert!(out.status.success());
    let s = text(&out);
    assert!(!s.contains("hidden.txt") && !s.contains("b.log"), "{s}");
    assert!(
        tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= 64,
        "{s}"
    );
}

#[test]
fn ranking_keeps_explicit_function_scopes_and_reports_nested_query_misuse() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("x.rs"),
        "fn cancel_audio() {\n    if stale {\n        target();\n    }\n    finish();\n}\n",
    )
    .unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "cancel audio",
            "--intent",
            "explain",
            "target",
            "x.rs",
        ],
        "",
    );
    assert!(out.status.success());
    assert!(text(&out).contains("finish();") && text(&out).contains("fn cancel_audio"));
    let out = run(
        d.path(),
        &["--smart", "--batch", "-"],
        r#"{"operations":[{"search":["--rank-query","cancel","target","x.rs"]}]}"#,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out).contains("--rank-query belongs on the outer command"));
}

#[test]
fn discovery_limit_cli_preserves_exact_reads_and_omissions_can_be_read_without_repeat() {
    let d = TempDir::new().unwrap();
    for n in 0..8 {
        fs::write(
            d.path().join(format!("a{n}.txt")),
            format!("hit ordinary {n}\n"),
        )
        .unwrap();
    }
    fs::write(d.path().join("z.txt"), "hit cancellation\n").unwrap();
    fs::write(d.path().join("needed.txt"), "known body\n".repeat(80)).unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--session",
            "state.json",
            "--rank-query",
            "cancellation",
            "--discovery-limit",
            "1",
            "--batch",
            "-",
        ],
        r#"{"operations":[{"search":["hit","."]},{"read":{"path":"needed.txt","start":1,"end":80}}]}"#,
    );
    assert!(out.status.success(), "{}", text(&out));
    let s = text(&out);
    assert_eq!(
        s.lines().filter(|l| l.starts_with("1:hit ")).count(),
        1,
        "{s}"
    );
    assert!(
        s.contains("1:hit cancellation") && s.contains("80:known body") && s.contains("pending=0"),
        "{s}"
    );
    let resume = run(
        d.path(),
        &["--smart", "--session", "state.json", "--resume"],
        "",
    );
    assert!(!text(&resume).contains("ordinary"), "{}", text(&resume));
    let read = run(
        d.path(),
        &["--smart", "--session", "state.json", "--batch", "-"],
        r#"{"operations":[{"read":{"path":"a7.txt","start":1,"end":1}}]}"#,
    );
    assert!(text(&read).contains("1:hit ordinary 7"), "{}", text(&read));
}

#[test]
fn discovery_has_stable_ties_and_budget_counters_survive_small_budget() {
    let d = TempDir::new().unwrap();
    for n in 0..14 {
        fs::write(d.path().join(format!("a{n:02}.txt")), "hit same\n").unwrap();
    }
    let out = run(
        d.path(),
        &[
            "--smart",
            "--rank-query",
            "unseen",
            "--discovery-limit",
            "1",
            "--max-tokens",
            "256",
            "hit",
            ".",
        ],
        "",
    );
    assert!(out.status.success());
    let s = text(&out);
    assert!(
        s.contains("@ ./a00.txt:1-1") && !s.contains("@ ./a01.txt:"),
        "{s}"
    );
    assert!(
        s.contains("discovery_selected=1 discovery_omitted=13") && s.contains("pending=0"),
        "{s}"
    );
    for budget in ["64", "128"] {
        let out = run(
            d.path(),
            &[
                "--smart",
                "--rank-query",
                "same",
                "--discovery-limit",
                "1",
                "--max-tokens",
                budget,
                "hit",
                ".",
            ],
            "",
        );
        let s = text(&out);
        assert!(out.status.success());
        assert!(
            tiktoken_rs::o200k_base().unwrap().encode_ordinary(&s).len() <= budget.parse().unwrap(),
            "{s}"
        );
        assert!(s.contains("discovery_omitted=13"), "{s}");
    }
}

#[test]
fn discovery_limit_requires_rank_query_and_rejects_out_of_range() {
    let d = TempDir::new().unwrap();
    for args in [
        vec!["--smart", "--discovery-limit", "2", "hit"],
        vec![
            "--smart",
            "--rank-query",
            "hit",
            "--discovery-limit",
            "257",
            "hit",
        ],
    ] {
        assert_eq!(run(d.path(), &args, "hit\n").status.code(), Some(2));
    }
}
