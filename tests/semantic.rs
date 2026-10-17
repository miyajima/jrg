//! The protocol fixture verifies integration and boundaries, not embedding quality.
#[cfg(unix)]
mod unix {
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        process::Command,
        thread,
    };
    use tempfile::TempDir;
    #[test]
    fn semantic_candidate_can_recover_a_lexical_miss_and_cannot_escape_requested_paths() {
        let d = TempDir::new().unwrap();
        fs::write(
            d.path().join("vehicle.rs"),
            "fn car() {\n    verify_engine();\n}\n",
        )
        .unwrap();
        let sock = d.path().join("s");
        let listener = UnixListener::bind(&sock).unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        let server = thread::spawn(move || {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
            let (mut s, _) = loop {
                if let Ok(pair) = listener.accept() {
                    break pair;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "embedding client did not connect"
                );
                thread::sleep(std::time::Duration::from_millis(10));
            };
            let mut req = String::new();
            BufReader::new(s.try_clone().unwrap())
                .read_line(&mut req)
                .unwrap();
            let req: serde_json::Value = serde_json::from_str(&req).unwrap();
            assert!(
                req["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_str().unwrap().ends_with("vehicle.rs"))
            );
            let answer = serde_json::json!({"version":1,"model":"google/embeddinggemma-2","revision":"914f7f89142e33e77833254d9c9b90c3cef7303b","hits":[{"path":root.join("vehicle.rs"),"line":1,"text":"fn car() {","score":0.8},{"path":"/private/tmp/outside.rs","line":1,"text":"fn stolen() {}","score":0.99}]});
            writeln!(s, "{answer}").unwrap();
        });
        let out = Command::new(env!("CARGO_BIN_EXE_jrg"))
            .current_dir(d.path())
            .env("JRG_EMBEDDING_SOCKET", sock)
            .args([
                "--smart",
                "--intent",
                "review",
                "--rank-query",
                "vehicle engine",
                "vehicle",
                ".",
            ])
            .output()
            .unwrap();
        server.join().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("fn car()"), "{text}");
        assert!(text.contains("verify_engine()"), "{text}");
        assert!(!text.contains("stolen"));
        assert!(out.status.success(), "{text}");
    }
    #[test]
    fn unavailable_embedding_server_reports_lexical_fallback_and_preserves_code() {
        let d = TempDir::new().unwrap();
        fs::write(d.path().join("a.rs"), "fn car() { target(); }\n").unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_jrg"))
            .current_dir(d.path())
            .env("JRG_EMBEDDING_SOCKET", d.path().join("missing"))
            .args(["--smart", "--intent", "review", "target", "a.rs"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{text}");
        assert!(text.contains("target()"));
        assert!(text.contains("embedding fallback"), "{text}");
    }
}
#[test]
fn unsupported_explain_parser_reports_fallback_and_preserves_evidence() {
    let d = tempfile::TempDir::new().unwrap();
    std::fs::write(d.path().join("x.go"), "func f() {\n target();\n}\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_jrg"))
        .current_dir(d.path())
        .args(["--smart", "--intent", "explain", "target", "x.go"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("target();"));
    assert!(text.contains("scope_parser=lexical fallback"), "{text}");
}
