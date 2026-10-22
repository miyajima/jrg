use axum::{extract::State, routing::post, Json, Router};
use jrg_rtk_proxy::{build_router, JrgTransform, SelectionConfig};
use rtk_proxy::{
    model::{InterceptedRequest, Provider},
    pipeline::Transform,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::mpsc;

fn config() -> SelectionConfig {
    SelectionConfig {
        selector: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/jrg-select")
            .canonicalize()
            .expect("build jrg-select first"),
        search_tools: vec!["search".into()],
        page_tools: vec!["fetch".into()],
        max_tokens: 500,
        timeout: Duration::from_secs(10),
    }
}

fn source() -> Value {
    json!({"results":[
        {"id":"answer","url":"https://example.test/original?x=1","title":"Aurora evidence",
         "text":"Aurora release date is 2026-10-20. Original evidence text."},
        {"id":"noise","url":"https://noise.test/","title":"Bananas",
         "text":format!("RAW_NOISE {}", "banana unrelated weather ".repeat(300))}
    ]})
}

fn conversation(name: &str, input: Value, result: Value) -> Value {
    json!({"model":"synthetic-model","max_tokens":100,"stream":false,"messages":[
        {"role":"user","content":"Aurora release date"},
        {"role":"assistant","content":[{"type":"tool_use","id":"same-id","name":name,"input":input}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"same-id","content":result}]}
    ]})
}

fn request(body: Value) -> InterceptedRequest {
    InterceptedRequest::for_pipeline(Provider::Anthropic, "/v1/messages".into(), body)
}

fn result(body: &Value) -> &Value {
    &body["messages"][2]["content"][0]
}

#[tokio::test]
async fn search_removes_noise_and_preserves_original_citation_text() {
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora release date"}),
        json!(source().to_string()),
    ));
    req.body["messages"][2]["content"][0]["raw_debug"] = json!("RAW_NOISE debug mirror");
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    let selected = result(&req.body).to_string();
    assert!(
        !selected.contains("RAW_NOISE"),
        "unselected raw source reached request"
    );
    assert!(selected.contains("https://example.test/original?x=1"));
    assert!(selected.contains("Aurora release date is 2026-10-20. Original evidence text."));
    assert!(selected.contains("source_hash"));
    assert!(selected.contains("omitted_count"));
}

#[tokio::test]
async fn markdown_selection_keeps_table_and_footnote_context() {
    let page = json!({"id":"p","url":"https://example.test/page","title":"Aurora pricing",
        "markdown":"# Aurora pricing\n\n| Plan | Price |\n| --- | --- |\n| Aurora | $9 [^a] |\n\n[^a]: Aurora price excludes tax.\n\n# Other\n\nUnrelated vegetable recipes."});
    let mut req = request(conversation(
        "fetch",
        json!({"focus":"Aurora pricing"}),
        json!([
            {"type":"text","text":page.to_string()}
        ]),
    ));
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    let selected = result(&req.body).to_string();
    assert!(selected.contains("jrg_evidence_selection"));
    assert!(selected.contains("| Plan | Price |"));
    assert!(selected.contains("[^a]: Aurora price excludes tax."));
    assert!(selected.contains("https://example.test/page"));
}

#[tokio::test]
async fn historical_focus_stays_fixed_after_new_user_question() {
    let transform = JrgTransform::new(config());
    let original = conversation("search", json!({}), json!(source().to_string()));
    let mut first = request(original.clone());
    transform.on_request(&mut first).await.unwrap();
    let mut later = original;
    later["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant","content":"Answer"}));
    later["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user","content":"banana unrelated weather"}));
    let mut later = request(later);
    transform.on_request(&mut later).await.unwrap();
    assert_eq!(result(&first.body), result(&later.body));
    assert!(!result(&later.body).to_string().contains("RAW_NOISE"));
}

#[tokio::test]
async fn memo_does_not_cross_contaminate_sessions_with_reused_tool_ids() {
    let transform = JrgTransform::new(config());
    let mut first = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(source().to_string()),
    ));
    first.session = Some("session-a".into());
    transform.on_request(&mut first).await.unwrap();
    let mut second_source = source();
    second_source["results"][0]["text"] = json!("Aurora session B original evidence.");
    let mut second = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(second_source.to_string()),
    ));
    second.session = Some("session-b".into());
    transform.on_request(&mut second).await.unwrap();
    assert!(result(&second.body)
        .to_string()
        .contains("session B original evidence"));
    assert_ne!(result(&first.body), result(&second.body));
}

fn bilingual_source() -> Value {
    json!({"results":[
        {"id":"aurora","url":"https://example.test/aurora","title":"Aurora",
         "text":"Aurora launched on October 20."},
        {"id":"borealis","url":"https://example.test/borealis","title":"Borealis",
         "text":"Borealis launched on October 21."},
        {"id":"noise","url":"https://noise.test/","title":"Vegetables",
         "text":"RAW_NOISE unrelated vegetables"}
    ]})
}

#[tokio::test]
async fn bilingual_japanese_focus_uses_explicit_english_expansion() {
    let mut req = request(conversation(
        "search",
        json!({
            "jrg_focus":"公開日", "jrg_focus_en":"  Aurora  "
        }),
        json!(bilingual_source().to_string()),
    ));
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    let selected = result(&req.body).to_string();
    assert_eq!(result(&req.body)["is_error"], false);
    assert!(selected.contains("Aurora launched on October 20."));
    assert!(selected.contains("https://example.test/aurora"));
    assert!(!selected.contains("Borealis"));
    assert!(!selected.contains("RAW_NOISE"));
}

#[tokio::test]
async fn bilingual_absent_english_keeps_legacy_focus_behavior() {
    let mut req = request(conversation(
        "search",
        json!({"jrg_focus":"Aurora"}),
        json!(bilingual_source().to_string()),
    ));
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    assert_eq!(result(&req.body)["is_error"], false);
    assert!(result(&req.body)
        .to_string()
        .contains("Aurora launched on October 20."));
    assert!(!result(&req.body).to_string().contains("Borealis"));
}

#[tokio::test]
async fn bilingual_invalid_english_and_combined_byte_budget_fail_closed() {
    for expansion in [
        Value::Null,
        json!(true),
        json!(7),
        json!([]),
        json!({}),
        json!(""),
        json!(" \n\t "),
        json!("x".repeat(4090)),
        json!("あ".repeat(1364)),
    ] {
        let mut req = request(conversation(
            "search",
            json!({"jrg_focus":"Aurora","jrg_focus_en":expansion}),
            json!(bilingual_source().to_string()),
        ));
        JrgTransform::new(config())
            .on_request(&mut req)
            .await
            .unwrap();
        assert_eq!(
            result(&req.body)["is_error"],
            true,
            "invalid English focus must be explicit"
        );
        assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
        assert!(!result(&req.body).to_string().contains("Aurora launched"));
    }
    // Six primary bytes + one separator + 4089 expansion bytes = 4096.
    let mut boundary = request(conversation(
        "search",
        json!({"jrg_focus":"Aurora","jrg_focus_en":"x".repeat(4089)}),
        json!(bilingual_source().to_string()),
    ));
    JrgTransform::new(config())
        .on_request(&mut boundary)
        .await
        .unwrap();
    assert_eq!(result(&boundary.body)["is_error"], false);
    assert!(result(&boundary.body)
        .to_string()
        .contains("Aurora launched"));
}

#[tokio::test]
async fn bilingual_historical_english_focus_stays_bound_to_origin() {
    let transform = JrgTransform::new(config());
    let original = conversation(
        "search",
        json!({"jrg_focus":"公開日","jrg_focus_en":"Aurora"}),
        json!(bilingual_source().to_string()),
    );
    let mut first = request(original.clone());
    transform.on_request(&mut first).await.unwrap();
    let mut later = original;
    later["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant","content":"Answer"}));
    later["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user","content":"Borealis instead"}));
    let mut later = request(later);
    transform.on_request(&mut later).await.unwrap();
    assert!(result(&first.body).to_string().contains("Aurora launched"));
    assert_eq!(result(&first.body), result(&later.body));
    assert!(!result(&later.body).to_string().contains("Borealis"));
}

#[tokio::test]
async fn bilingual_memo_distinguishes_changed_english_with_same_primary_and_source() {
    let transform = JrgTransform::new(config());
    let mut first = request(conversation(
        "search",
        json!({"jrg_focus":"公開日","jrg_focus_en":"Aurora"}),
        json!(bilingual_source().to_string()),
    ));
    first.session = Some("same-session".into());
    transform.on_request(&mut first).await.unwrap();
    let mut second = request(conversation(
        "search",
        json!({"jrg_focus":"公開日","jrg_focus_en":"Borealis"}),
        json!(bilingual_source().to_string()),
    ));
    second.session = Some("same-session".into());
    transform.on_request(&mut second).await.unwrap();
    assert!(result(&first.body).to_string().contains("Aurora launched"));
    assert!(!result(&first.body).to_string().contains("Borealis"));
    assert!(result(&second.body)
        .to_string()
        .contains("Borealis launched"));
    assert!(!result(&second.body).to_string().contains("Aurora launched"));
    assert_ne!(result(&first.body), result(&second.body));
}

#[tokio::test]
async fn malformed_multimedia_and_missing_origin_fail_closed() {
    for content in [
        json!("RAW_NOISE malformed json"),
        json!([
            {"type":"text","text":source().to_string()}, {"type":"image","source":{"data":"RAW_NOISE"}}
        ]),
    ] {
        let mut req = request(conversation("search", json!({"query":"Aurora"}), content));
        JrgTransform::new(config())
            .on_request(&mut req)
            .await
            .unwrap();
        assert_eq!(result(&req.body)["is_error"], true);
        assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
    }
    let mut body = conversation("search", json!({}), json!("RAW_NOISE no origin"));
    body["messages"][1]["content"] = json!([]);
    let mut req = request(body);
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    assert_eq!(result(&req.body)["is_error"], true);
    assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
}

#[tokio::test]
async fn original_tool_errors_remain_errors_without_raw_mirrors() {
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!("RAW_NOISE provider error"),
    ));
    req.body["messages"][2]["content"][0]["is_error"] = json!(true);
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    assert_eq!(result(&req.body)["is_error"], true);
    assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
}

#[tokio::test]
async fn non_target_and_non_anthropic_requests_stay_unchanged() {
    let body = conversation(
        "read_file",
        json!({}),
        json!("RAW_NOISE expected passthrough"),
    );
    let mut req = request(body.clone());
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    assert_eq!(body, req.body);
    req.provider = Provider::Openai;
    req.body = conversation("search", json!({}), json!(source().to_string()));
    let before = req.body.clone();
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    assert_eq!(before, req.body);
}

#[cfg(unix)]
fn executable(script: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("selector");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (temp, path)
}

fn expected_english_guidance() -> &'static str {
    concat!(
        "Optional English equivalent for English-language evidence. ",
        "Generate from the user's task before reading tool output. ",
        "Preserve negation, versions, numbers, and identifiers; do not add answers or assume settings. ",
        "Where unambiguous, include standard API terminology and a short equivalent form of numerical bounds ",
        "(for example, zero or negative / non-positive). Keep the expression concise. Do not add facts."
    )
}

fn selector_response(text: &str) -> Value {
    json!({"documents":[{"id":"answer","url":"https://example.test/original?x=1","title":"Aurora evidence",
        "blocks":[{"id":"snippet","kind":"result","text":text}],"source_hash":"a".repeat(64)}],
        "omitted_count":1,"duplicate_count":0,"status":"selected","tokens":100,"max_tokens":500})
}

#[cfg(unix)]
#[tokio::test]
async fn verified_policy_selector_argv_uses_verified_only() {
    let capture = tempfile::tempdir().unwrap();
    let args_path = capture.path().join("argv");
    let script = format!(
        "printf '%s\\n' \"$@\" > '{}'\n/bin/cat >/dev/null\n/bin/cat <<'RESPONSE'\n{}\nRESPONSE",
        args_path.to_str().unwrap().replace('\'', "'\\''"),
        selector_response("Verified-only invocation")
    );
    let (_temp, selector) = executable(&script);
    let mut cfg = config();
    cfg.selector = selector;
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(source().to_string()),
    ));
    JrgTransform::new(cfg).on_request(&mut req).await.unwrap();
    assert_eq!(result(&req.body)["is_error"], false);
    let args = std::fs::read_to_string(args_path).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "--query-rules",
            "verified",
            "--adapter",
            "search-results",
            "-"
        ]
    );
}

#[tokio::test]
async fn verified_policy_guidance_updates_only_existing_mapped_string_fields() {
    let tools = json!([
        {"name":"search","description":"Keep tool description","input_schema":{"type":"object","properties":{
            "query":{"type":"string","description":"Keep query"},
            "jrg_focus_en":{"type":"string","description":"Old guidance","maxLength":120}},"required":["query"]}},
        {"name":"fetch","input_schema":{"type":"object","properties":{"url":{"type":"string"}}}},
        {"name":"fetch","input_schema":{"type":"object","properties":{"jrg_focus_en":{"type":"number","description":"Keep numeric"}}}},
        {"name":"unmapped","input_schema":{"type":"object","properties":{"jrg_focus_en":{"type":"string","description":"Keep unmapped"}}}}
    ]);
    let mut req = request(json!({"tools":tools}));
    JrgTransform::new(config())
        .on_request(&mut req)
        .await
        .unwrap();
    let mut expected = tools;
    expected[0]["input_schema"]["properties"]["jrg_focus_en"]["description"] =
        json!(expected_english_guidance());
    assert_eq!(req.body["tools"], expected);
}

#[cfg(unix)]
#[tokio::test]
async fn verified_policy_modified_selector_is_rejected_before_cached_result_reuse() {
    let first = selector_response("Pinned selector evidence A").to_string();
    let (_temp, selector) = executable(&format!(
        "/bin/cat >/dev/null\n/bin/cat <<'RESPONSE'\n{first}\nRESPONSE"
    ));
    let mut cfg = config();
    cfg.selector = selector.clone();
    let transform = JrgTransform::new(cfg);
    let original = conversation(
        "search",
        json!({"jrg_focus":"Aurora","jrg_focus_en":"release date"}),
        json!(source().to_string()),
    );
    let mut before = request(original.clone());
    transform.on_request(&mut before).await.unwrap();
    assert!(result(&before.body)
        .to_string()
        .contains("Pinned selector evidence A"));
    let metadata = std::fs::metadata(&selector).unwrap();
    let second = first.replace("evidence A", "evidence B");
    std::fs::write(
        &selector,
        format!("#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'RESPONSE'\n{second}\nRESPONSE\n"),
    )
    .unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&selector)
        .unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
        .unwrap();
    assert_eq!(std::fs::metadata(&selector).unwrap().len(), metadata.len());
    let mut after = request(original);
    transform.on_request(&mut after).await.unwrap();
    assert_eq!(result(&after.body)["is_error"], true);
    assert!(result(&after.body).to_string().contains("selector_changed"));
    assert!(!result(&after.body)
        .to_string()
        .contains("Pinned selector evidence"));
    assert!(!result(&after.body).to_string().contains("RAW_NOISE"));
}

#[cfg(unix)]
#[tokio::test]
async fn verified_policy_missing_initial_selector_is_not_hot_loaded() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("initially-missing");
    let mut cfg = config();
    cfg.selector = path.clone();
    let transform = JrgTransform::new(cfg);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'RESPONSE'\n{}\nRESPONSE\n",
            selector_response("New selector must not hot load")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(source().to_string()),
    ));
    transform.on_request(&mut req).await.unwrap();
    assert_eq!(result(&req.body)["is_error"], true);
    assert!(!result(&req.body).to_string().contains("New selector"));
    assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
}

#[cfg(unix)]
#[tokio::test]
async fn verified_policy_bare_selector_name_resolves_from_absolute_path_entries() {
    let mut cfg = config();
    cfg.selector = PathBuf::from("true");
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(source().to_string()),
    ));
    JrgTransform::new(cfg).on_request(&mut req).await.unwrap();
    assert_eq!(result(&req.body)["is_error"], true);
    // The standard `true` executable ran and returned empty stdout, rather than
    // failing path resolution or being treated as a selector JSON response.
    assert!(result(&req.body)
        .to_string()
        .contains("invalid_selector_response"));
    assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
}

#[tokio::test]
async fn verified_policy_relative_selector_path_is_rejected() {
    let mut cfg = config();
    cfg.selector = PathBuf::from("./selector");
    let mut req = request(conversation(
        "search",
        json!({"query":"Aurora"}),
        json!(source().to_string()),
    ));
    JrgTransform::new(cfg).on_request(&mut req).await.unwrap();
    assert_eq!(result(&req.body)["is_error"], true);
    assert!(result(&req.body)
        .to_string()
        .contains("selector_path_rejected"));
    assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
}

#[cfg(unix)]
#[tokio::test]
async fn selector_failure_timeout_and_oversized_output_fail_closed() {
    for script in [
        "exit 7",
        "printf 'RAW_NOISE invalid selector response'",
        "exec /bin/sleep 5",
        "exec /usr/bin/yes RAW_NOISE",
    ] {
        let (_temp, path) = executable(script);
        let mut cfg = config();
        cfg.selector = path;
        cfg.timeout = Duration::from_millis(100);
        let mut req = request(conversation(
            "search",
            json!({"query":"Aurora"}),
            json!(source().to_string()),
        ));
        let started = std::time::Instant::now();
        JrgTransform::new(cfg).on_request(&mut req).await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(result(&req.body)["is_error"], true);
        assert!(!result(&req.body).to_string().contains("RAW_NOISE"));
    }
}

#[cfg(unix)]
struct DescendantCleanup(Option<u32>);

#[cfg(unix)]
impl Drop for DescendantCleanup {
    fn drop(&mut self) {
        let Some(pid) = self.0 else {
            return;
        };
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(unix)]
async fn descendant_case(mode: &str) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let mut unrelated = tokio::process::Command::new("/bin/sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let marker = temp.path().join("descendant.pid");
    let selector = temp.path().join("selector");
    let ending = match mode {
        "overflow" => "/bin/dd if=/dev/zero bs=1048576 count=5 2>/dev/null; wait",
        "leader_exit" => "exit 0",
        _ => "wait",
    };
    std::fs::write(&selector, format!(
        "#!/bin/sh\n/bin/cat >/dev/null\n/bin/sleep 30 </dev/null >/dev/null 2>&1 &\nchild=$!\nprintf '%s' \"$child\" > '{}'\n{ending}\n",
        marker.to_str().unwrap().replace('\'', "'\\''")
    )).unwrap();
    std::fs::set_permissions(&selector, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut cfg = config();
    cfg.selector = selector;
    cfg.timeout = Duration::from_secs(if mode == "cancel" { 30 } else { 5 });
    let mut task = tokio::spawn(async move {
        let mut req = request(conversation(
            "search",
            json!({"query":"Aurora"}),
            json!(source().to_string()),
        ));
        JrgTransform::new(cfg).on_request(&mut req).await.unwrap();
        req
    });
    let pid = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&marker) {
                if let Ok(pid) = text.parse::<u32>() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("selector descendant started");
    let mut cleanup = DescendantCleanup(Some(pid));
    if mode == "cancel" {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    } else {
        let req = tokio::time::timeout(Duration::from_secs(7), &mut task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result(&req.body)["is_error"], true);
    }
    let stopped = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let alive = std::process::Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        unrelated.try_wait().unwrap().is_none(),
        "unrelated process was signalled"
    );
    unrelated.kill().await.unwrap();
    unrelated.wait().await.unwrap();
    if stopped.is_ok() {
        cleanup.0 = None;
    }
    assert!(stopped.is_ok(), "selector descendant survived {mode}");
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_timeout_cleanup() {
    descendant_case("timeout").await;
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_overflow_cleanup() {
    descendant_case("overflow").await;
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_cancellation_cleanup() {
    descendant_case("cancel").await;
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_cleanup_after_leader_exit() {
    descendant_case("leader_exit").await;
}

async fn listen(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), task)
}

async fn upstream(
    State(sender): State<Arc<mpsc::Sender<Value>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    sender.send(body).await.unwrap();
    Json(
        json!({"id":"fake-response","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"text","text":"local fixture reply"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":10,"output_tokens":3}}),
    )
}

#[tokio::test]
async fn actual_rtk_router_transforms_before_fake_upstream_and_relays_response() {
    let (sender, mut receive) = mpsc::channel(2);
    let fake = Router::new()
        .route("/v1/messages", post(upstream))
        .with_state(Arc::new(sender));
    let (fake_url, fake_task) = listen(fake).await;
    let (proxy_url, proxy_task) = listen(build_router(config(), fake_url)).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "synthetic-test-only")
        .header("anthropic-version", "2023-06-01")
        .json(&conversation(
            "search",
            json!({"query":"Aurora release date"}),
            json!(source().to_string()),
        ))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.json::<Value>().await.unwrap()["id"],
        "fake-response"
    );
    let forwarded = tokio::time::timeout(Duration::from_secs(3), receive.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        !forwarded.to_string().contains("RAW_NOISE"),
        "raw source reached HTTP upstream"
    );
    assert!(forwarded.to_string().contains("source_hash"));
    let unsupported = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(unsupported.status(), reqwest::StatusCode::NOT_FOUND);
    let responses = client
        .post(format!("{proxy_url}/v1/responses"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(responses.status(), reqwest::StatusCode::NOT_FOUND);
    fake_task.abort();
    proxy_task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn verified_policy_http_forwards_verified_argv_guidance_and_origin_bound_english() {
    let capture = tempfile::tempdir().unwrap();
    let args_path = capture.path().join("argv");
    let input_path = capture.path().join("input.json");
    let script = format!(
        "printf '%s\\n' \"$@\" > '{}'\n/bin/cat > '{}'\n/bin/cat <<'RESPONSE'\n{}\nRESPONSE",
        args_path.to_str().unwrap().replace('\'', "'\\''"),
        input_path.to_str().unwrap().replace('\'', "'\\''"),
        selector_response("Aurora original English evidence")
    );
    let (_selector_temp, selector) = executable(&script);
    let mut cfg = config();
    cfg.selector = selector;
    let (sender, mut receive) = mpsc::channel(2);
    let (fake_url, fake_task) = listen(
        Router::new()
            .route("/v1/messages", post(upstream))
            .with_state(Arc::new(sender)),
    )
    .await;
    let (proxy_url, proxy_task) = listen(build_router(cfg, fake_url)).await;
    let mut body = conversation(
        "search",
        json!({"jrg_focus":"公開日","jrg_focus_en":" Aurora "}),
        json!(source().to_string()),
    );
    body["tools"] = json!([{"name":"search","input_schema":{"type":"object","properties":{
        "jrg_focus_en":{"type":"string","description":"Old guidance"},"query":{"type":"string"}}}}]);
    let original_call = body["messages"][1].clone();
    body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant","content":"Answer"}));
    body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user","content":"Borealis instead"}));
    let response = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "synthetic-verified-policy-only")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.json::<Value>().await.unwrap()["id"],
        "fake-response"
    );
    let forwarded = tokio::time::timeout(Duration::from_secs(3), receive.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        forwarded["tools"][0]["input_schema"]["properties"]["jrg_focus_en"]["description"],
        expected_english_guidance()
    );
    assert_eq!(forwarded["messages"][1], original_call);
    assert!(result(&forwarded)
        .to_string()
        .contains("Aurora original English evidence"));
    assert!(!forwarded.to_string().contains("RAW_NOISE"));
    let args = std::fs::read_to_string(args_path).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "--query-rules",
            "verified",
            "--adapter",
            "search-results",
            "-"
        ]
    );
    let envelope: Value = serde_json::from_slice(&std::fs::read(input_path).unwrap()).unwrap();
    assert_eq!(envelope["query"], "公開日");
    assert_eq!(envelope["query_expansions"], json!(["Aurora"]));
    fake_task.abort();
    proxy_task.abort();
}

#[tokio::test]
async fn compiled_binary_serves_local_http_and_rejects_non_loopback_bind() {
    use std::process::Stdio;
    use tokio::io::AsyncBufReadExt;
    let (sender, mut receive) = mpsc::channel(2);
    let (fake_url, fake_task) = listen(
        Router::new()
            .route("/v1/messages", post(upstream))
            .with_state(Arc::new(sender)),
    )
    .await;
    let binary = env!("CARGO_BIN_EXE_jrg-rtk-proxy");
    let cfg = config();
    let mut child = tokio::process::Command::new(binary)
        .args([
            "--selector",
            cfg.selector.to_str().unwrap(),
            "--anthropic-upstream",
            &fake_url,
            "--bind",
            "127.0.0.1:0",
            "--max-tokens",
            "500",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = tokio::io::BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let address = line
        .split_whitespace()
        .nth(3)
        .unwrap()
        .trim_end_matches(';');
    let response = reqwest::Client::new()
        .post(format!("http://{address}/v1/messages"))
        .header("x-api-key", "synthetic-binary-test-only")
        .json(&conversation(
            "search",
            json!({"query":"Aurora release date"}),
            json!(source().to_string()),
        ))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.json::<Value>().await.unwrap()["id"],
        "fake-response"
    );
    let forwarded = tokio::time::timeout(Duration::from_secs(5), receive.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!forwarded.to_string().contains("RAW_NOISE"));
    assert!(forwarded.to_string().contains("source_hash"));
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    fake_task.abort();

    let rejected = tokio::process::Command::new(binary)
        .args([
            "--selector",
            cfg.selector.to_str().unwrap(),
            "--anthropic-upstream",
            "http://127.0.0.1:1",
            "--bind",
            "0.0.0.0:8081",
        ])
        .output()
        .await
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("loopback"));
}
