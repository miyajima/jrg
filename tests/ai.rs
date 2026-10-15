use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
    sync::OnceLock,
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
    String::from_utf8(out.stdout.clone()).unwrap()
}
fn tokens(text: &str) -> usize {
    static TOKENIZER: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    TOKENIZER
        .get_or_init(|| tiktoken_rs::o200k_base().unwrap())
        .encode_ordinary(text)
        .len()
}

#[test]
fn duplicate_lines_share_one_path_and_keep_all_locations() {
    let dir = TempDir::new().unwrap();
    let out = run(dir.path(), &["--ai", "hit"], "hit\nhit\nother\nhit\n");
    assert!(out.status.success());
    assert_eq!(
        text(&out),
        "@ <stdin>\n1-2,4:1:hit\n# matched=3 shown=3 omitted=0 clipped=0\n"
    );
}

#[test]
fn long_japanese_lines_are_cropped_around_match_not_line_start() {
    let dir = TempDir::new().unwrap();
    let source = format!("{}ｶﾞｯｺｳ{}\n", "日本語".repeat(100), "後半".repeat(100));
    let out = run(
        dir.path(),
        &["--ai", "がっこう", "--snippet-chars", "32"],
        &source,
    );
    assert!(out.status.success());
    assert!(text(&out).contains("ｶﾞｯｺｳ"));
    assert!(text(&out).contains("…"));
    assert!(text(&out).contains("matched=1 shown=1 omitted=0 clipped=1"));
    assert!(tokens(&text(&out)) < tokens(&source));
}

#[test]
fn different_long_lines_are_not_misrepresented_as_duplicates() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "hit{}first\nhit{}second\n",
        "x".repeat(400),
        "x".repeat(400)
    );
    let out = run(dir.path(), &["--ai", "hit"], &source);
    let result = text(&out);
    assert!(result.contains("1:1:hit") && result.contains("2:1:hit"));
    assert!(!result.contains("1-2:1:hit"));
    assert!(result.contains("matched=2 shown=2 omitted=0 clipped=2"));
}

#[test]
fn token_budget_includes_headers_newlines_and_omission_footer() {
    let dir = TempDir::new().unwrap();
    let source: String = (0..100)
        .map(|n| format!("検索 hit unique_{n} code and extra documentation\n"))
        .collect();
    for budget in [64, 96, 128, 256] {
        let out = run(
            dir.path(),
            &["--ai", "hit", "--max-tokens", &budget.to_string()],
            &source,
        );
        assert!(out.status.success());
        assert!(tokens(&text(&out)) <= budget, "{}", text(&out));
        assert!(text(&out).contains("matched=100"));
        assert!(!text(&out).contains("omitted=0 "));
    }
}

#[test]
fn many_sparse_duplicate_references_can_be_partially_displayed() {
    let dir = TempDir::new().unwrap();
    let source = "hit\nother\n".repeat(200);
    let out = run(dir.path(), &["--ai", "hit", "--max-tokens", "64"], &source);
    assert!(tokens(&text(&out)) <= 64);
    assert!(text(&out).contains(":1:hit"));
    assert!(text(&out).contains("matched=200"));
    assert!(!text(&out).contains("shown=0"));
}

#[test]
fn output_is_deterministic_and_allocates_matches_across_files() {
    let dir = TempDir::new().unwrap();
    for name in ["a", "b", "c"] {
        let source: String = (0..100).map(|n| format!("hit {name} item_{n}\n")).collect();
        fs::write(dir.path().join(name), source).unwrap();
    }
    let args = ["--ai", "hit", ".", "-j4", "--max-tokens", "128"];
    let first = run(dir.path(), &args, "");
    let second = run(dir.path(), &args, "");
    assert_eq!(text(&first), text(&second));
    for name in ["a", "b", "c"] {
        assert!(text(&first).contains(&format!("@ ./{name}\n")));
    }
    assert!(tokens(&text(&first)) <= 128);
}

#[test]
fn context_is_labeled_and_does_not_change_match_accounting() {
    let dir = TempDir::new().unwrap();
    let out = run(dir.path(), &["--ai", "hit", "-C1"], "before\nhit\nafter\n");
    assert_eq!(
        text(&out),
        "@ <stdin>\n~1:1:before\n2:1:hit\n~3:1:after\n# matched=1 shown=1 omitted=0 clipped=0\n"
    );
}

#[test]
fn controls_and_tokenizer_special_strings_are_plain_source_text() {
    let dir = TempDir::new().unwrap();
    let out = run(
        dir.path(),
        &["--ai", "hit", "--color", "always"],
        "hit\x1b[31m<|endoftext|>\r\n",
    );
    assert!(out.status.success());
    assert!(!out.stdout.contains(&0x1b));
    assert!(text(&out).contains("<|endoftext|>"));
    assert!(tokens(&text(&out)) <= 2000);
}

#[test]
fn invalid_combinations_are_rejected_and_exit_codes_stay_meaningful() {
    let dir = TempDir::new().unwrap();
    for args in [
        vec!["--ai", "--json", "hit"],
        vec!["--max-tokens", "100", "hit"],
        vec!["--ai", "--max-tokens", "10", "hit"],
        vec!["--ai", "-q", "hit"],
    ] {
        assert_eq!(run(dir.path(), &args, "").status.code(), Some(2));
    }
    let out = run(dir.path(), &["--ai", "hit"], "other\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("matched=0 shown=0 omitted=0"));
    assert_eq!(
        run(dir.path(), &["--ai", "hit", "missing-file"], "")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn information_preserving_grouping_reduces_actual_token_count() {
    let dir = TempDir::new().unwrap();
    let path = "a_very_long_repeated_path_to_a_documentation_file_for_the_project.txt";
    fs::write(dir.path().join(path), "hit same evidence\n".repeat(200)).unwrap();
    let plain = run(dir.path(), &["hit", path], "");
    let ai = run(dir.path(), &["--ai", "hit", path], "");
    assert!(tokens(&text(&ai)) * 10 < tokens(&text(&plain)));
    assert!(text(&ai).contains("matched=200 shown=200 omitted=0 clipped=0"));
}
