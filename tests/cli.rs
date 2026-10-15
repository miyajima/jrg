use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

#[test]
fn japanese_variants_preserve_original_text() {
    let dir = TempDir::new().unwrap();
    let source = "がっこう\nガッコウ\nｶﾞｯｺｳ\nか\u{3099}っこう\n学校\n";
    let output = run(dir.path(), &["がっこう"], source.as_bytes());
    assert_eq!(code(&output), 0);
    assert_eq!(
        text(&output),
        "<stdin>:1:がっこう\n<stdin>:2:ガッコウ\n<stdin>:3:ｶﾞｯｺｳ\n<stdin>:4:か\u{3099}っこう\n"
    );
    assert_eq!(
        text(&run(
            dir.path(),
            &["--no-normalize", "がっこう"],
            source.as_bytes()
        )),
        "<stdin>:1:がっこう\n"
    );
    assert_eq!(
        text(&run(
            dir.path(),
            &["--no-kana", "ガッコウ"],
            source.as_bytes()
        )),
        "<stdin>:2:ガッコウ\n<stdin>:3:ｶﾞｯｺｳ\n"
    );
}

#[test]
fn normalization_is_symmetric_and_works_with_case_options() {
    let dir = TempDir::new().unwrap();
    let source = "ＡＰＩ\napi\nAPI\n";
    assert_eq!(
        text(&run(dir.path(), &["-i", "ＡＰＩ"], source.as_bytes()))
            .lines()
            .count(),
        3
    );
    assert_eq!(
        text(&run(dir.path(), &["-S", "api"], source.as_bytes()))
            .lines()
            .count(),
        3
    );
    assert_eq!(
        text(&run(dir.path(), &["-S", "ＡＰＩ"], source.as_bytes()))
            .lines()
            .count(),
        2
    );
    assert_eq!(
        text(&run(dir.path(), &["ｶﾞ", "-o"], "ガがｶﾞ".as_bytes())),
        "<stdin>:1:ガ\n<stdin>:1:が\n<stdin>:1:ｶﾞ\n"
    );
}

#[test]
fn fixed_strings_and_raw_regex_have_distinct_semantics() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        text(&run(dir.path(), &["a.b"], b"axb\na.b\n")),
        "<stdin>:2:a.b\n"
    );
    assert_eq!(
        text(&run(dir.path(), &["--regex", "a.b"], b"axb\na.b\n"))
            .lines()
            .count(),
        2
    );
    assert_eq!(
        text(&run(
            dir.path(),
            &["--regex", "ガ(ク|ッ)"],
            "ガク\nがく\n".as_bytes()
        )),
        "<stdin>:1:ガク\n"
    );
    assert_eq!(code(&run(dir.path(), &["--regex", "["], b"")), 2);
    assert_eq!(code(&run(dir.path(), &["--regex", "-F", "a"], b"")), 2);
}

#[test]
fn repeated_patterns_and_positional_paths() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "ねこ\nイヌ\nとり\n").unwrap();
    fs::write(dir.path().join("b.txt"), "ネコ\n").unwrap();
    let output = run(
        dir.path(),
        &["-e", "猫", "-e", "いぬ", "a.txt", "b.txt"],
        b"",
    );
    assert_eq!(text(&output), "a.txt:2:イヌ\n");
    let output = run(
        dir.path(),
        &["-e", "ねこ", "-e", "いぬ", "a.txt", "b.txt", "-j1"],
        b"",
    );
    assert_eq!(text(&output).lines().count(), 3);
    assert_eq!(
        text(&run(dir.path(), &["-e", "いぬ"], "イヌ\n".as_bytes())),
        "<stdin>:1:イヌ\n"
    );
}

#[test]
fn recursive_ignore_hidden_globs_and_nested_negation() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    for name in [
        "keep.md",
        "skip.md",
        ".hidden.md",
        "docs/nested.md",
        "keep.txt",
    ] {
        fs::write(dir.path().join(name), "検索\n").unwrap();
    }
    fs::write(
        dir.path().join(".gitignore"),
        "skip.md\ndocs/*\n!docs/nested.md\n",
    )
    .unwrap();
    let output = run(dir.path(), &["検索", ".", "-l"], b"");
    assert_eq!(code(&output), 0);
    let result = text(&output);
    assert_eq!(result.lines().count(), 3, "{result}");
    assert!(result.contains("keep.md") && result.contains("nested.md"));
    assert!(!result.contains("skip.md") && !result.contains(".hidden.md"));
    // Explicit inclusion globs override ignore/hidden rules, as in rg.
    let output = run(dir.path(), &["検索", ".", "-g", "*.md", "-l"], b"");
    assert_eq!(text(&output).lines().count(), 4);
    assert!(!text(&output).contains("keep.txt"));
    assert_eq!(
        text(&run(
            dir.path(),
            &["検索", ".", "-l", "--hidden", "--no-ignore"],
            b""
        ))
        .lines()
        .count(),
        5
    );
    let output = run(dir.path(), &["検索", ".", "-l", "-g", "!*.txt"], b"");
    assert!(!text(&output).contains("keep.txt"));
}

#[test]
fn json_offsets_and_columns_reference_original_decoded_text() {
    let dir = TempDir::new().unwrap();
    let output = run(
        dir.path(),
        &["--json", "-e", "が", "-e", "株式会社"],
        "前ｶﾞ㍿\n".as_bytes(),
    );
    let value: serde_json::Value = serde_json::from_str(text(&output).trim()).unwrap();
    assert_eq!(value["text"], "前ｶﾞ㍿");
    assert_eq!(
        value["submatches"][0],
        serde_json::json!({"start":3,"end":9,"column":2,"text":"ｶﾞ"})
    );
    assert_eq!(
        value["submatches"][1],
        serde_json::json!({"start":9,"end":12,"column":4,"text":"㍿"})
    );
    let output = run(dir.path(), &["-o", "--column", "社"], "前㍿\n".as_bytes());
    assert_eq!(text(&output), "<stdin>:1:2:㍿\n");
    let output = run(
        dir.path(),
        &["-o", "--regex", "."],
        "か\u{3099}\n".as_bytes(),
    );
    assert_eq!(text(&output), "<stdin>:1:か\n<stdin>:1:\u{3099}\n");
}

#[test]
fn compatibility_expansion_does_not_duplicate_original_glyph() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        text(&run(
            dir.path(),
            &["-o", "-e", "株", "-e", "社"],
            "㍿\n".as_bytes()
        )),
        "<stdin>:1:㍿\n"
    );
}

#[test]
fn context_merges_without_duplicates_and_supports_max_count() {
    let dir = TempDir::new().unwrap();
    let source = b"before\nhit\nmiddle\nhit\nafter\nend\n";
    assert_eq!(
        text(&run(dir.path(), &["hit", "-C1"], source)),
        "<stdin>-1-before\n<stdin>:2:hit\n<stdin>-3-middle\n<stdin>:4:hit\n<stdin>-5-after\n"
    );
    assert_eq!(
        text(&run(dir.path(), &["hit", "-B1", "-A2", "-m1"], source)),
        "<stdin>-1-before\n<stdin>:2:hit\n<stdin>-3-middle\n<stdin>-4-hit\n"
    );
    assert_eq!(
        text(&run(dir.path(), &["hit", "-c", "-m1"], source)),
        "<stdin>:1\n"
    );
}

#[test]
fn count_files_quiet_invert_and_exit_codes() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        text(&run(dir.path(), &["hit", "-c"], b"hit hit\nhit\nno\n")),
        "<stdin>:2\n"
    );
    assert_eq!(
        text(&run(dir.path(), &["hit", "-l"], b"hit\nhit\n")),
        "<stdin>\n"
    );
    let output = run(dir.path(), &["hit", "-q"], b"hit\n");
    assert_eq!(code(&output), 0);
    assert!(output.stdout.is_empty());
    assert_eq!(
        text(&run(dir.path(), &["hit", "-v"], b"hit\nno\n")),
        "<stdin>:2:no\n"
    );
    assert_eq!(code(&run(dir.path(), &["missing"], b"hit\n")), 1);
    assert_eq!(code(&run(dir.path(), &["x", "missing-file"], b"")), 2);
    assert_eq!(code(&run(dir.path(), &["x", "-m0"], b"")), 2);
    assert_eq!(
        text(&run(dir.path(), &["hit", "--no-line-number"], b"hit\n")),
        "<stdin>:hit\n"
    );
}

#[test]
fn shift_jis_and_euc_jp_are_decoded_strictly() {
    let dir = TempDir::new().unwrap();
    for (name, codec) in [
        ("shift-jis", encoding_rs::SHIFT_JIS),
        ("euc-jp", encoding_rs::EUC_JP),
    ] {
        let (bytes, _, error) = codec.encode("東京 ｶﾞｯｺｳ\n");
        assert!(!error);
        let output = run(dir.path(), &["--encoding", name, "がっこう"], &bytes);
        assert_eq!(code(&output), 0);
        assert_eq!(text(&output), "<stdin>:1:東京 ｶﾞｯｺｳ\n");
        assert_eq!(code(&run(dir.path(), &["東京"], &bytes)), 2);
    }
    assert_eq!(
        code(&run(dir.path(), &["--encoding", "shift-jis", "x"], &[0x81])),
        2
    );
}

#[test]
fn bom_crlf_final_line_and_binary_detection() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        text(&run(dir.path(), &["hit"], b"\xef\xbb\xbfhit\r\nhit")),
        "<stdin>:1:hit\n<stdin>:2:hit\n"
    );
    assert_eq!(
        text(&run(dir.path(), &["hit"], b"hit\r")),
        "<stdin>:1:hit\r\n"
    );
    assert_eq!(code(&run(dir.path(), &["hit"], b"hit\0\n")), 1);
    assert_eq!(code(&run(dir.path(), &["hit", "-a"], b"hit\0\n")), 0);
    assert_eq!(code(&run(dir.path(), &["hit"], &[0xff, b'\n'])), 2);
}

#[test]
fn color_ranges_and_zero_width_regex_are_safe() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        text(&run(
            dir.path(),
            &["が", "--color", "always"],
            "前ｶﾞ後\n".as_bytes()
        )),
        "<stdin>:1:前\x1b[1;31mｶﾞ\x1b[0m後\n"
    );
    assert_eq!(
        text(&run(
            dir.path(),
            &["--regex", "^|$", "--json"],
            "か\u{3099}\n".as_bytes()
        ))
        .lines()
        .count(),
        1
    );
    assert_eq!(text(&run(dir.path(), &["--regex", "^", "-o"], b"hi\n")), "");
}

#[test]
fn parallel_output_remains_valid_json() {
    let dir = TempDir::new().unwrap();
    for n in 0..40 {
        fs::write(dir.path().join(format!("{n}.txt")), "ガッコウ\n".repeat(3)).unwrap();
    }
    let output = run(dir.path(), &["がっこう", ".", "--json", "-j4"], b"");
    assert_eq!(code(&output), 0);
    assert_eq!(text(&output).lines().count(), 120);
    for line in text(&output).lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["type"], "match");
    }
}

#[test]
fn explicit_stdin_and_file_paths_can_be_combined() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file.txt"), "ガ\n").unwrap();
    let output = run(dir.path(), &["が", "-", "file.txt"], "ｶﾞ\n".as_bytes());
    assert_eq!(text(&output), "<stdin>:1:ｶﾞ\nfile.txt:1:ガ\n");
}

#[test]
fn broken_pipe_does_not_deadlock_workers() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("many.txt"), "hit\n".repeat(20000)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(dir.path())
        .args(["hit", "."])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("broken pipe hung");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
