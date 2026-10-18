use crate::{
    ai::{AiOutput, TrackedRows},
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
    plain_rows: TrackedRows,
    tracking_enabled: bool,
    omit_path: bool,
    byte_limit: usize,
}

impl SmartOutput {
    pub fn new(args: &Args, paths: &[PathBuf]) -> Self {
        Self {
            ai: AiOutput::new(args.snippet_chars as usize),
            plain: Some(Vec::new()),
            plain_rows: TrackedRows::default(),
            tracking_enabled: true,
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
            if self.tracking_enabled {
                self.plain_rows.add_row(&line.path, line.number, &line.text);
            }
            if buffer.len() > self.byte_limit {
                self.plain = None;
                self.plain_rows = TrackedRows::default();
            }
        }
        self.ai.push(line);
        Ok(())
    }

    pub fn disable_tracking(&mut self) {
        self.tracking_enabled = false;
        self.plain_rows = TrackedRows::default();
    }

    pub fn finish(self, tokenizer: &tiktoken_rs::CoreBPE, budget: usize) -> String {
        self.finish_impl(tokenizer, budget, false).0
    }

    pub fn finish_tracked(
        self,
        tokenizer: &tiktoken_rs::CoreBPE,
        budget: usize,
    ) -> (String, TrackedRows) {
        self.finish_impl(tokenizer, budget, true)
    }

    fn finish_impl(
        self,
        tokenizer: &tiktoken_rs::CoreBPE,
        budget: usize,
        track_rows: bool,
    ) -> (String, TrackedRows) {
        let track_rows = track_rows && self.tracking_enabled;
        let (compact, complete, compact_rows) = if track_rows {
            self.ai.finish_smart_tracked(tokenizer, budget)
        } else {
            let (text, complete) = self.ai.finish_smart(tokenizer, budget);
            (text, complete, TrackedRows::default())
        };
        let plain_rows = self.plain_rows;
        if let Some(bytes) = self.plain {
            let plain = String::from_utf8(bytes).unwrap();
            let plain_tokens = tokenizer.encode_ordinary(&plain).len();
            // Prefer complete evidence if it fits; otherwise compare complete encodings.
            if plain_tokens <= budget
                && (!complete || plain_tokens <= tokenizer.encode_ordinary(&compact).len())
            {
                return (
                    plain,
                    if track_rows {
                        plain_rows
                    } else {
                        TrackedRows::default()
                    },
                );
            }
        }
        (compact, compact_rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::sync::{Arc, OnceLock};
    use tiktoken_rs::CoreBPE;

    fn tokenizer() -> &'static CoreBPE {
        static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();
        TOKENIZER.get_or_init(|| tiktoken_rs::o200k_base().unwrap())
    }

    fn args() -> Args {
        let matches = Args::command()
            .try_get_matches_from(["jrg", "--smart", "--max-tokens", "512", "needle"])
            .unwrap();
        Args::prepared(&matches)
    }

    fn line(number: u64, text: &str) -> Line {
        Line {
            path: Arc::new("source.rs".into()),
            number,
            text: text.to_owned(),
            ranges: std::iter::once(0..text.len()).collect(),
            selected: true,
        }
    }

    #[test]
    fn tracked_plain_output_matches_legacy_and_returns_original_lines() {
        let args = args();
        let mut legacy = SmartOutput::new(&args, &[]);
        let mut tracked = SmartOutput::new(&args, &[]);
        for number in [2, 4] {
            legacy.push(&args, line(number, "same")).unwrap();
            tracked.push(&args, line(number, "same")).unwrap();
        }

        let expected = legacy.finish(tokenizer(), 512);
        let (actual, rows) = tracked.finish_tracked(tokenizer(), 512);

        assert_eq!(actual, expected);
        assert!(actual.contains("same"), "{actual}");
        assert_eq!(
            rows.rows.get("source.rs").unwrap(),
            &vec![(1, "same".to_owned()), (3, "same".to_owned())]
        );
        assert_eq!(rows.omitted, 0);
    }

    #[test]
    fn tracked_output_after_plain_buffer_drop_only_returns_rendered_rows() {
        let args = args();
        let mut legacy = SmartOutput::new(&args, &[]);
        let mut tracked = SmartOutput::new(&args, &[]);
        legacy.byte_limit = 1;
        tracked.byte_limit = 1;
        for number in 1..=40 {
            let text =
                format!("needle_{number:03} alpha_{number:03} beta_{number:03} gamma_{number:03}");
            legacy.push(&args, line(number, &text)).unwrap();
            tracked.push(&args, line(number, &text)).unwrap();
        }

        let expected = legacy.finish(tokenizer(), 64);
        let (actual, rows) = tracked.finish_tracked(tokenizer(), 64);

        assert_eq!(actual, expected);
        let rows = rows.rows.get("source.rs").unwrap();
        assert!(!rows.is_empty() && rows.len() < 40, "{actual}");
        for (number, original) in rows {
            assert!(
                actual.contains(&format!("{}:1:{original}", number + 1)),
                "{actual}"
            );
        }
    }

    #[test]
    fn plain_sidecar_caps_rows_and_reports_every_omitted_row() {
        let args = args();
        let mut output = SmartOutput::new(&args, &[]);
        output.byte_limit = 8 * 1024 * 1024;
        for number in 1..=9000 {
            output.push(&args, line(number * 2, "row")).unwrap();
        }

        let (rendered, tracked) = output.finish_tracked(tokenizer(), 100_000);

        assert_eq!(rendered.matches("row").count(), 9000);
        assert_eq!(tracked.rows.get("source.rs").unwrap().len(), 8192);
        assert_eq!(tracked.omitted, 808);
    }

    #[test]
    fn plain_fallback_discards_sidecar_counts_before_compact_tracking() {
        let args = args();
        let mut output = SmartOutput::new(&args, &[]);
        output.byte_limit = 8 * 1024 * 1024;
        for number in 1..=8195 {
            output.push(&args, line(number, "same")).unwrap();
        }
        assert_eq!(output.plain_rows.omitted, 3);

        output.byte_limit = 0;
        output.push(&args, line(8196, "same")).unwrap();
        assert!(output.plain.is_none());
        assert!(output.plain_rows.rows.is_empty());
        assert_eq!(output.plain_rows.omitted, 0);

        let (_, tracked) = output.finish_tracked(tokenizer(), 64);

        assert_eq!(tracked.rows.get("source.rs").unwrap().len(), 8192);
        assert_eq!(tracked.omitted, 4);
    }

    #[test]
    fn disabling_tracking_preserves_text_without_collecting_rows() {
        let args = args();
        let mut expected = SmartOutput::new(&args, &[]);
        let mut disabled = SmartOutput::new(&args, &[]);
        disabled.disable_tracking();
        for number in [2, 4] {
            expected.push(&args, line(number, "same")).unwrap();
            disabled.push(&args, line(number, "same")).unwrap();
        }

        let expected = expected.finish(tokenizer(), 512);
        let (actual, tracked) = disabled.finish_tracked(tokenizer(), 512);

        assert_eq!(actual, expected);
        assert!(tracked.rows.is_empty());
        assert_eq!(tracked.omitted, 0);
    }
}
