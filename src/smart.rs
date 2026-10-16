use crate::{
    ai::AiOutput,
    cli::{Args, Color, Encoding},
    output::Output,
    search::{Event, Line, Search},
};
use std::{
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
};

pub fn rg_eligible(args: &Args, paths: &[PathBuf]) -> bool {
    (args.regex || args.no_normalize)
        && matches!(args.encoding, Encoding::Utf8)
        && !paths.iter().any(|p| p == Path::new("-"))
        && !args
            .pattern
            .iter()
            .chain(&args.patterns)
            .any(|p| p.contains('\n') || p.contains(r"\n"))
        && paths.iter().all(|p| {
            (p.is_file() || p.is_dir())
                && !p
                    .symlink_metadata()
                    .is_ok_and(|s| s.file_type().is_symlink())
        })
        && !args.column
        && !args.json
        && !args.count
        && !args.files_with_matches
        && !args.quiet
        && !args.only_matching
        && !args.invert_match
        && matches!(args.color, Color::Auto | Color::Never)
}

pub struct RgProcess {
    child: Child,
}

impl Drop for RgProcess {
    fn drop(&mut self) {
        // Also reap on setup/formatting errors after spawn, before produce runs.
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl RgProcess {
    pub fn spawn(args: &Args, patterns: &[String], paths: &[PathBuf]) -> io::Result<Self> {
        let mut command = Command::new("rg");
        // Neither user RG_CONFIG nor a parent environment's locale changes the query.
        command.args([
            "--no-config",
            "--json",
            "--color",
            "never",
            "--no-require-git",
            "--crlf",
            "--encoding",
            "utf-8",
        ]);
        if !args.regex {
            command.arg("--fixed-strings");
        }
        if args.ignore_case {
            command.arg("--ignore-case");
        }
        if args.smart_case {
            command.arg("--smart-case");
        }
        if args.hidden {
            command.arg("--hidden");
        }
        if args.no_ignore {
            command.arg("--no-ignore");
        }
        if args.text {
            command.arg("--text");
        }
        if let Some(n) = args.max_count {
            command.arg("--max-count").arg(n.to_string());
        }
        if args.threads > 0 {
            command.arg("--threads").arg(args.threads.to_string());
        }
        command
            .arg("--before-context")
            .arg(args.context.max(args.before_context).to_string());
        command
            .arg("--after-context")
            .arg(args.context.max(args.after_context).to_string());
        for glob in &args.globs {
            command.arg("--glob").arg(glob);
        }
        for pattern in patterns {
            command.arg("-e").arg(pattern);
        }
        command
            .arg("--")
            .args(paths)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        command.spawn().map(|child| Self { child })
    }

    pub fn produce(mut self, search: &Search<'_>) {
        let stdout = self.child.stdout.take().unwrap();
        let mut failed = false;
        for raw in BufReader::new(stdout).lines() {
            if search.stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            match raw.and_then(|line| self.decode(&line)) {
                Ok(Some(line)) => {
                    if line.selected {
                        search.emit(Event::Found);
                    }
                    search.emit(Event::Line(line));
                }
                Ok(None) => {}
                Err(err) => {
                    search.error(format!("rg output: {err}"));
                    failed = true;
                    break;
                }
            }
        }
        if failed || search.stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = self.child.kill();
        }
        match self.child.wait() {
            Ok(status) if !failed && !matches!(status.code(), Some(0 | 1)) => {
                search.error(format!("rg exited with {status}"))
            }
            Err(err) => search.error(format!("rg wait: {err}")),
            _ => {}
        }
    }

    fn decode(&self, raw: &str) -> io::Result<Option<Line>> {
        let value: serde_json::Value = serde_json::from_str(raw)?;
        let selected = match value["type"].as_str() {
            Some("match") => true,
            Some("context") => false,
            _ => return Ok(None),
        };
        let data = &value["data"];
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "rg returned non-UTF-8 or invalid match data",
            )
        };
        let path = data["path"]["text"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        let mut text = data["lines"]["text"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        let number = data["line_number"].as_u64().ok_or_else(invalid)?;
        let mut ranges = Vec::new();
        for sub in data["submatches"].as_array().ok_or_else(invalid)? {
            let start = usize::try_from(sub["start"].as_u64().ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            let end =
                usize::try_from(sub["end"].as_u64().ok_or_else(invalid)?).map_err(|_| invalid())?;
            if start > end
                || end > text.len()
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
            {
                return Err(invalid());
            }
            ranges.push(start..end);
        }
        Ok(Some(Line {
            path: Arc::new(path),
            number,
            text,
            ranges,
            selected,
        }))
    }
}

pub struct SmartOutput {
    ai: AiOutput,
    plain: Option<Vec<u8>>,
    omit_path: bool,
    byte_limit: usize,
}

impl SmartOutput {
    pub fn new(args: &Args, paths: &[PathBuf]) -> Self {
        Self {
            ai: AiOutput::new(args.snippet_chars as usize),
            plain: Some(Vec::new()),
            omit_path: paths.len() == 1 && paths[0].is_file(),
            byte_limit: (args.max_tokens as usize)
                .saturating_mul(128)
                .min(8 * 1024 * 1024),
        }
    }

    pub fn push(&mut self, args: &Args, line: Line) -> io::Result<()> {
        if let Some(buffer) = &mut self.plain {
            if self.omit_path && !line.text.chars().any(|c| c.is_control()) {
                use std::io::Write;
                writeln!(
                    buffer,
                    "{}{}{}",
                    line.number,
                    if line.selected { ':' } else { '-' },
                    line.text
                )?;
            } else {
                // Escape control bytes before displaying untrusted source, as AI mode does.
                let mut clean = Line {
                    path: line.path.clone(),
                    number: line.number,
                    text: line
                        .text
                        .chars()
                        .flat_map(|c| {
                            if c.is_control() {
                                c.escape_default().collect::<Vec<_>>()
                            } else {
                                vec![c]
                            }
                        })
                        .collect(),
                    ranges: Vec::new(),
                    selected: line.selected,
                };
                if clean.path.chars().any(char::is_control) {
                    clean.path = Arc::new(serde_json::to_string(&*clean.path)?);
                }
                Output {
                    args,
                    writer: &mut *buffer,
                    color: false,
                }
                .event(Event::Line(clean))?;
            }
            if buffer.len() > self.byte_limit {
                self.plain = None;
            }
        }
        self.ai.push(line);
        Ok(())
    }

    pub fn finish(self, tokenizer: &tiktoken_rs::CoreBPE, budget: usize) -> String {
        let (compact, complete) = self.ai.finish_smart(tokenizer, budget);
        if let Some(bytes) = self.plain {
            let plain = String::from_utf8(bytes).unwrap();
            let plain_tokens = tokenizer.encode_ordinary(&plain).len();
            // Prefer complete evidence if it fits; otherwise compare complete encodings.
            if plain_tokens <= budget
                && (!complete || plain_tokens <= tokenizer.encode_ordinary(&compact).len())
            {
                return plain;
            }
        }
        compact
    }
}
