use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn run(root: &Path, args: &[&str], input: &str, path: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(root)
        .env("PATH", path)
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

#[cfg(unix)]
fn fake_rg(root: &Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let path = bin.join("rg");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[test]
fn smart_preserves_normalization_even_for_ascii_queries_and_when_rg_absent() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "ＡＰＩ\nAPI\n").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--explain-plan", "API", "x.txt"],
        "",
        d.path(),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text(&out).contains("ＡＰＩ") && text(&out).contains("API"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("engine=native"));
    let out = run(
        d.path(),
        &["--smart", "がっこう"],
        "ｶﾞｯｺｳ\nガッコウ\n",
        d.path(),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out).lines().count(), 2);
}

#[test]
fn explain_keeps_conditions_in_the_first_result_and_explicit_zero_overrides_it() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("code.rs"),
        "fn deadline() {\n    if active {\n        Some(control.clone())\n    }\n}\n",
    )
    .unwrap();
    let out = run(
        d.path(),
        &[
            "--smart", "--regex", "--intent", "explain", "deadline", "code.rs",
        ],
        "",
        d.path(),
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out).contains("if active") && text(&out).contains("Some(control.clone())"));
    let out = run(
        d.path(),
        &[
            "--smart", "--regex", "--intent", "explain", "-C0", "deadline", "code.rs",
        ],
        "",
        d.path(),
    );
    assert_eq!(text(&out).lines().count(), 1);
}

#[cfg(unix)]
#[test]
fn raw_code_uses_rg_once_and_translates_json_without_losing_context() {
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("code.rs"),
        "fn deadline() {\n    if active {\n        Some(control.clone())\n    }\n}\n",
    )
    .unwrap();
    let bin = fake_rg(
        d.path(),
        "printf '%s\\n' \"$@\" > argv\nprintf '%s\\n' '{\"type\":\"match\",\"data\":{\"path\":{\"text\":\"code.rs\"},\"line_number\":1,\"lines\":{\"text\":\"fn deadline() {\\n\"},\"submatches\":[{\"start\":3,\"end\":11}]}}' '{\"type\":\"context\",\"data\":{\"path\":{\"text\":\"code.rs\"},\"line_number\":2,\"lines\":{\"text\":\"    if active {\\n\"},\"submatches\":[]}}'",
    );
    let out = run(
        d.path(),
        &[
            "--smart",
            "--regex",
            "--intent",
            "explain",
            "--explain-plan",
            "deadline",
            "code.rs",
        ],
        "",
        &bin,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text(&out).contains("if active"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("engine=rg"));
    let argv = fs::read_to_string(d.path().join("argv")).unwrap();
    assert!(argv.contains("--json\n") && argv.contains("--after-context\n3\n"));
    assert!(argv.contains("--no-config\n") && argv.contains("--no-require-git\n"));
}

#[cfg(unix)]
#[test]
fn incompatible_forced_rg_is_rejected_and_rg_errors_are_not_retried() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x"), "hit\n").unwrap();
    let bin = fake_rg(
        d.path(),
        "echo called >> invocations\necho denied >&2\nexit 2",
    );
    let out = run(
        d.path(),
        &["--smart", "--engine", "rg", "hit", "x"],
        "",
        &bin,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(!d.path().join("invocations").exists());
    let out = run(d.path(), &["--smart", "--regex", "hit", "x"], "", &bin);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(d.path().join("invocations"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("denied"));
}

#[test]
fn short_output_is_complete_and_long_output_is_bounded() {
    let d = TempDir::new().unwrap();
    let out = run(
        d.path(),
        &["--smart", "--max-tokens", "64", "hit"],
        "hit\n",
        d.path(),
    );
    assert_eq!(text(&out), "<stdin>:1:hit\n");
    let input = (0..100)
        .map(|i| format!("hit 日本語{} unique content repeated for budget\n", i))
        .collect::<String>();
    let out = run(
        d.path(),
        &["--smart", "--max-tokens", "64", "hit"],
        &input,
        d.path(),
    );
    let tok = tiktoken_rs::o200k_base().unwrap();
    assert!(tok.encode_ordinary(&text(&out)).len() <= 64);
    assert!(text(&out).contains("matched=100"));
}

#[test]
fn empty_smart_result_is_empty_and_machine_modes_are_not_silently_compressed() {
    let d = TempDir::new().unwrap();
    let out = run(d.path(), &["--smart", "missing"], "other\n", d.path());
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    for mode in ["--json", "-q", "-l", "-c", "-o"] {
        assert_eq!(
            run(d.path(), &["--smart", mode, "hit"], "hit\n", d.path())
                .status
                .code(),
            Some(2)
        );
    }
}

#[test]
fn complete_line_wins_over_a_shorter_but_clipped_excerpt_when_it_fits() {
    let d = TempDir::new().unwrap();
    let input = format!("hit {} return Some(control)\n", "x".repeat(200));
    let out = run(
        d.path(),
        &[
            "--smart",
            "--snippet-chars",
            "16",
            "--max-tokens",
            "512",
            "hit",
        ],
        &input,
        d.path(),
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out).contains("return Some(control)"));
    assert!(!text(&out).contains("clipped="));
}

#[test]
fn bounded_smart_reports_context_loss_even_when_all_matches_are_visible() {
    let d = TempDir::new().unwrap();
    let input = format!("{}\nhit\n{}\n", "before ".repeat(100), "after ".repeat(100));
    let out = run(
        d.path(),
        &["--smart", "-C1", "--max-tokens", "64", "hit"],
        &input,
        d.path(),
    );
    let tok = tiktoken_rs::o200k_base().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(tok.encode_ordinary(&text(&out)).len() <= 64);
    assert!(text(&out).contains("matched=1 shown=1 omitted=0"));
    assert!(text(&out).contains("context_omitted=") && text(&out).contains("context_clipped="));
    assert!(!text(&out).contains("context_omitted=0 context_clipped=0"));
}

#[test]
fn raw_smart_keeps_crlf_end_anchor_semantics() {
    let d = TempDir::new().unwrap();
    if Command::new("rg").arg("--version").output().is_err() {
        eprintln!("optional rg not installed; real-rg CRLF integration skipped");
        return;
    }
    fs::write(d.path().join("crlf.txt"), b"needle\r\nother\r\n").unwrap();
    let path = std::env::var_os("PATH").unwrap();
    let out = run(
        d.path(),
        &[
            "--smart",
            "--regex",
            "--explain-plan",
            "needle$",
            "crlf.txt",
        ],
        "",
        Path::new(&path),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text(&out).contains("1:needle"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("engine=rg"));
}

#[test]
fn raw_smart_keeps_line_search_behavior_for_newline_patterns() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("x.txt"), "a\nb\n").unwrap();
    let path = std::env::var_os("PATH").unwrap();
    let out = run(
        d.path(),
        &["--smart", "--regex", r"a\nb", "x.txt"],
        "",
        Path::new(&path),
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn real_rg_and_native_match_on_bom_globs_multiple_patterns_and_context() {
    if Command::new("rg").arg("--version").output().is_err() {
        return;
    }
    let d = TempDir::new().unwrap();
    fs::write(
        d.path().join("a.txt"),
        "\u{feff}needle\r\n条件\r\nother\r\n",
    )
    .unwrap();
    fs::write(d.path().join("b.txt"), "before\nneedle\nafter\n").unwrap();
    fs::write(d.path().join("skip.txt"), "needle\n").unwrap();
    fs::write(d.path().join(".hidden.txt"), "needle\n").unwrap();
    fs::write(d.path().join(".gitignore"), "skip.txt\n").unwrap();
    let path = std::env::var_os("PATH").unwrap();
    let args = [
        "--smart", "--regex", "-C1", "-g", "*.txt", "-e", "needle", "-e", "other", ".",
    ];
    let mut rg = vec!["--engine", "rg"];
    rg.extend_from_slice(&args);
    let mut native = vec!["--engine", "native"];
    native.extend_from_slice(&args);
    let a = run(d.path(), &rg, "", Path::new(&path));
    let b = run(d.path(), &native, "", Path::new(&path));
    assert_eq!(
        a.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&a.stderr)
    );
    assert_eq!(b.status.code(), Some(0));
    let at = text(&a);
    let bt = text(&b);
    let mut al: Vec<_> = at.lines().collect();
    let mut bl: Vec<_> = bt.lines().collect();
    al.sort();
    bl.sort();
    assert_eq!(al, bl);
}
