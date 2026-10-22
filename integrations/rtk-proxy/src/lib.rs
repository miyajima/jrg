//! Optional, fail-closed web evidence selection in RTK's public transform chain.
use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rtk_proxy::{
    config::Config,
    error::AppResult,
    model::{InterceptedRequest, Provider},
    pipeline::{Pipeline, Transform, TRUNCATION_MARKER_TAG},
    AppState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

/// Own one isolated process group and signal it at most once. Failure and drop
/// signal before reaping; completion signals immediately after the leader wait.
struct SelectorProcess {
    child: Child,
    #[cfg(unix)]
    group: Option<rustix::process::Pid>,
}

impl SelectorProcess {
    fn spawn(command: &mut Command) -> std::io::Result<Self> {
        #[cfg(unix)]
        command.process_group(0);
        let child = command.kill_on_drop(true).spawn()?;
        #[cfg(unix)]
        let group = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(rustix::process::Pid::from_raw);
        Ok(Self {
            child,
            #[cfg(unix)]
            group,
        })
    }

    #[cfg(unix)]
    fn signal_group(&mut self) -> std::io::Result<()> {
        use rustix::{
            io::Errno,
            process::{kill_process_group, Signal},
        };
        if let Some(group) = self.group.take() {
            match kill_process_group(group, Signal::KILL) {
                Ok(()) | Err(Errno::SRCH) => {}
                Err(error) => {
                    eprintln!(
                        "jrg selector group signal failed (OS error {})",
                        error.raw_os_error()
                    );
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }

    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child.wait().await?;
        // Do not yield between reaping and signalling. A surviving descendant
        // retains the group ID. Empty groups normally report ESRCH; a genuine
        // permission failure remains an explicit cleanup failure.
        #[cfg(unix)]
        self.signal_group()?;
        Ok(status)
    }

    async fn terminate(&mut self) -> std::io::Result<()> {
        #[cfg(unix)]
        if let Err(error) = self.signal_group() {
            let _ = self.child.start_kill();
            return Err(error);
        }
        #[cfg(not(unix))]
        self.child.start_kill()?;
        tokio::time::timeout(Duration::from_secs(1), self.child.wait())
            .await
            .map_err(|_| std::io::Error::other("selector reap timeout"))??;
        Ok(())
    }
}

impl Drop for SelectorProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.signal_group().is_err() {
            // Drop cannot report a result, but a failed cancellation cleanup
            // must remain visible without exposing selector output or arguments.
            eprintln!("jrg selector process-group cleanup failed");
        }
        // Child's kill_on_drop and Tokio reaper handle the direct child after
        // the group signal, including cancellation while stdin/stdout is open.
    }
}

pub const MAX_SELECTOR_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MEMO_ENTRIES: usize = 128;
pub const MAX_MEMO_BYTES: usize = 8 * 1024 * 1024;
const MAX_FOCUS_BYTES: usize = 4096;
const MAX_SELECTOR_BINARY_BYTES: u64 = 128 * 1024 * 1024;
const QUERY_RULE_MODE: &str = "verified";
const ENGLISH_FOCUS_DESCRIPTION: &str = concat!(
    "Optional English equivalent for English-language evidence. ",
    "Generate from the user's task before reading tool output. ",
    "Preserve negation, versions, numbers, and identifiers; do not add answers or assume settings. ",
    "Where unambiguous, include standard API terminology and a short equivalent form of numerical bounds ",
    "(for example, zero or negative / non-positive). Keep the expression concise. Do not add facts."
);

struct SelectorIdentity {
    path: PathBuf,
    sha256: String,
}

impl SelectorIdentity {
    fn resolve(configured: &Path) -> Result<Self, &'static str> {
        let requested = if configured.is_absolute() {
            configured.to_path_buf()
        } else if configured.components().count() == 1 && configured.file_name().is_some() {
            let search_path = std::env::var_os("PATH").ok_or("selector_identity_unavailable")?;
            std::env::split_paths(&search_path)
                .filter(|directory| directory.is_absolute())
                .map(|directory| directory.join(configured))
                .find(|candidate| Self::executable_file(candidate))
                .ok_or("selector_identity_unavailable")?
        } else {
            return Err("selector_path_rejected");
        };
        let path = requested
            .canonicalize()
            .map_err(|_| "selector_identity_unavailable")?;
        let sha256 = Self::fingerprint(&path)?;
        Ok(Self { path, sha256 })
    }

    fn executable_file(path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return false;
            }
        }
        true
    }

    fn fingerprint(path: &Path) -> Result<String, &'static str> {
        if !Self::executable_file(path) {
            return Err("selector_identity_unavailable");
        }
        let mut file = std::fs::File::open(path).map_err(|_| "selector_identity_unavailable")?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut bytes = 0_u64;
        loop {
            let read = std::io::Read::read(&mut file, &mut buffer)
                .map_err(|_| "selector_identity_unavailable")?;
            if read == 0 {
                break;
            }
            bytes += read as u64;
            if bytes > MAX_SELECTOR_BINARY_BYTES {
                return Err("selector_binary_too_large");
            }
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn verify(&self) -> Result<(), &'static str> {
        match Self::fingerprint(&self.path) {
            Ok(current) if current == self.sha256 => Ok(()),
            _ => Err("selector_changed"),
        }
    }
}

#[derive(Clone)]
pub struct SelectionConfig {
    pub selector: PathBuf,
    pub search_tools: Vec<String>,
    pub page_tools: Vec<String>,
    pub max_tokens: usize,
    pub timeout: Duration,
}

pub struct JrgTransform {
    config: SelectionConfig,
    selector: Result<SelectorIdentity, &'static str>,
    memo: Mutex<Memo>,
}

#[derive(Default)]
struct Memo {
    values: HashMap<String, (Value, usize)>,
    order: VecDeque<String>,
    bytes: usize,
}

impl Memo {
    fn insert(&mut self, key: String, value: Value) {
        let bytes = value.to_string().len();
        if bytes > MAX_MEMO_BYTES || self.values.contains_key(&key) {
            return;
        }
        while self.values.len() >= MAX_MEMO_ENTRIES || self.bytes + bytes > MAX_MEMO_BYTES {
            if let Some(old) = self.order.pop_front() {
                if let Some((_, size)) = self.values.remove(&old) {
                    self.bytes -= size;
                }
            } else {
                break;
            }
        }
        self.bytes += bytes;
        self.order.push_back(key.clone());
        self.values.insert(key, (value, bytes));
    }
}

#[derive(Clone)]
struct Origin {
    adapter: Option<&'static str>,
    focus: Option<String>,
    focus_en: Option<String>,
    focus_error: Option<&'static str>,
    fingerprint: String,
}

impl JrgTransform {
    pub fn new(config: SelectionConfig) -> Self {
        let selector = SelectorIdentity::resolve(&config.selector);
        Self {
            config,
            selector,
            memo: Mutex::new(Memo::default()),
        }
    }

    fn update_english_guidance(&self, body: &mut Value) {
        let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) else {
            return;
        };
        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            if !self
                .config
                .search_tools
                .iter()
                .chain(self.config.page_tools.iter())
                .any(|mapped| mapped == name)
            {
                continue;
            }
            let Some(field) = tool
                .get_mut("input_schema")
                .and_then(|schema| schema.get_mut("properties"))
                .and_then(|properties| properties.get_mut("jrg_focus_en"))
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            if field.get("type").and_then(Value::as_str) == Some("string") {
                field.insert("description".into(), json!(ENGLISH_FOCUS_DESCRIPTION));
            }
        }
    }

    fn origin(&self, block: &Value, user: &str) -> Origin {
        let name = block["name"].as_str().unwrap_or_default();
        let adapter = if self.config.search_tools.iter().any(|n| n == name) {
            Some("search-results")
        } else if self.config.page_tools.iter().any(|n| n == name) {
            Some("markdown-page")
        } else {
            None
        };
        let input = &block["input"];
        let focus = ["jrg_focus", "focus", "query", "q"]
            .iter()
            .find_map(|key| {
                input[*key]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| (!user.trim().is_empty()).then_some(user.trim()))
            .map(str::to_owned);
        let (focus_en, focus_error) = match input.get("jrg_focus_en") {
            None => (None, None),
            Some(Value::String(text)) if !text.trim().is_empty() => {
                let text = text.trim();
                if text.len() > MAX_FOCUS_BYTES {
                    (None, Some("focus_too_large"))
                } else {
                    (Some(text.to_owned()), None)
                }
            }
            Some(_) => (None, Some("invalid_jrg_focus_en")),
        };
        Origin {
            adapter,
            focus,
            focus_en,
            focus_error,
            fingerprint: digest(&block.to_string()),
        }
    }

    async fn selected(&self, origin: &Origin, content: &Value, scope: &str) -> Value {
        let selector = match &self.selector {
            Ok(selector) => selector,
            Err(error) => return selection_error(error),
        };
        if let Err(error) = selector.verify() {
            return selection_error(error);
        }
        let key = digest(
            &json!({"origin":origin.fingerprint, "focus":origin.focus,
            "focus_en":origin.focus_en, "focus_error":origin.focus_error,
            "source":content, "scope":scope, "adapter":origin.adapter,
            "max_tokens":self.config.max_tokens, "query_rules":QUERY_RULE_MODE,
            "selector_sha256":selector.sha256})
            .to_string(),
        );
        let cached = self
            .memo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values
            .get(&key)
            .map(|(value, _)| value.clone());
        if let Some(value) = cached {
            return value;
        }
        let outcome = self.select(origin, content, &selector.path).await;
        let value = match outcome {
            Ok(selection) => json!({"content":json!({
                "type":"jrg_evidence_selection",
                "notice":format!("{TRUNCATION_MARKER_TAG} Evidence selected for the originating tool call; omitted source is not included."),
                "selection":selection
            }).to_string(), "is_error":false}),
            Err(code) => selection_error(code),
        };
        self.memo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, value.clone());
        value
    }

    async fn select(
        &self,
        origin: &Origin,
        content: &Value,
        selector: &Path,
    ) -> Result<Value, &'static str> {
        if let Some(error) = origin.focus_error {
            return Err(error);
        }
        let focus = origin.focus.as_deref().ok_or("missing_focus")?;
        if let Some(english) = origin.focus_en.as_deref() {
            if focus.len().saturating_add(1).saturating_add(english.len()) > MAX_FOCUS_BYTES {
                return Err("focus_too_large");
            }
        }
        let raw = match content {
            Value::String(text) => text.as_str(),
            Value::Array(blocks) if blocks.len() == 1 && blocks[0]["type"] == "text" => {
                blocks[0]["text"].as_str().ok_or("unsupported_content")?
            }
            _ => return Err("unsupported_content"),
        };
        if raw.len() > MAX_SELECTOR_BYTES {
            return Err("input_too_large");
        }
        let input: Value = serde_json::from_str(raw).map_err(|_| "malformed_json")?;
        let mut envelope = json!({"query":focus,"max_tokens":self.config.max_tokens,"input":input});
        if let Some(english) = origin.focus_en.as_deref() {
            envelope["query_expansions"] = json!([english]);
        }
        let encoded = serde_json::to_vec(&envelope).map_err(|_| "invalid_input")?;
        if encoded.len() > MAX_SELECTOR_BYTES {
            return Err("input_too_large");
        }
        let mut command = Command::new(selector);
        command
            .args([
                "--query-rules",
                QUERY_RULE_MODE,
                "--adapter",
                origin.adapter.ok_or("unknown_tool")?,
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut process =
            SelectorProcess::spawn(&mut command).map_err(|_| "selector_unavailable")?;
        let execution = async {
            let mut stdin = process.child.stdin.take().ok_or("selector_io")?;
            let stdout = process.child.stdout.take().ok_or("selector_io")?;
            let write = async {
                stdin.write_all(&encoded).await.map_err(|_| "selector_io")?;
                stdin.shutdown().await.map_err(|_| "selector_io")?;
                drop(stdin);
                Ok::<_, &'static str>(())
            };
            let read = async {
                let mut bytes = Vec::new();
                stdout
                    .take((MAX_SELECTOR_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| "selector_io")?;
                if bytes.len() > MAX_SELECTOR_BYTES {
                    return Err("output_too_large");
                }
                Ok(bytes)
            };
            let (_, bytes) = tokio::try_join!(write, read)?;
            let status = process.wait().await.map_err(|error| {
                eprintln!(
                    "jrg selector wait failed (OS error {:?})",
                    error.raw_os_error()
                );
                "selector_io"
            })?;
            if !status.success() {
                return Err("selector_failed");
            }
            let selection: Value =
                serde_json::from_slice(&bytes).map_err(|_| "invalid_selector_response")?;
            validate_selection(&selection)?;
            Ok(selection)
        };
        let outcome = match tokio::time::timeout(self.config.timeout, execution).await {
            Ok(outcome) => outcome,
            Err(_) => Err("selector_timeout"),
        };
        if outcome.is_err() {
            process
                .terminate()
                .await
                .map_err(|_| "selector_cleanup_failed")?;
        }
        outcome
    }
}

#[async_trait::async_trait]
impl Transform for JrgTransform {
    fn name(&self) -> &'static str {
        "jrg-web-selection"
    }

    async fn on_request(&self, req: &mut InterceptedRequest) -> AppResult<()> {
        if req.provider != Provider::Anthropic || req.path != "/v1/messages" {
            return Ok(());
        }
        self.update_english_guidance(&mut req.body);
        let scope = json!([req.session, req.project, req.user_id, req.api_key_hash]).to_string();
        let Some(messages) = req.body["messages"].as_array_mut() else {
            return Ok(());
        };
        let mut origins: HashMap<String, Option<Origin>> = HashMap::new();
        let mut user = String::new();
        let mut removed = 0_i64;
        for message in messages {
            let role = message["role"].as_str().unwrap_or_default().to_owned();
            if role == "user" {
                let text = user_text(&message["content"]);
                if !text.trim().is_empty() {
                    user = text;
                }
            }
            let Some(blocks) = message["content"].as_array_mut() else {
                continue;
            };
            for block in blocks {
                if role == "assistant" && block["type"] == "tool_use" {
                    if let Some(id) = block["id"].as_str() {
                        let origin = self.origin(block, &user);
                        origins
                            .entry(id.to_owned())
                            .and_modify(|existing| *existing = None)
                            .or_insert(Some(origin));
                    }
                } else if role == "user" && block["type"] == "tool_result" {
                    let id = block["tool_use_id"].as_str().unwrap_or_default().to_owned();
                    let replacement = match origins.get(&id) {
                        Some(Some(origin)) if origin.adapter.is_none() => continue,
                        Some(Some(_)) if block["is_error"] == true => {
                            selection_error("original_tool_error")
                        }
                        Some(Some(origin)) => {
                            self.selected(origin, &block["content"], &scope).await
                        }
                        _ => selection_error("missing_or_ambiguous_origin"),
                    };
                    let before = rtk_proxy::tokenizer::count(&block.to_string());
                    // Reconstruct instead of preserving arbitrary fields that may mirror raw source.
                    *block = json!({"type":"tool_result", "tool_use_id":id,
                        "content":replacement["content"], "is_error":replacement["is_error"]});
                    let after = rtk_proxy::tokenizer::count(&block.to_string());
                    removed += before.saturating_sub(after) as i64;
                }
            }
        }
        req.record_saving(self.name(), removed);
        // Never return Err for configured-result failures: RTK falls back to raw on Err.
        Ok(())
    }
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn user_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn selection_error(code: &'static str) -> Value {
    json!({"content":format!("jrg evidence selection failed ({code}); raw tool source was not forwarded."), "is_error":true})
}

fn validate_selection(value: &Value) -> Result<(), &'static str> {
    let invalid = "invalid_selector_response";
    let object = value.as_object().ok_or(invalid)?;
    let allowed = [
        "documents",
        "omitted_count",
        "duplicate_count",
        "status",
        "tokens",
        "max_tokens",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str()))
        || !value["status"].is_string()
        || ["omitted_count", "duplicate_count", "tokens", "max_tokens"]
            .iter()
            .any(|key| !value[*key].is_u64())
    {
        return Err(invalid);
    }
    for doc in value["documents"].as_array().ok_or(invalid)? {
        let object = doc.as_object().ok_or(invalid)?;
        let allowed = ["id", "url", "title", "blocks", "source_hash"];
        if object.keys().any(|key| !allowed.contains(&key.as_str()))
            || ["id", "url", "title", "source_hash"]
                .iter()
                .any(|key| !doc[*key].is_string())
        {
            return Err(invalid);
        }
        for block in doc["blocks"].as_array().ok_or(invalid)? {
            let object = block.as_object().ok_or(invalid)?;
            let allowed = [
                "id",
                "kind",
                "text",
                "parent_id",
                "related_ids",
                "start_line",
                "end_line",
            ];
            if object.keys().any(|key| !allowed.contains(&key.as_str()))
                || ["id", "kind", "text"]
                    .iter()
                    .any(|key| !block[*key].is_string())
                || object.get("parent_id").is_some_and(|id| !id.is_string())
                || ["start_line", "end_line"]
                    .iter()
                    .any(|key| object.get(*key).is_some_and(|line| !line.is_u64()))
                || object.get("related_ids").is_some_and(|ids| {
                    ids.as_array()
                        .is_none_or(|ids| ids.iter().any(|id| !id.is_string()))
                })
            {
                return Err(invalid);
            }
        }
    }
    Ok(())
}

async fn anthropic_routes(req: Request, next: Next) -> Response {
    if (req.method() == Method::POST && req.uri().path() == "/v1/messages")
        || (req.method() == Method::GET && req.uri().path() == "/health")
    {
        next.run(req).await
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub fn build_router(config: SelectionConfig, upstream: String) -> axum::Router {
    let config_rtk = Config {
        bind: "127.0.0.1:8081".into(),
        anthropic_upstream: upstream,
        openai_upstream: "http://127.0.0.1:1".into(),
        ccr: false,
        cache_align: false,
        normalize: false,
        engine_methods: false,
        compression: "off".into(),
        pixel_compress: Default::default(),
        relay_borrowing: false,
        db: "off".into(),
        icm_db: "off".into(),
        icm_ttl_secs: 0,
        upstream_timeout_secs: 10,
        upstream_retries: 0,
    };
    let mut state = AppState::new(config_rtk);
    state.pipeline = Pipeline::new().with(Arc::new(JrgTransform::new(config)));
    rtk_proxy::server::build_router(state).layer(middleware::from_fn(anthropic_routes))
}
