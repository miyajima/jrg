//! Local stdio MCP transport. No shell, network, arbitrary argv, or source writes.
use crate::cli::Args;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    error::Error,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

pub fn serve(args: &Args) -> Result<u8, Box<dyn Error>> {
    Server::new(args)?.run()?;
    Ok(0)
}
struct Server {
    args: Args,
    root: PathBuf,
    initialized: bool,
    ready: bool,
}
struct Reply<'a> {
    output: &'a mut dyn Write,
    id: Value,
    body: Vec<u8>,
    sent: bool,
    metadata: Rc<RefCell<Option<Value>>>,
}
impl Write for Reply<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.body.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if !self.sent {
            let text = std::str::from_utf8(&self.body).map_err(io::Error::other)?;
            let mut result = json!({"content":[{"type":"text","text":text}],"isError":false});
            if let Some(metadata) = self.metadata.borrow().as_ref() {
                result["structuredContent"] = metadata.clone();
            }
            serde_json::to_writer(
                &mut self.output,
                &json!({"jsonrpc":"2.0","id":self.id,"result":result}),
            )?;
            self.output.write_all(b"\n")?;
            self.output.flush()?;
            self.sent = true;
        }
        Ok(())
    }
}
impl Server {
    fn new(args: &Args) -> io::Result<Self> {
        let root = args.root.as_ref().unwrap().canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::other("MCP root must be a directory"));
        }
        let mut args = args.clone();
        if args.context_id.is_none() {
            args.context_id = Some(crate::history::new_id());
        }
        if let Some(path) = &args.session {
            args.session = Some(std::path::absolute(path)?);
        }
        args.mcp = false;
        args.root = None;
        args.smart = true;
        args.batch = Some("-".into());
        args.max_tokens = 24000;
        std::env::set_current_dir(&root)?;
        Ok(Self {
            args,
            root,
            initialized: false,
            ready: false,
        })
    }
    fn keys(value: &Value, allowed: &[&str]) -> Result<(), String> {
        let object = value.as_object().ok_or("expected an object")?;
        let unknown: Vec<_> = object
            .keys()
            .filter(|key| !allowed.contains(&key.as_str()))
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "unknown argument(s): {unknown:?}. Allowed arguments: {}. Read tools/list for the request shape; retrieve uses operations containing search or read, not a top-level query.",
                allowed.join(", ")
            ));
        }
        Ok(())
    }
    fn path(&self, value: &Value, field: &str) -> Result<String, String> {
        let path = value.as_str().ok_or("path must be a string")?;
        if path == "-" {
            return Err("stdin is not a source path".into());
        }
        let canonical = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|e| {
                let hint = if field == "read.path" {
                    "Locate an existing file using search.paths with a known parent directory and optional search.globs, then use its returned path in read.path."
                } else {
                    "Use an existing parent directory in search.paths and filename patterns in search.globs."
                };
                format!("invalid {field} {path:?} relative to configured MCP root {:?}: {e}. {hint} Do not retry the same missing path; use permitted native tools if the required path is outside this root.", self.root)
            })?;
        if !canonical.starts_with(&self.root) {
            return Err(format!(
                "path escapes configured MCP root {:?}: {path:?}. Use permitted native tools for paths outside this root.",
                self.root
            ));
        }
        if !canonical.is_file() && !canonical.is_dir() {
            return Err("path must be a regular file or directory".into());
        }
        // Keep relative paths in evidence; canonicalization is only a boundary check.
        Ok(if Path::new(path).is_absolute() {
            canonical.to_string_lossy().into_owned()
        } else {
            path.to_owned()
        })
    }
    fn string_array(value: &Value, field: &str) -> Result<Vec<String>, String> {
        let values = value
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 64)
            .ok_or_else(|| {
                let hint = match field {
                    "search.paths" => "Omit paths to use the configured root, or supply an existing file or directory; use globs for filename patterns.",
                    "search.globs" => "Omit globs when no filename filter is needed.",
                    _ => "Supply at least one literal search string in patterns.",
                };
                format!("{field} must contain 1..64 strings. {hint}")
            })?;
        values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{field} entries must be strings"))
            })
            .collect()
    }
    fn plan(&self, input: &Value) -> Result<(Args, Value), String> {
        Self::keys(
            input,
            &[
                "operations",
                "max_tokens",
                "repeat",
                "resume",
                "rank_query",
                "discovery_limit",
                "requirements",
                "history_mode",
                "context_id",
            ],
        )?;
        let mut args = self.args.clone();
        if let Some(mode) = input.get("history_mode") {
            let requested = match mode.as_str() {
                Some("auto") => crate::history::Mode::Auto,
                Some("record") => crate::history::Mode::Record,
                Some("off") => crate::history::Mode::Off,
                _ => return Err("history_mode must be auto, record or off".into()),
            };
            args.history_mode = match args.history_mode {
                crate::history::Mode::Off => crate::history::Mode::Off,
                crate::history::Mode::Record if requested == crate::history::Mode::Auto => {
                    crate::history::Mode::Record
                }
                _ => requested,
            };
        }
        if let Some(context) = input.get("context_id") {
            let context = context
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
                .ok_or("context_id must be 1..128 non-control bytes")?;
            args.context_id = Some(context.to_owned());
        }
        if let Some(query) = input.get("rank_query") {
            let query = query.as_str().ok_or("rank_query must be a string")?;
            if crate::rank::tokens(query).is_empty() {
                return Err("rank_query needs at least one searchable term".into());
            }
            args.rank_query = Some(query.to_owned());
        }
        if let Some(n) = input.get("discovery_limit") {
            if args.rank_query.is_none() {
                return Err("discovery_limit needs rank_query".into());
            }
            args.discovery_limit = Some(
                n.as_u64()
                    .filter(|n| *n <= 256)
                    .ok_or("discovery_limit must be 0..256")? as u16,
            );
        }
        if let Some(n) = input.get("max_tokens") {
            args.max_tokens = n
                .as_u64()
                .filter(|n| *n >= 64 && *n <= 26000)
                .ok_or("max_tokens must be 64..26000")? as u32;
        }
        for (key, target) in [("repeat", &mut args.repeat), ("resume", &mut args.resume)] {
            if let Some(v) = input.get(key) {
                *target = v.as_bool().ok_or("expected boolean")?;
            }
        }
        if args.repeat && args.resume {
            return Err("repeat and resume conflict".into());
        }
        crate::requirements::Requirements::parse(
            input.get("requirements"),
            args.max_tokens as usize,
        )
        .map_err(|e| e.to_string())?;
        if args.resume && input.get("requirements").is_some() {
            return Err("requirements needs explicit operations; repeat the scoped operation to resurface conditions".into());
        }
        if (args.repeat || args.resume) && args.session.is_none() {
            return Err("repeat/resume needs server --session".into());
        }
        let operations = input
            .get("operations")
            .and_then(Value::as_array)
            .ok_or(r#"operations array required; use {"operations":[{"search":{"patterns":["identifier"],"paths":["."],"intent":"locate"}}]}. query is not a top-level argument."#)?;
        if operations.len() > 64
            || (operations.is_empty() && !args.resume)
            || (args.resume && !operations.is_empty())
        {
            return Err("use 1..64 operations, or empty operations with resume".into());
        }
        let mut plan = Vec::new();
        for op in operations {
            Self::keys(op, &["search", "read"])?;
            if op.as_object().unwrap().len() != 1 {
                return Err("one search or read per operation".into());
            }
            if let Some(read) = op.get("read") {
                Self::keys(read, &["path", "start", "end"])?;
                let path = self.path(&read["path"], "read.path")?;
                let start = read["start"]
                    .as_u64()
                    .filter(|n| *n > 0)
                    .ok_or("start must be a positive line number")?;
                let end = read["end"]
                    .as_u64()
                    .filter(|n| *n >= start)
                    .ok_or("end must be >= start")?;
                plan.push(json!({"read":{"path":path,"start":start,"end":end}}));
            } else {
                let search = &op["search"];
                Self::keys(
                    search,
                    &[
                        "patterns",
                        "paths",
                        "intent",
                        "regex",
                        "case_insensitive",
                        "context",
                        "full_scope",
                        "globs",
                    ],
                )?;
                let patterns = Self::string_array(&search["patterns"], "search.patterns")?;
                let paths = if let Some(paths) = search.get("paths") {
                    Self::string_array(paths, "search.paths")?
                } else {
                    vec![".".into()]
                };
                let mut argv: Vec<String> = Vec::new();
                for (key, flag) in [
                    ("regex", "--regex"),
                    ("case_insensitive", "--ignore-case"),
                    ("full_scope", "--explain-scope"),
                ] {
                    if let Some(value) = search.get(key)
                        && value.as_bool().ok_or("expected boolean")?
                    {
                        argv.push(flag.into());
                        if key == "full_scope" {
                            argv.push("full".into());
                        }
                    }
                }
                if let Some(intent) = search.get("intent") {
                    let intent = intent
                        .as_str()
                        .filter(|v| ["locate", "explain", "review"].contains(v))
                        .ok_or("intent must be locate, explain or review")?;
                    argv.extend(["--intent".into(), intent.into()]);
                }
                if let Some(n) = search.get("context") {
                    let n = n
                        .as_u64()
                        .filter(|n| *n <= 128)
                        .ok_or("context must be 0..128")?;
                    // Review chooses complete scopes; a line-context hint cannot change them.
                    if search["intent"] != "review" {
                        argv.extend(["--context".into(), n.to_string()]);
                    }
                }
                if let Some(globs) = search.get("globs") {
                    for glob in Self::string_array(globs, "search.globs")? {
                        argv.extend(["--glob".into(), glob]);
                    }
                }
                for pattern in patterns {
                    argv.push(format!("--regexp={pattern}"));
                }
                argv.push("--".into());
                for path in paths {
                    argv.push(self.path(&json!(path), "search.paths")?);
                }
                plan.push(json!({"search":argv}));
            }
        }
        let mut batch = json!({"operations":plan});
        if let Some(requirements) = input.get("requirements") {
            batch["requirements"] = requirements.clone();
        }
        Ok((args, batch))
    }
    fn history_call(&self, tool: &str, input: &Value) -> Result<Value, String> {
        if crate::history::effective_mode(self.args.history_mode) == crate::history::Mode::Off {
            return Err("history is disabled for this server".into());
        }
        let repo = crate::history::Repo::discover(&self.root).map_err(|e| e.to_string())?;
        let mut store = crate::history::Store::open(&repo).map_err(|e| e.to_string())?;
        let allowed = |record: &Value| -> bool {
            let Some(paths) = record["conditions"]["requested_paths"]
                .as_array()
                .filter(|ps| !ps.is_empty())
            else {
                return false;
            };
            let path_allowed = |path: &str| {
                let path = Path::new(path);
                if path.is_absolute()
                    || path
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return false;
                }
                let joined = repo.root.join(path);
                joined
                    .canonicalize()
                    .unwrap_or(joined)
                    .starts_with(&self.root)
            };
            if !paths.iter().all(|p| p.as_str().is_some_and(path_allowed)) {
                return false;
            }
            let Some(results) = record["results"].as_array() else {
                return false;
            };
            results.iter().all(|r| {
                let Some(path) = r["path"].as_str() else {
                    return false;
                };
                let path = Path::new(path);
                if path.is_absolute()
                    || path
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return false;
                }
                let joined = repo.root.join(path);
                joined
                    .canonicalize()
                    .unwrap_or(joined)
                    .starts_with(&self.root)
            })
        };
        let value = if tool == "feedback" {
            Self::keys(
                input,
                &["request_id", "result_id", "label", "event_id", "context_id"],
            )?;
            input["event_id"]
                .as_str()
                .ok_or("event_id required for retry-safe feedback")?;
            let id = input["request_id"].as_str().ok_or("request_id required")?;
            let record = store.show(id).map_err(|e| e.to_string())?;
            if !allowed(&record["request"]) {
                return Err("request is outside configured root".into());
            }
            let mut input = input.clone();
            input["source"] = "agent".into();
            store.feedback(&input).map_err(|e| e.to_string())?
        } else {
            Self::keys(input, &["action", "request_id", "limit"])?;
            match input["action"].as_str() {
                Some("show") => {
                    let id = input["request_id"].as_str().ok_or("request_id required")?;
                    let value = store.show(id).map_err(|e| e.to_string())?;
                    if !allowed(&value["request"]) {
                        return Err("request is outside configured root".into());
                    }
                    value
                }
                Some("list" | "status") => {
                    let limit = match input.get("limit") {
                        None => 20,
                        Some(v) => {
                            v.as_u64()
                                .filter(|n| *n > 0 && *n <= 100)
                                .ok_or("limit must be 1..100")? as usize
                        }
                    };
                    let list = store.list(100).map_err(|e| e.to_string())?;
                    let mut visible = Vec::new();
                    for item in list["requests"]
                        .as_array()
                        .ok_or("invalid stored history")?
                    {
                        let Some(id) = item["request_id"].as_str() else {
                            continue;
                        };
                        if store
                            .show(id)
                            .ok()
                            .is_some_and(|record| allowed(&record["request"]))
                        {
                            visible.push(item.clone());
                        }
                    }
                    let count = visible.len();
                    visible.truncate(limit);
                    json!({"schema_version":1,"requests":visible,"visible_recent_requests":count,"scan_limit":100})
                }
                _ => return Err("action must be list, show or status".into()),
            }
        };
        let encoded = value.to_string();
        if encoded.len() > 48_000
            || tiktoken_rs::o200k_base_singleton()
                .encode_ordinary(&encoded)
                .len()
                > 12_000
        {
            return Err("history response exceeds budget; use CLI history export".into());
        }
        Ok(value)
    }
    fn schema(&self) -> Value {
        let search = json!({"type":"object","additionalProperties":false,"required":["patterns"],"properties":{
            "patterns":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"},"description":"Required literal source strings; use regex=true for regular expressions. Not a top-level query."},"paths":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"},"description":"Existing files/directories relative to the configured MCP root. Omit for the root; never send an empty array or guessed filename. Filename filters belong in globs."},
            "intent":{"type":"string","enum":["locate","explain","review"]},"regex":{"type":"boolean"},"case_insensitive":{"type":"boolean"},"full_scope":{"type":"boolean"},"context":{"type":"integer","minimum":0,"maximum":128,"description":"Extra lines for locate/explain. Omit for review: valid values are accepted but ignored because review selects source scopes."},"globs":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"}}}});
        let read = json!({"type":"object","additionalProperties":false,"required":["path","start","end"],"properties":{"path":{"type":"string"},"start":{"type":"integer","minimum":1},"end":{"type":"integer","minimum":1}}});
        let requirement = json!({"type":"object","additionalProperties":false,"required":["id","description","patterns"],"properties":{
            "id":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,32}$"},"description":{"type":"string","minLength":1,"maxLength":160},
            "patterns":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string","minLength":1,"maxLength":128}}}});
        let mut schema = json!({"tools":[{"name":"retrieve","description":"Prefer jrg retrieve for source investigation and bounded reads within the configured root. Keep native rg for exhaustive enumeration, absence checks, exact counts, machine output, pipelines and unsupported options; fall back when jrg is unavailable. Call with operations:[{search:{patterns:[\"identifier\"],paths:[\".\"],intent:\"locate\"}}], never a top-level query. Paths must already exist; omit paths for the configured root, use globs for filename filters. review accepts but ignores context. Batch independent source searches and line reads. For multi-item requests, supply outer requirements with stable IDs, descriptions and case-sensitive original-source literal patterns. Returns item/evidence links plus whole if/while predicates before ordinary source; answer every item and preserve all linked clauses. matched means literal evidence, not verified semantic coverage. Check unresolved, conditions_incomplete, omitted and syntax=unavailable; do not infer absence or a passed contract. Requirements only annotate selected source, never expand retrieval or invoke models. Reissue scoped operations to resurface conditions even when seen. Fixed root; no shell. patterns are literal by default; regex=true searches original text, default normalizes Japanese. review for source investigations: BM25 ranks query-focused previews and production definition names, focuses the leading module family for broad searches, then bundles functions/guards/helpers/tests. Explicit paths override family focus; other families have hints. No guessed broad reads needed. Scope paths when known; root paths are valid. Independent review searches may share one call; scope slots are shared across the batch (source8 plus up to2 small sibling JSON definitions/tests6/helpers8/constants16), so combine overlapping subjects in one search. Generic stage/verify names do not expand root tests; test-only searches stay scoped. JSON/schema investigations also include small sibling serialization definitions and tests asserting their field keys. Document review returns matching headings/sections or local contexts directly, no locate retry needed. Literal patterns starting with - are values. Heuristic links and omitted file/scope hints are explicit; not semantic test/dependency closure. For review, omit context; a supplied valid context is ignored because review selects scopes. Use returned evidence/paths/line numbers; follow concrete gaps with narrow explain/read, never guess a file path. Small known modules may be read with their tests directly. locate for discovery; explain for named definitions/guards/tests. Explain keeps matched definitions, short caller contexts; full_scope=true or read for needed caller bodies. Shared session suppresses already delivered rows (seen); empty operations with resume retrieves pending rows; repeat only when necessary. Default24000 tokens, max26000, with96000-byte transport ceiling before seen tracking. scope_limited is not pending. Batch implementation and related tests when paths are known. Optional outer rank_query enables BM25. Broad locate searches over directories or multiple files return top12 novel passages per operation (discovery_limit overrides; 0 disables). Single-file locate, explain functions and explicit read ranges remain uncapped and are packed first. Batch independent known functions/guards/tests with their related reads. Unselected range hints are discovery omissions, not pending; retrieve only needed hints narrowly. Seen rows are excluded before selecting top candidates. English identifiers work for code. review optionally supplements lexical candidates with pinned local EmbeddingGemma 2 over a configured Unix socket; model/service failures explicitly fall back to lexical. ast-grep parses Rust/TS/JS/Python scopes, with labeled lexical fallback for invalid or unsupported syntax. Lexical locate/explain stay exact. Candidate ranking cannot prove absence. Errors/omissions are explicit in footer.","inputSchema":{"type":"object","additionalProperties":false,"required":["operations"],"properties":{"operations":{"type":"array","maxItems":64,"items":{"oneOf":[{"type":"object","required":["search"],"additionalProperties":false,"properties":{"search":search}},{"type":"object","required":["read"],"additionalProperties":false,"properties":{"read":read}}]}},"requirements":{"type":"array","minItems":1,"maxItems":16,"items":requirement,"description":"Requested item checklist with original-source case-sensitive literal patterns (not regex or normalized). Explicit operations only; statuses must fit budget or request fails before execution."},"rank_query":{"type":"string","description":"Optional BM25 relevance query for this entire batch, not the retrieval pattern. Needs a searchable term."},"discovery_limit":{"type":"integer","minimum":0,"maximum":256,"description":"Per broad locate operation BM25 top passages, default12; 0 disables cutoff. Never limits read or explain."},"max_tokens":{"type":"integer","minimum":64,"maximum":26000},"repeat":{"type":"boolean"},"resume":{"type":"boolean"}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]});
        schema["tools"][0]["inputSchema"]["examples"] = json!([
            {"operations":[{"search":{"intent":"locate","patterns":["identifier"],"paths":["."]}}]},
            {"operations":[{"search":{"intent":"review","patterns":["identifier"],"paths":["src"]}}]},
            {"operations":[{"read":{"path":"src/lib.rs","start":1,"end":1}}]}
        ]);
        schema["tools"][0]["inputSchema"]["properties"]["history_mode"] =
            json!({"type":"string","enum":["auto","record","off"]});
        schema["tools"][0]["inputSchema"]["properties"]["context_id"] =
            json!({"type":"string","minLength":1,"maxLength":128});
        let tools = schema["tools"].as_array_mut().unwrap();
        tools.push(json!({"name":"feedback","description":"Report helpful/not_helpful/unknown for an emitted jrg result. This records an agent report, never a human confirmation. Retry with the same event_id for idempotence.","inputSchema":{"type":"object","additionalProperties":false,"required":["request_id","result_id","label","event_id"],"properties":{"request_id":{"type":"string"},"result_id":{"type":"string"},"label":{"type":"string","enum":["helpful","not_helpful","unknown"]},"event_id":{"type":"string"},"context_id":{"type":"string"}}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
        tools.push(json!({"name":"history","description":"Inspect local search history within the configured root. No source bodies are stored. show requires request_id.","inputSchema":{"type":"object","additionalProperties":false,"required":["action"],"properties":{"action":{"type":"string","enum":["list","show","status"]},"request_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}));
        schema
    }
    fn response(&mut self, request: &Value, output: &mut dyn Write) -> io::Result<()> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request["method"].as_str().unwrap_or("");
        if request.get("id").is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return Ok(());
        }
        let result = if request["jsonrpc"] != "2.0" || !id.is_string() && !id.is_number() {
            Err((-32600, "invalid request"))
        } else if method == "initialize" {
            self.initialized = true;
            let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
            let version =
                if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"].contains(&requested) {
                    requested
                } else {
                    "2025-11-25"
                };
            Ok(
                json!({"protocolVersion":version,"capabilities":{"tools":{}},"instructions":format!("{}\nConfigured MCP root: {:?}. Relative paths resolve from this root, not another project or the client working directory.\n", include_str!("mcp_instructions.txt"), self.root),"serverInfo":{"name":"jrg","version":env!("CARGO_PKG_VERSION")}}),
            )
        } else if method == "ping" {
            Ok(json!({}))
        } else if !self.ready {
            Err((
                -32000,
                "initialize first: send initialize, then notifications/initialized before tools/list or tools/call",
            ))
        } else if method == "tools/list" {
            Ok(self.schema())
        } else if method == "tools/call"
            && matches!(
                request["params"]["name"].as_str(),
                Some("history" | "feedback")
            )
        {
            let params = &request["params"];
            let response =
                self.history_call(params["name"].as_str().unwrap(), &params["arguments"]);
            Ok(match response {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                }
                Err(error) => json!({"content":[{"type":"text","text":error}],"isError":true}),
            })
        } else if method == "tools/call" {
            let params = &request["params"];
            let plan = if params["name"] != "retrieve" {
                Err("unknown tool".into())
            } else {
                self.plan(&params["arguments"])
            };
            match plan {
                Err(message) => {
                    Ok(json!({"content":[{"type":"text","text":message}],"isError":true}))
                }
                Ok((args, value)) => {
                    let metadata = Rc::new(RefCell::new(None));
                    let callback_metadata = metadata.clone();
                    let mut callback = move |value| {
                        *callback_metadata.borrow_mut() = Some(value);
                    };
                    let mut reply = Reply {
                        output,
                        id: id.clone(),
                        body: Vec::new(),
                        sent: false,
                        metadata,
                    };
                    let result = crate::batch::execute(
                        &args,
                        if args.resume { None } else { Some(value) },
                        &mut reply,
                        Some(&mut callback),
                    );
                    if reply.sent {
                        if let Err(e) = result {
                            eprintln!("jrg MCP session: {e}");
                        }
                        return Ok(());
                    }
                    match result {
                        Ok(_) => return Ok(()),
                        Err(e) => Ok(
                            json!({"content":[{"type":"text","text":e.to_string()}],"isError":true}),
                        ),
                    }
                }
            }
        } else {
            Err((-32601, "method not found"))
        };
        let response = match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        };
        serde_json::to_writer(&mut *output, &response)?;
        output.write_all(b"\n")?;
        output.flush()
    }
    fn run(&mut self) -> io::Result<()> {
        let mut input = io::stdin().lock();
        let mut output = io::stdout().lock();
        loop {
            let mut line = Vec::new();
            let n = (&mut input)
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            if line.len() > 1024 * 1024 {
                return Err(io::Error::other("MCP request exceeds 1 MiB"));
            }
            match serde_json::from_slice(&line) {
                Ok(value) => self.response(&value, &mut output)?,
                Err(_) => {
                    serde_json::to_writer(
                        &mut output,
                        &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}),
                    )?;
                    output.write_all(b"\n")?;
                    output.flush()?;
                }
            }
        }
        Ok(())
    }
}
