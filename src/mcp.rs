//! Local stdio MCP transport. No shell, network, arbitrary argv, or source writes.
use crate::cli::Args;
use serde_json::{Value, json};
use std::{
    error::Error,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
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
}
impl Write for Reply<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.body.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if !self.sent {
            let text = std::str::from_utf8(&self.body).map_err(io::Error::other)?;
            serde_json::to_writer(
                &mut self.output,
                &json!({"jsonrpc":"2.0","id":self.id,"result":{"content":[{"type":"text","text":text}],"isError":false}}),
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
        if object.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err("unknown argument".into());
        }
        Ok(())
    }
    fn path(&self, value: &Value) -> Result<String, String> {
        let path = value.as_str().ok_or("path must be a string")?;
        if path == "-" {
            return Err("stdin is not a source path".into());
        }
        let canonical = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|e| format!("invalid path: {e}"))?;
        if !canonical.starts_with(&self.root) {
            return Err("path escapes configured MCP root".into());
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
    fn string_array(value: &Value) -> Result<Vec<String>, String> {
        let values = value
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 64)
            .ok_or("expected 1..64 strings")?;
        values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "expected string".into())
            })
            .collect()
    }
    fn plan(&self, input: &Value) -> Result<(Args, Value), String> {
        Self::keys(input, &["operations", "max_tokens", "repeat", "resume"])?;
        let mut args = self.args.clone();
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
        if (args.repeat || args.resume) && args.session.is_none() {
            return Err("repeat/resume needs server --session".into());
        }
        let operations = input
            .get("operations")
            .and_then(Value::as_array)
            .ok_or("operations array required")?;
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
                let path = self.path(&read["path"])?;
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
                let patterns = Self::string_array(&search["patterns"])?;
                let paths = if let Some(paths) = search.get("paths") {
                    Self::string_array(paths)?
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
                        .filter(|v| ["locate", "explain"].contains(v))
                        .ok_or("intent must be locate or explain")?;
                    argv.extend(["--intent".into(), intent.into()]);
                }
                if let Some(n) = search.get("context") {
                    argv.extend([
                        "--context".into(),
                        n.as_u64()
                            .filter(|n| *n <= 128)
                            .ok_or("context must be 0..128")?
                            .to_string(),
                    ]);
                }
                if let Some(globs) = search.get("globs") {
                    for glob in Self::string_array(globs)? {
                        argv.extend(["--glob".into(), glob]);
                    }
                }
                for pattern in patterns {
                    argv.extend(["-e".into(), pattern]);
                }
                argv.push("--".into());
                for path in paths {
                    argv.push(self.path(&json!(path))?);
                }
                plan.push(json!({"search":argv}));
            }
        }
        Ok((args, json!({"operations":plan})))
    }
    fn schema(&self) -> Value {
        let search = json!({"type":"object","additionalProperties":false,"required":["patterns"],"properties":{
            "patterns":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"}},"paths":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"}},
            "intent":{"type":"string","enum":["locate","explain"]},"regex":{"type":"boolean"},"case_insensitive":{"type":"boolean"},"full_scope":{"type":"boolean"},"context":{"type":"integer","minimum":0,"maximum":128},"globs":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string"}}}});
        let read = json!({"type":"object","additionalProperties":false,"required":["path","start","end"],"properties":{"path":{"type":"string"},"start":{"type":"integer","minimum":1},"end":{"type":"integer","minimum":1}}});
        json!({"tools":[{"name":"retrieve","description":"Batch independent source searches and line reads. Fixed root; no shell. patterns are literal by default; regex=true searches original text, default normalizes Japanese. locate for discovery; explain for named definitions/guards/tests. Explain keeps matched definitions, short caller contexts; full_scope=true or read for needed caller bodies. Shared session suppresses already delivered rows (seen); empty operations with resume retrieves pending rows; repeat only when necessary. Default 24000 tokens, max26000. scope_limited is not pending. Batch implementation and related tests when paths are known. Errors/omissions are explicit in footer.","inputSchema":{"type":"object","additionalProperties":false,"required":["operations"],"properties":{"operations":{"type":"array","maxItems":64,"items":{"oneOf":[{"type":"object","required":["search"],"additionalProperties":false,"properties":{"search":search}},{"type":"object","required":["read"],"additionalProperties":false,"properties":{"read":read}}]}},"max_tokens":{"type":"integer","minimum":64,"maximum":26000},"repeat":{"type":"boolean"},"resume":{"type":"boolean"}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}}]})
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
                json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"jrg","version":env!("CARGO_PKG_VERSION")}}),
            )
        } else if method == "ping" {
            Ok(json!({}))
        } else if !self.ready {
            Err((-32000, "initialize first"))
        } else if method == "tools/list" {
            Ok(self.schema())
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
                    let mut reply = Reply {
                        output,
                        id: id.clone(),
                        body: Vec::new(),
                        sent: false,
                    };
                    let result = crate::batch::execute(
                        &args,
                        if args.resume { None } else { Some(value) },
                        &mut reply,
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
