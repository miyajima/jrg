//! Bounded AST scopes with explicit lexical fallback; no semantic closure guarantee.
use crate::{
    cli::{Args, Encoding, ExplainScope, Intent},
    search::Line,
};
use regex::Regex;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    ops::Range,
    path::Path,
    sync::OnceLock,
};

const MAX_SOURCE: usize = 4 * 1024 * 1024;
const MAX_CAPTURE: usize = 8 * 1024 * 1024;
const MAX_BLOCK: usize = 256;
const SCOPE: u8 = 0;
const HIT: u8 = 1;
const READ: u8 = 2;
const DISCOVERY: u8 = 3;

#[derive(Default)]
pub struct Capture {
    pub lines: Vec<Line>,
    pub omitted: usize,
    bytes: usize,
    preview: bool,
    preview_counts: BTreeMap<String, usize>,
    preview_query: BTreeSet<String>,
    preview_slots: BTreeMap<String, Vec<(usize, usize)>>,
    preview_omitted: usize,
}
impl Capture {
    pub fn preview() -> Self {
        Self {
            preview: true,
            ..Self::default()
        }
    }
    pub fn preview_for(query: &str) -> Self {
        Self {
            preview_query: crate::rank::tokens(query).into_iter().collect(),
            ..Self::preview()
        }
    }
    pub fn push(&mut self, line: Line) {
        let score = if self.preview && line.selected {
            crate::rank::tokens(&line.text)
                .into_iter()
                .collect::<BTreeSet<_>>()
                .intersection(&self.preview_query)
                .count()
        } else {
            0
        };
        if self.preview {
            let count = self.preview_counts.entry((*line.path).clone()).or_default();
            if *count >= 16 {
                self.preview_omitted += 1;
                if let Some(slots) = self.preview_slots.get_mut(&*line.path)
                    && let Some((slot, old_score)) =
                        slots.iter_mut().min_by_key(|(_, score)| *score)
                    && score > *old_score
                {
                    let new_bytes = self
                        .bytes
                        .saturating_sub(self.lines[*slot].text.len())
                        .saturating_add(line.text.len());
                    if new_bytes <= MAX_CAPTURE {
                        self.bytes = new_bytes;
                        self.lines[*slot] = line;
                        *old_score = score;
                    }
                }
                return;
            }
            *count += 1;
        }
        if self.lines.len() >= 4096 || self.bytes.saturating_add(line.text.len()) > MAX_CAPTURE {
            self.omitted += 1;
        } else {
            self.bytes += line.text.len();
            if self.preview {
                self.preview_slots
                    .entry((*line.path).clone())
                    .or_default()
                    .push((self.lines.len(), score));
            }
            self.lines.push(line);
        }
    }
}

#[derive(Clone)]
struct Scope {
    start: usize,
    open: usize,
    end: usize,
    function: bool,
}

fn declaration(line: &str, rust: bool) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    if line.split('{').next().unwrap_or(line).contains(';') || line.trim_start().starts_with("//") {
        return false;
    }
    if rust {
        static RUST: OnceLock<Regex> = OnceLock::new();
        return RUST
            .get_or_init(|| Regex::new(r"\bfn\s+\w+").unwrap())
            .is_match(line);
    }
    RE.get_or_init(|| Regex::new(r"(?:\bfn\s+\w+|\bfunction\b|\b(?:const|let|var)\s+\w+.*=>|^\s*(?:(?:private|public|protected|async|static|override|export)\s+)*[\w$]+\s*\()").unwrap())
        .is_match(line) && !["if", "for", "while", "switch", "catch", "match", "with"].iter()
            .any(|word| line.trim_start().strip_prefix(word).is_some_and(|tail| tail.starts_with([' ', '('])))
}

fn header(lines: &[&str], open: usize, column: usize, rust: bool) -> (usize, bool) {
    for start in (open.saturating_sub(16)..=open).rev() {
        if !declaration(lines[start], rust) {
            continue;
        }
        let prefix = format!(
            "{}\n{}",
            lines[start..open].join("\n"),
            &lines[open][..column]
        );
        let mut depth = 0usize;
        let mut last_close = None;
        for (i, c) in prefix.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' && depth > 0 {
                depth -= 1;
                if depth == 0 && last_close.is_none() {
                    last_close = Some(i + 1);
                }
            }
        }
        // Parameter object/default braces are not a method body. Completed
        // parent bodies must not turn a nested guard into another function.
        let arrow = prefix.trim_end().ends_with("=>");
        let body = depth == 0
            && last_close.is_some_and(|i| {
                let tail = prefix[i..].trim();
                let braces = tail.chars().fold(0isize, |depth, c| {
                    depth
                        + if c == '{' {
                            1
                        } else if c == '}' {
                            -1
                        } else {
                            0
                        }
                });
                braces == 0
                    && (tail.is_empty() || tail.starts_with(':') || tail.starts_with("->"))
                    && !["return ", "await ", "throw ", "const ", "let ", "if ("]
                        .iter()
                        .any(|word| tail.contains(word))
            });
        if !arrow && !body {
            continue;
        }
        let mut first = start;
        while first > 0
            && (lines[first - 1].trim_start().starts_with("#[")
                || lines[first - 1].trim_start().starts_with("///"))
        {
            first -= 1;
        }
        return (first, true);
    }
    (open, false)
}

fn brace_scopes(source: &str, lines: &[&str], rust: bool) -> Vec<Scope> {
    let b = source.as_bytes();
    let mut i = 0;
    let mut line = 0;
    let mut line_start = 0;
    let mut stack = Vec::new();
    let mut scopes = Vec::new();
    let mut comment = 0usize;
    let mut quote = 0u8;
    let mut raw: Option<usize> = None;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == b'\n' {
            line += 1;
            line_start = i + 1;
        }
        if let Some(hashes) = raw {
            if c == b'"'
                && b.get(i + 1..i + 1 + hashes)
                    .is_some_and(|v| v.iter().all(|v| *v == b'#'))
            {
                i += hashes;
                raw = None;
            }
        } else if comment > 0 {
            if c == b'/' && next == Some(b'*') {
                comment += 1;
                i += 1;
            } else if c == b'*' && next == Some(b'/') {
                comment -= 1;
                i += 1;
            }
        } else if quote != 0 {
            if c == b'\\' {
                if next == Some(b'\n') {
                    line += 1;
                    line_start = i + 2;
                }
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else if c == b'/' && next == Some(b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        } else if c == b'/' && next == Some(b'*') {
            comment = 1;
            i += 1;
        } else if rust && c == b'r' {
            let mut end = i + 1;
            while b.get(end) == Some(&b'#') {
                end += 1;
            }
            if b.get(end) == Some(&b'"') {
                raw = Some(end - i - 1);
                i = end;
            }
        } else if c == b'"'
            || c == b'`'
            || (c == b'\''
                && (!rust
                    || source[i + 1..]
                        .find('\'')
                        .filter(|end| *end <= 12)
                        .is_some_and(|end| {
                            let inner = &source[i + 1..i + 1 + end];
                            inner.chars().count() == 1 || inner.starts_with('\\')
                        })))
        {
            quote = c;
        } else if c == b'{' {
            let (start, function) = header(lines, line, i - line_start, rust);
            stack.push((start, line, function));
        } else if c == b'}'
            && let Some((start, open, function)) = stack.pop()
        {
            scopes.push(Scope {
                start,
                open,
                end: line + 1,
                function,
            });
        }
        i += 1;
    }
    scopes
}

impl Scope {
    fn name(&self, lines: &[&str]) -> String {
        static NAME: OnceLock<Regex> = OnceLock::new();
        let header = lines[self.start..=self.open].join(" ");
        if let Some(cap) = NAME
            .get_or_init(|| Regex::new(r"\b(?:fn|def|function)\s+([A-Za-z_]\w*)").unwrap())
            .captures(&header)
        {
            return cap[1].to_owned();
        }
        let head = header.split('(').next().unwrap_or("");
        head.split_whitespace()
            .last()
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect()
    }
    fn parse(source: &str, lines: &[&str], extension: &str) -> Vec<Self> {
        if let Some(spans) = crate::syntax::parse(source, extension) {
            return spans
                .into_iter()
                .map(|s| Self {
                    start: s.start,
                    open: s.open,
                    end: s.end,
                    function: s.function,
                })
                .collect();
        }
        if extension == "py" {
            lines
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let t = s.trim_start();
                    if !t.starts_with("def ") && !t.starts_with("async def ") {
                        return None;
                    }
                    let indent = s.len() - t.len();
                    let end = (i + 1..lines.len())
                        .find(|j| {
                            !lines[*j].trim().is_empty()
                                && lines[*j].len() - lines[*j].trim_start().len() <= indent
                        })
                        .unwrap_or(lines.len());
                    Some(Scope {
                        start: i,
                        open: i,
                        end,
                        function: true,
                    })
                })
                .collect()
        } else {
            brace_scopes(source, lines, extension == "rs")
        }
    }
}

pub fn enclosing_ranges(
    source: &str,
    hits: &[usize],
    extension: &str,
    whole_functions: bool,
    definition_first: bool,
) -> Vec<Range<usize>> {
    let lines: Vec<_> = source.lines().collect();
    if lines.is_empty() {
        return vec![];
    }
    let scopes = Scope::parse(source, &lines, extension);
    let has_definition = definition_first;
    let mut out = Vec::new();
    for &hit in hits {
        if hit >= lines.len() {
            continue;
        }
        let function = scopes
            .iter()
            .filter(|s| s.function && s.start <= hit && hit < s.end)
            .min_by_key(|s| s.end - s.start);
        if has_definition && function.is_some_and(|s| hit > s.open) {
            let s = function.unwrap();
            out.push(s.start..s.open + 1);
            out.push(hit.saturating_sub(3)..(hit + 4).min(lines.len()));
            continue;
        }
        let limit = if whole_functions { MAX_BLOCK } else { 64 };
        let bounded = |s: &Scope| {
            s.end - s.start <= limit
                && lines[s.start..s.end]
                    .iter()
                    .map(|s| s.len() + 1)
                    .sum::<usize>()
                    <= 12000
        };
        if let Some(s) = function.filter(|s| whole_functions && bounded(s)) {
            out.push(s.start..s.end);
            continue;
        }
        if let Some(s) = function {
            out.push(s.start..s.open + 1);
        }
        if let Some(s) = scopes
            .iter()
            .filter(|s| !s.function && s.start <= hit && hit < s.end && bounded(s))
            .max_by_key(|s| s.end - s.start)
        {
            let mut end = s.end;
            // Include the sibling else body, including when its brace shares a line.
            while let Some(next) = scopes
                .iter()
                .filter(|next| {
                    next.end > end
                        && next.end - s.start <= limit
                        && ((next.open == end - 1 && lines[end - 1].contains("else"))
                            || (next.open == end
                                && lines
                                    .get(end)
                                    .is_some_and(|l| l.trim_start().starts_with("else"))))
                })
                .min_by_key(|next| next.end)
            {
                end = next.end;
            }
            out.push(s.start..end);
        } else if function.is_some() && whole_functions {
            out.push(hit..hit + 1);
        } else {
            out.push(hit.saturating_sub(3)..(hit + 4).min(lines.len()));
        }
    }
    out
}

#[derive(Clone)]
struct ReviewCandidate {
    path: String,
    span: Range<usize>,
    hits: Vec<usize>,
    test: bool,
}
impl ReviewCandidate {
    fn test_scope(lines: &[&str]) -> bool {
        static TEST: OnceLock<Regex> = OnceLock::new();
        lines.iter().take(4).any(|line| {
            let t = line.trim_start();
            t.starts_with("#[") && t.contains("test")
                || t.starts_with("def test_")
                || t.starts_with("async def test_")
                || TEST
                    .get_or_init(|| Regex::new(r"^(?:test|it)(?:\.\w+)?\s*\(").unwrap())
                    .is_match(t)
        })
    }
    fn test_path(path: &str) -> bool {
        let p = Path::new(path);
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        p.components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("test" | "tests" | "__tests__")))
            || name.contains(".test.")
            || name.contains(".spec.")
            || name.starts_with("test_")
            || name.ends_with("_test.rs")
            || name.ends_with("_tests.rs")
            || name == "tests.rs"
    }
}

type SourceRanges = BTreeMap<String, Vec<(Range<usize>, u8)>>;

#[derive(Default)]
pub struct Evidence {
    pub requirements: crate::requirements::Requirements,
    pub rank_query: Option<String>,
    pub discovery_limit: usize,
    review_active: bool,
    semantic: BTreeMap<String, Vec<(u64, String)>>,
    semantic_ranks: BTreeMap<String, usize>,
    review_sources: usize,
    review_contract_sources: usize,
    review_tests: usize,
    review_helpers: usize,
    review_omitted: usize,
    review_files: usize,
    review_files_omitted: usize,
    review_preview_omitted: usize,
    review_constants: usize,
    discovery: Vec<SourceRanges>,
    discovery_units: Vec<(String, Range<usize>, u8)>,
    discovery_active: bool,
    discovery_selected: usize,
    discovery_omitted: usize,
    files: BTreeMap<String, Vec<String>>,
    ranges: SourceRanges,
    pub messages: Vec<String>,
    bytes: usize,
    aliases: BTreeMap<std::path::PathBuf, String>,
    complete_files: BTreeSet<String>,
    rows: usize,
    capture_omitted: usize,
    scope_limited: usize,
    focus: BTreeMap<String, BTreeSet<usize>>,
}
impl Evidence {
    fn key(&mut self, path: &str) -> io::Result<String> {
        let canonical = fs::canonicalize(path)?;
        Ok(self
            .aliases
            .entry(canonical)
            .or_insert_with(|| path.to_owned())
            .clone())
    }
    fn load(&mut self, path: &str) -> io::Result<String> {
        use std::io::Read;
        let key = self.key(path)?;
        if self.complete_files.contains(&key) {
            return Ok(key);
        }
        if !fs::metadata(path)?.is_file() {
            return Err(io::Error::other("read requires a regular UTF-8 file"));
        }
        let file = fs::File::open(path)?;
        let meta = file.metadata()?;
        let size = meta.len();
        if !meta.is_file() {
            return Err(io::Error::other("read requires a regular UTF-8 file"));
        }
        if size > MAX_SOURCE as u64 || self.bytes.saturating_add(size as usize) > 16 * 1024 * 1024 {
            return Err(io::Error::other(
                "source exceeds bounded scope cache; use a narrower explicit read",
            ));
        }
        let mut text = String::new();
        file.take(MAX_SOURCE as u64 + 1).read_to_string(&mut text)?;
        if text.len() > MAX_SOURCE {
            return Err(io::Error::other("source grew beyond scope cache limit"));
        }
        let lines: Vec<_> = text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .lines()
            .map(str::to_owned)
            .collect();
        let rows = self
            .rows
            .saturating_sub(self.files.get(&key).map_or(0, Vec::len))
            .saturating_add(lines.len());
        if rows > 200000 {
            return Err(io::Error::other("source exceeds scope row limit"));
        }
        self.bytes += text.len();
        self.rows = rows;
        self.files.insert(key.clone(), lines);
        self.complete_files.insert(key.clone());
        Ok(key)
    }
    pub fn read(&mut self, path: &str, start: usize, end: usize) -> io::Result<()> {
        if start == 0 || start > end {
            return Err(io::Error::other("invalid 1-based inclusive range"));
        }
        let key = self.load(path)?;
        let total = self.files[&key].len();
        if start > total && total != 0 {
            return Err(io::Error::other("read starts past EOF"));
        }
        if total == 0 {
            self.messages.push(format!("{key} empty file"));
            return Ok(());
        }
        if end > total {
            self.messages.push(format!("{key} EOF={total}"));
        }
        self.ranges
            .entry(key)
            .or_default()
            .push((start - 1..end.min(total), READ));
        Ok(())
    }
    pub fn search(&mut self, args: &Args, capture: Capture) -> io::Result<()> {
        if args.intent == Intent::Review {
            return self.review(args, capture, None, false);
        }
        let (_, paths) = args.inputs(true);
        let broad = self.rank_query.is_some()
            && self.discovery_limit > 0
            && args.intent == Intent::Locate
            && matches!(args.encoding, Encoding::Utf8)
            && (paths.len() > 1 || paths.iter().any(|path| path.is_dir()));
        if !broad {
            return self.search_captured(args, capture);
        }
        // Keep operation provenance: discovery cannot suppress explicit reads or explain scopes.
        let protected = std::mem::take(&mut self.ranges);
        let result = self.search_captured(args, capture);
        self.discovery
            .push(std::mem::replace(&mut self.ranges, protected));
        self.discovery_active = true;
        result
    }
    fn search_captured(&mut self, args: &Args, capture: Capture) -> io::Result<()> {
        self.capture_omitted = self.capture_omitted.saturating_add(capture.omitted);
        if capture.omitted > 0 {
            self.messages.push(format!(
                "capture_omitted={} (not exhaustive)",
                capture.omitted
            ));
        }
        let mut groups: BTreeMap<String, Vec<Line>> = BTreeMap::new();
        for line in capture.lines {
            groups.entry((*line.path).clone()).or_default().push(line);
        }
        // A definition may live in a different file from its callers.
        let mut definition_first = false;
        if args.block_context
            && args.explain_scope == ExplainScope::Auto
            && matches!(args.encoding, Encoding::Utf8)
        {
            for (original, captured) in &groups {
                if !Path::new(original).is_file() {
                    continue;
                }
                let key = self.load(original)?;
                let text = self.files[&key].join("\n");
                let lines: Vec<_> = text.lines().collect();
                let ext = Path::new(original)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                definition_first |= Scope::parse(&text, &lines, ext).iter().any(|s| {
                    s.function
                        && captured.iter().any(|l| {
                            l.selected
                                && s.start < l.number as usize
                                && l.number as usize - 1 <= s.open
                        })
                });
            }
        }
        for (original, lines) in groups {
            let path = if Path::new(&original).is_file() {
                self.key(&original)?
            } else {
                original
            };
            self.focus.entry(path.clone()).or_default().extend(
                lines
                    .iter()
                    .filter(|l| l.selected)
                    .map(|l| l.number as usize - 1),
            );
            if args.block_context
                && matches!(args.encoding, Encoding::Utf8)
                && Path::new(&path).is_file()
            {
                self.load(&path)?;
                if lines
                    .iter()
                    .any(|l| self.files[&path].get(l.number as usize - 1) != Some(&l.text))
                {
                    return Err(io::Error::other(
                        "source changed between search and scope read",
                    ));
                }
                let hits: Vec<_> = lines
                    .iter()
                    .filter(|l| l.selected)
                    .map(|l| l.number as usize - 1)
                    .collect();
                let text = self.files[&path].join("\n");
                let extension = Path::new(&path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if crate::syntax::parse(&text, extension).is_none() {
                    self.messages.push(format!(
                        "scope_parser=lexical fallback {path} (unsupported or parse error)"
                    ));
                } else {
                    self.messages.push(format!("scope_parser=ast-grep {path}"));
                }
                let original_ranges = enclosing_ranges(&text, &hits, extension, true, false);
                let mut ranges = enclosing_ranges(&text, &hits, extension, true, definition_first);
                let retained: BTreeSet<_> = ranges.iter().flat_map(|r| r.clone()).collect();
                let callers: Vec<_> = original_ranges
                    .iter()
                    .filter(|r| !(*r).clone().all(|n| retained.contains(&n)))
                    .collect();
                if !callers.is_empty() {
                    self.scope_limited += callers.len();
                    self.messages.push(format!(
                        "definition kept; callers use context; full caller ranges via read: {}",
                        callers
                            .iter()
                            .take(4)
                            .map(|r| format!("scope_read {path}:{}-{}", r.start + 1, r.end))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
                let expanded: BTreeSet<_> = ranges.iter().flat_map(|r| r.clone()).collect();
                let near: BTreeSet<_> = hits
                    .iter()
                    .flat_map(|n| {
                        n.saturating_sub(3)..n.saturating_add(4).min(self.files[&path].len())
                    })
                    .collect();
                if args.explain_scope == ExplainScope::Auto
                    && expanded.len() > MAX_BLOCK
                    && expanded.len() > near.len().saturating_mul(4)
                {
                    let local = enclosing_ranges(&text, &hits, extension, false, false);
                    let retained: BTreeSet<_> = local.iter().flat_map(|r| r.clone()).collect();
                    let limited: BTreeSet<_> = ranges
                        .iter()
                        .filter(|r| !(*r).clone().all(|n| retained.contains(&n)))
                        .map(|r| (r.start, r.end))
                        .collect();
                    if !limited.is_empty() {
                        self.scope_limited += limited.len();
                        self.messages.push(format!(
                            "scope narrowed (all hits kept); full ranges via --explain-scope full or read: {}",
                            limited.iter().take(4).map(|(start, end)| format!("scope_read {path}:{}-{end}", start + 1)).collect::<Vec<_>>().join("; ")
                        ));
                        ranges = local;
                    }
                }
                self.ranges
                    .entry(path)
                    .or_default()
                    .extend(ranges.into_iter().map(|r| (r, SCOPE)));
            } else {
                // Keep decoded search evidence for stdin, encodings and explicit contexts.
                let max = lines.iter().map(|l| l.number as usize).max().unwrap_or(0);
                let delta = max.saturating_sub(self.files.get(&path).map_or(0, Vec::len));
                if self.rows.saturating_add(delta) > 200000 {
                    return Err(io::Error::other("capture exceeds scope row limit"));
                }
                self.rows += delta;
                let file = self.files.entry(path.clone()).or_default();
                file.resize(max.max(file.len()), String::new());
                for line in lines {
                    let n = line.number as usize - 1;
                    let priority = if line.selected { HIT } else { READ };
                    file[n] = line.text;
                    self.ranges
                        .entry(path.clone())
                        .or_default()
                        .push((n..n + 1, priority));
                }
            }
        }
        Ok(())
    }
    pub fn resume(&mut self, session: &mut crate::session::Session) -> io::Result<()> {
        for (path, ranges) in session.pending() {
            match self.load(&path) {
                Ok(key) => {
                    let signature = crate::session::Session::signature(&self.files[&key]);
                    if session.refresh(&path, signature) {
                        self.messages.push(format!(
                            "{path} changed; rerun original search instead of stale ranges"
                        ));
                    } else {
                        self.ranges
                            .entry(key)
                            .or_default()
                            .extend(ranges.into_iter().map(|r| (r, READ)));
                    }
                }
                Err(err) => self.messages.push(format!("error: resume {path}: {err}")),
            }
        }
        Ok(())
    }
    fn definition_affinity(path: &str, query: &BTreeSet<String>) -> (usize, usize) {
        static NAME: OnceLock<Regex> = OnceLock::new();
        if fs::metadata(path).is_ok_and(|m| m.len() > MAX_SOURCE as u64) {
            return (0, 0);
        }
        let Ok(source) = fs::read_to_string(path) else {
            return (0, 0);
        };
        let re=NAME.get_or_init(||Regex::new(r"(?:\b(?:fn|def|function)\s+([A-Za-z_]\w*)|^\s*(?:(?:private|public|protected|async|static|override)\s+)+([A-Za-z_]\w*)\s*\()").unwrap());
        let mut covered = BTreeSet::new();
        let mut count = 0;
        let mut test = false;
        for line in source.lines() {
            if line.contains("#[cfg(test)]") || line.trim_start().starts_with("mod tests") {
                break;
            }
            if line.trim_start().starts_with("#[") && line.contains("test") {
                test = true;
                continue;
            }
            if let Some(cap) = re.captures(line) {
                let name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
                if test || name.starts_with("test_") {
                    test = false;
                    continue;
                }
                let words: BTreeSet<_> = crate::rank::tokens(name).into_iter().collect();
                let hits: Vec<_> = words.intersection(query).cloned().collect();
                if !hits.is_empty() {
                    count += 1;
                    covered.extend(hits);
                }
            }
        }
        (covered.len(), count)
    }
    fn module_family(path: &str) -> Option<String> {
        let parts: Vec<_> = Path::new(path)
            .components()
            .filter_map(|c| {
                if let std::path::Component::Normal(s) = c {
                    s.to_str()
                } else {
                    None
                }
            })
            .collect();
        parts
            .iter()
            .rposition(|s| matches!(*s, "src" | "lib" | "test" | "tests" | "__tests__"))
            .map(|i| parts[..i].join("/"))
    }
    fn schema_keys(lines: &[String]) -> BTreeSet<String> {
        static KEY: OnceLock<Regex> = OnceLock::new();
        KEY.get_or_init(|| Regex::new(r#"["']([a-z][a-z_]{3,})["']\s*:"#).unwrap())
            .captures_iter(&lines.join("\n"))
            .map(|cap| cap[1].to_owned())
            .filter(|s| {
                ![
                    "type", "text", "path", "name", "value", "status", "reason", "error",
                    "message", "state",
                ]
                .contains(&s.as_str())
            })
            .take(12)
            .collect()
    }
    fn review_documents(&mut self, args: &Args, capture: Capture, query: &str) -> io::Result<()> {
        let paths: BTreeSet<_> = capture
            .lines
            .iter()
            .filter(|l| l.selected)
            .map(|l| (*l.path).clone())
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        let mut exact = args.clone();
        exact.intent = Intent::Locate;
        exact.block_context = false;
        exact.paths = paths.into_iter().map(Into::into).collect();
        let mut full = Capture::default();
        let code = crate::run(&exact, &mut io::sink(), false, Some(&mut full))
            .map_err(|e| io::Error::other(e.to_string()))?;
        if code == 2 {
            self.messages
                .push("search error: review document may be partial".into());
        }
        self.capture_omitted += full.omitted;
        let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for line in full.lines.into_iter().filter(|l| l.selected) {
            grouped
                .entry((*line.path).clone())
                .or_default()
                .push(line.number as usize - 1);
        }
        let mut candidates = Vec::new();
        for (path, hits) in grouped {
            let key = self.load(&path)?;
            let lines = &self.files[&key];
            for hit in hits.into_iter().filter(|n| *n < lines.len()) {
                let heading = |line: &String| {
                    let t = line.trim_start();
                    t.starts_with('#') && t.trim_start_matches('#').starts_with(' ')
                };
                let start = (0..=hit)
                    .rev()
                    .find(|i| heading(&lines[*i]))
                    .unwrap_or(hit.saturating_sub(4));
                let end = (hit + 1..lines.len())
                    .find(|i| heading(&lines[*i]))
                    .unwrap_or(lines.len());
                let span = if end - start <= 120 {
                    start..end
                } else {
                    hit.saturating_sub(4)..(hit + 5).min(lines.len())
                };
                candidates.push(ReviewCandidate {
                    path: key.clone(),
                    span,
                    hits: vec![hit],
                    test: false,
                });
            }
        }
        let selected = self.review_pick(candidates, query, 6);
        self.messages.push(format!(
            "review documents={} (sections/context; no test expansion)",
            selected.len()
        ));
        for c in selected {
            self.ranges.entry(c.path).or_default().push((c.span, SCOPE));
        }
        Ok(())
    }
    fn contract_definitions(
        &mut self,
        args: &Args,
        selected: &[ReviewCandidate],
        query: &str,
        session: Option<&crate::session::Session>,
        repeat: bool,
    ) -> Vec<ReviewCandidate> {
        let words = crate::rank::tokens(query);
        if self.review_contract_sources >= 2
            || !words
                .iter()
                .any(|s| ["json", "serialization", "serialize", "schema"].contains(&s.as_str()))
        {
            return Vec::new();
        }
        let bytes: usize = self
            .files
            .iter()
            .filter(|(p, _)| {
                !ReviewCandidate::test_path(p)
                    && matches!(
                        Path::new(p).extension().and_then(|s| s.to_str()),
                        Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
                    )
            })
            .map(|(_, ls)| ls.iter().map(String::len).sum::<usize>())
            .sum();
        if bytes > 65536 {
            return Vec::new();
        }
        let Ok(root) = fs::canonicalize(".") else {
            return Vec::new();
        };
        let parents: BTreeSet<_> = selected
            .iter()
            .filter_map(|c| fs::canonicalize(&c.path).ok())
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| p.starts_with(&root))
            .collect();
        if parents.is_empty() {
            return Vec::new();
        }
        let mut extra = args.clone();
        extra.intent = Intent::Locate;
        extra.regex = false;
        extra.no_normalize = false;
        extra.ignore_case = true;
        extra.smart_case = false;
        extra.context = 0;
        extra.before_context = 0;
        extra.after_context = 0;
        extra.pattern = None;
        extra.block_context = false;
        extra.paths = parents
            .into_iter()
            .map(|p| {
                let relative = p.strip_prefix(&root).unwrap();
                if relative.as_os_str().is_empty() {
                    Path::new(".").to_path_buf()
                } else {
                    relative.to_path_buf()
                }
            })
            .collect();
        extra.patterns = ["json!", "serde_json::to_", "JSON.stringify", "json.dumps"]
            .map(str::to_owned)
            .to_vec();
        let mut preview = Capture::preview_for(query);
        match crate::run(&extra, &mut io::sink(), false, Some(&mut preview)) {
            Ok(code) => {
                if code == 2 {
                    self.messages
                        .push("search error: review contract may be partial".into());
                }
                // Filter actual serializer scopes before ranking them; a
                // parser/reference in a higher-ranked file must not hide a
                // sibling output definition. Keep this supplemental scan small.
                extra.paths = preview
                    .lines
                    .iter()
                    .filter(|l| l.selected)
                    .map(|l| Path::new(&*l.path).to_path_buf())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter(|p| fs::metadata(p).is_ok_and(|m| m.len() <= 16384))
                    .take(8)
                    .collect();
                preview
                    .lines
                    .retain(|l| extra.paths.iter().any(|p| p == Path::new(&*l.path)));
                let contract_query = format!(
                    "json serialization {}",
                    words
                        .iter()
                        .filter(|s| [
                            "column",
                            "byte",
                            "offset",
                            "position",
                            "coordinates",
                            "submatches",
                            "schema",
                            "start",
                            "end"
                        ]
                        .contains(&s.as_str()))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                let candidates = self
                    .review_file_candidates(&extra, preview, &contract_query, session, repeat)
                    .into_iter()
                    .filter(|c| {
                        !c.test
                            && Self::schema_keys(&self.files[&c.path][c.span.clone()]).len() >= 2
                            && !selected
                                .iter()
                                .any(|p| p.path == c.path && p.span == c.span)
                    })
                    .collect();
                self.review_pick(
                    candidates,
                    &contract_query,
                    2 - self.review_contract_sources,
                )
            }
            Err(e) => {
                self.messages.push(format!("error: review contract: {e}"));
                Vec::new()
            }
        }
    }
    // Rank bounded file previews before loading/expanding source scopes. Only
    // directory searches are gated; explicitly named files keep their contract.
    fn review_file_candidates(
        &mut self,
        args: &Args,
        capture: Capture,
        query: &str,
        session: Option<&crate::session::Session>,
        repeat: bool,
    ) -> Vec<ReviewCandidate> {
        self.capture_omitted += capture.omitted;
        self.review_preview_omitted += capture.preview_omitted;
        let mut groups: BTreeMap<String, Vec<Line>> = BTreeMap::new();
        for line in capture.lines.into_iter().filter(|l| l.selected) {
            if matches!(
                Path::new(&*line.path).extension().and_then(|s| s.to_str()),
                Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
            ) {
                groups.entry((*line.path).clone()).or_default().push(line);
            }
        }
        let docs: Vec<_> = groups
            .iter()
            .map(|(path, lines)| {
                crate::rank::tokens(&format!(
                    "{path} {path} {}",
                    lines
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                ))
            })
            .collect();
        let scores = crate::rank::bm25_scores(&docs, &crate::rank::tokens(query));
        let mut groups: Vec<_> = groups
            .into_iter()
            .zip(scores)
            .map(|((path, lines), score)| {
                let prior = if ReviewCandidate::test_path(&path) {
                    0.5
                } else if Path::new(&path)
                    .components()
                    .any(|c| matches!(c.as_os_str().to_str(), Some("src" | "lib")))
                {
                    2.0
                } else {
                    1.0
                };
                let bytes = fs::metadata(&path).map_or(16384, |m| m.len().max(16384)) as f64;
                let topic: BTreeSet<_> =
                    crate::rank::tokens(&format!("{query} {}", args.patterns.join(" ")))
                        .into_iter()
                        .collect();
                let (coverage, functions) = Self::definition_affinity(&path, &topic);
                let compact = (4096.0 * (functions + 1) as f64 / bytes).sqrt().min(1.0);
                let definitions = 1.0 + coverage.min(8) as f64 * 0.5;
                ((path, lines), score * prior * compact * definitions)
            })
            .collect();
        groups.sort_by(|((pa, _), sa), ((pb, _), sb)| sb.total_cmp(sa).then(pa.cmp(pb)));
        let broad = args.paths.iter().any(|p| p.is_dir());
        if !self.semantic_ranks.is_empty() {
            // Reciprocal rank fusion retains lexical candidates while admitting semantic misses.
            for (rank, ((path, _), score)) in groups.iter_mut().enumerate() {
                *score = 1.0 / (16.0 + rank as f64)
                    + self
                        .semantic_ranks
                        .get(path)
                        .map_or(0.0, |r| 1.0 / (16.0 + *r as f64));
            }
            groups.sort_by(|((pa, _), sa), ((pb, _), sb)| sb.total_cmp(sa).then(pa.cmp(pb)));
        }
        let best = groups.first().map_or(0.0, |(_, score)| *score);
        let family = groups
            .first()
            .and_then(|((p, _), _)| Self::module_family(p));
        let mut selected = 0;
        let mut out = Vec::new();
        let mut hints = 0;
        for ((path, _), score) in groups {
            if broad
                && (selected >= 3
                    || (best > 0.0 && score < best * 0.25)
                    || family
                        .as_ref()
                        .is_some_and(|f| Self::module_family(&path).as_ref() != Some(f)))
            {
                self.review_files_omitted += 1;
                if hints < 4 {
                    self.messages.push(format!(
                        "review_file {path} (unselected; narrow review if needed)"
                    ));
                    hints += 1;
                }
                continue;
            }
            let mut exact = args.clone();
            exact.paths = vec![path.into()];
            let mut capture = Capture::default();
            match crate::run(&exact, &mut io::sink(), false, Some(&mut capture)) {
                Ok(code) => {
                    if code == 2 {
                        self.messages
                            .push("search error: review file may be partial".into());
                    }
                    for (number, text) in self
                        .semantic
                        .get(&exact.paths[0].to_string_lossy().into_owned())
                        .into_iter()
                        .flatten()
                    {
                        capture.push(Line {
                            path: std::sync::Arc::new(
                                exact.paths[0].to_string_lossy().into_owned(),
                            ),
                            number: *number,
                            text: text.clone(),
                            ranges: vec![],
                            selected: true,
                        });
                    }
                    let candidates = self.review_candidates(capture, query);
                    let candidates = self.review_novel(candidates, session, repeat);
                    if !candidates.is_empty() {
                        selected += 1;
                        out.extend(candidates);
                    }
                }
                Err(err) => self.messages.push(format!("error: review file: {err}")),
            }
        }
        self.review_files += selected;
        out
    }
    fn review_candidates(&mut self, capture: Capture, query: &str) -> Vec<ReviewCandidate> {
        self.capture_omitted += capture.omitted;
        let mut groups: BTreeMap<String, Vec<Line>> = BTreeMap::new();
        for line in capture.lines.into_iter().filter(|l| l.selected) {
            if !matches!(
                Path::new(&*line.path).extension().and_then(|s| s.to_str()),
                Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
            ) {
                continue;
            }
            groups.entry((*line.path).clone()).or_default().push(line);
        }
        let documents: Vec<_> = groups
            .values()
            .map(|lines| {
                crate::rank::tokens(
                    &lines
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            })
            .collect();
        let scores = crate::rank::bm25_scores(&documents, &crate::rank::tokens(query));
        let mut groups: Vec<_> = groups.into_iter().zip(scores).collect();
        groups.sort_by(|((pa, _), sa), ((pb, _), sb)| sb.total_cmp(sa).then(pa.cmp(pb)));
        let mut out = Vec::new();
        for (i, ((path, lines), _)) in groups.into_iter().enumerate() {
            if i >= 64 {
                self.review_omitted += 1;
                continue;
            }
            let key = match self.load(&path) {
                Ok(key) => key,
                Err(err) => {
                    self.messages.push(format!("error: review {path}: {err}"));
                    continue;
                }
            };
            if lines
                .iter()
                .any(|line| self.files[&key].get(line.number as usize - 1) != Some(&line.text))
            {
                self.messages.push(format!(
                    "error: review {path}: source changed between search and scope read"
                ));
                continue;
            }
            let source = self.files[&key].join("\n");
            let borrowed: Vec<_> = source.lines().collect();
            let ext = Path::new(&key)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let scopes = Scope::parse(&source, &borrowed, ext);
            if crate::syntax::parse(&source, ext).is_none() {
                self.messages.push(format!(
                    "scope_parser=lexical fallback {path} (unsupported or parse error)"
                ));
            } else {
                self.messages.push(format!("scope_parser=ast-grep {path}"));
            }
            let mut selected: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
            for hit in lines
                .iter()
                .map(|l| l.number as usize - 1)
                .filter(|n| *n < borrowed.len())
            {
                if let Some(scope) = scopes
                    .iter()
                    .filter(|s| s.function && s.start <= hit && hit < s.end)
                    .min_by_key(|s| s.end - s.start)
                {
                    selected
                        .entry((scope.start, scope.end))
                        .or_default()
                        .push(hit);
                } else {
                    for range in enclosing_ranges(&source, &[hit], ext, true, false) {
                        selected
                            .entry((range.start, range.end))
                            .or_default()
                            .push(hit);
                    }
                }
            }
            // An import/module reference establishes a file-level test link;
            // score its actual tests, not the import or fixture helpers.
            if ReviewCandidate::test_path(&key)
                && lines.iter().any(|line| {
                    let text = line.text.trim_start();
                    (text.starts_with("import ")
                        || text.starts_with("from ")
                        || text.starts_with("use "))
                        && !scopes.iter().any(|s| {
                            s.function
                                && s.start < line.number as usize
                                && (line.number as usize - 1) < s.end
                        })
                })
            {
                for scope in scopes.iter().filter(|s| {
                    s.function && ReviewCandidate::test_scope(&borrowed[s.start..s.end])
                }) {
                    selected
                        .entry((scope.start, scope.end))
                        .or_default()
                        .push(scope.start);
                }
            }
            for ((start, end), hits) in selected {
                let is_test = ReviewCandidate::test_scope(&borrowed[start..end]);
                if ReviewCandidate::test_path(&key) && !is_test {
                    continue;
                }
                out.push(ReviewCandidate {
                    path: key.clone(),
                    span: start..end,
                    hits,
                    test: is_test,
                });
            }
        }
        out
    }
    fn review_pick(
        &mut self,
        mut candidates: Vec<ReviewCandidate>,
        query: &str,
        limit: usize,
    ) -> Vec<ReviewCandidate> {
        candidates.sort_by(|a, b| {
            (&a.path, a.span.start, a.span.end).cmp(&(&b.path, b.span.start, b.span.end))
        });
        candidates.dedup_by(|a, b| {
            if a.path == b.path && a.span == b.span {
                b.hits.extend(&a.hits);
                true
            } else {
                false
            }
        });
        let docs: Vec<_> = candidates
            .iter()
            .map(|c| crate::rank::tokens(&self.files[&c.path][c.span.clone()].join("\n")))
            .collect();
        let scores = crate::rank::bm25_scores(&docs, &crate::rank::tokens(query));
        let mut uncovered: BTreeSet<_> = crate::rank::tokens(query)
            .into_iter()
            .filter(|s| s.len() >= 3)
            .collect();
        let terms: Vec<BTreeSet<_>> = docs.into_iter().map(|d| d.into_iter().collect()).collect();
        let mut indices: Vec<_> = (0..candidates.len()).collect();
        let mut picked = Vec::new();
        while !indices.is_empty() && picked.len() < limit {
            let best = *indices
                .iter()
                .max_by(|&&a, &&b| {
                    let new_a = terms[a].intersection(&uncovered).count();
                    let new_b = terms[b].intersection(&uncovered).count();
                    new_a
                        .cmp(&new_b)
                        .then_with(|| scores[a].total_cmp(&scores[b]))
                        .then_with(|| b.cmp(&a))
                })
                .unwrap();
            uncovered.retain(|t| !terms[best].contains(t));
            indices.retain(|i| *i != best);
            picked.push(candidates[best].clone());
        }
        self.review_omitted += indices.len();
        for i in indices.into_iter().take(8) {
            let c = &candidates[i];
            self.messages.push(format!(
                "review_scope {}:{}-{} (unselected; narrow explain/read if needed)",
                c.path,
                c.span.start + 1,
                c.span.end
            ));
        }
        picked
    }
    fn review_ranges(&self, candidate: &ReviewCandidate) -> Vec<Range<usize>> {
        if candidate.span.len() <= MAX_BLOCK
            && self.files[&candidate.path][candidate.span.clone()]
                .iter()
                .map(String::len)
                .sum::<usize>()
                <= 12000
        {
            vec![candidate.span.clone()]
        } else {
            let source = self.files[&candidate.path].join("\n");
            let ext = Path::new(&candidate.path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            enclosing_ranges(&source, &candidate.hits, ext, true, false)
        }
    }
    fn review_novel(
        &self,
        candidates: Vec<ReviewCandidate>,
        session: Option<&crate::session::Session>,
        repeat: bool,
    ) -> Vec<ReviewCandidate> {
        candidates
            .into_iter()
            .filter(|c| {
                if self.review_ranges(c).into_iter().flatten().all(|n| {
                    self.ranges
                        .get(&c.path)
                        .is_some_and(|ranges| ranges.iter().any(|(r, _)| r.contains(&n)))
                }) {
                    return false;
                }
                if repeat {
                    return true;
                }
                let Some(state) = session else {
                    return true;
                };
                let Ok(path) = fs::canonicalize(&c.path) else {
                    return true;
                };
                let Some(key) = path.to_str() else {
                    return true;
                };
                if !state.matches_source(
                    key,
                    &crate::session::Session::signature(&self.files[&c.path]),
                ) {
                    return true;
                }
                self.review_ranges(c)
                    .into_iter()
                    .flatten()
                    .any(|n| !state.seen(key, n))
            })
            .collect()
    }
    fn review_add(&mut self, candidate: &ReviewCandidate) {
        let ranges = self.review_ranges(candidate);
        if ranges.len() != 1 || ranges[0] != candidate.span {
            self.scope_limited += 1;
            self.messages.push(format!(
                "review local guards; full range via read {}:{}-{}",
                candidate.path,
                candidate.span.start + 1,
                candidate.span.end
            ));
        }
        self.ranges
            .entry(candidate.path.clone())
            .or_default()
            .extend(ranges.into_iter().map(|r| (r, SCOPE)));
    }
    pub fn semantic_found(&self) -> bool {
        !self.semantic_ranks.is_empty()
    }
    pub fn review(
        &mut self,
        args: &Args,
        mut capture: Capture,
        session: Option<&crate::session::Session>,
        repeat: bool,
    ) -> io::Result<()> {
        self.review_active = true;
        let query = self
            .rank_query
            .clone()
            .unwrap_or_else(|| args.patterns.join(" "));
        self.semantic.clear();
        self.semantic_ranks.clear();
        match crate::embedding::Client::search(args, &query) {
            Ok(Some(candidates)) => {
                self.messages.push(candidates.note);
                self.semantic_ranks = candidates.ranks;
                for line in candidates.lines {
                    self.semantic
                        .entry((*line.path).clone())
                        .or_default()
                        .push((line.number, line.text.clone()));
                    capture.push(line);
                }
            }
            Ok(None) => {}
            Err(err) => self.messages.push(format!(
                "embedding fallback: {err}; lexical candidates retained"
            )),
        }
        let (code, docs): (Vec<_>, Vec<_>) = std::mem::take(&mut capture.lines)
            .into_iter()
            .partition(|l| {
                matches!(
                    Path::new(&*l.path).extension().and_then(|s| s.to_str()),
                    Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
                )
            });
        let explicit_document = args.paths.iter().any(|p| {
            p.is_file()
                && !matches!(
                    p.extension().and_then(|s| s.to_str()),
                    Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
                )
        });
        if code.is_empty() || explicit_document {
            let mut documents = Capture::default();
            for line in docs {
                documents.push(line);
            }
            self.review_documents(args, documents, &query)?;
        }
        capture.lines = code;
        let candidates = self.review_file_candidates(args, capture, &query, session, repeat);
        let (tests, sources): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| c.test);
        let mut selected = self.review_pick(
            sources,
            &query,
            8usize.saturating_sub(
                self.review_sources
                    .saturating_sub(self.review_contract_sources),
            ),
        );
        let companions = self.contract_definitions(args, &selected, &query, session, repeat);
        if !companions.is_empty() {
            self.messages.push(format!(
                "review contract definitions={} (sibling serialization; lexical)",
                companions.len()
            ));
        }
        self.review_contract_sources += companions.len();
        selected.extend(companions);
        let mut related = tests;
        let mut terms: BTreeSet<_> = crate::rank::tokens(&query).into_iter().collect();
        let mut modules = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        let mut test_globs = Vec::new();
        for c in &selected {
            if let Some(stem) = Path::new(&c.path).file_stem().and_then(|s| s.to_str())
                && stem.len() >= 4
                && stem
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
            {
                modules.insert(stem.to_owned());
                modules.insert(stem.replace('-', "_"));
                let mut stems = vec![stem];
                if let Some(first) = stem.split(['_', '-']).next().filter(|s| s.len() >= 6) {
                    stems.push(first);
                }
                for stem in stems {
                    for glob in [
                        format!("**/tests/*{stem}*"),
                        format!("**/test/*{stem}*"),
                        format!("**/*{stem}*test*"),
                        format!("**/*test*{stem}*"),
                    ] {
                        test_globs.push(glob);
                    }
                }
            }
            let source = self.files[&c.path].join("\n");
            let lines: Vec<_> = source.lines().collect();
            let ext = Path::new(&c.path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if let Some(scope) = Scope::parse(&source, &lines, ext)
                .into_iter()
                .find(|s| s.function && s.start == c.span.start && s.end == c.span.end)
            {
                let name = scope.name(&lines);
                if name.len() >= 4
                    && ![
                        "main", "test", "new", "run", "line", "describe", "stage", "verify",
                        "check", "read", "write", "load", "save", "parse", "build", "setup",
                        "default", "create", "handle",
                    ]
                    .contains(&name.as_str())
                {
                    aliases.insert(name);
                }
            }
        }
        terms.retain(|s| s.len() >= 4);
        terms.extend(modules.iter().cloned());
        terms.extend(aliases.iter().cloned());
        // No source provenance means no new root-wide test expansion. In
        // particular a test-only request remains within its named test files.
        if !selected.is_empty() && self.review_tests < 6 {
            let mut test_args = args.clone();
            test_args.intent = Intent::Locate;
            test_args.block_context = false;
            test_args.regex = false;
            test_args.no_normalize = false;
            test_args.ignore_case = true;
            test_args.smart_case = false;
            test_args.context = 0;
            test_args.before_context = 0;
            test_args.after_context = 0;
            test_args.pattern = None;
            test_args.paths = vec![".".into()];
            test_args.patterns = terms.iter().take(48).cloned().collect();
            test_globs.sort();
            test_globs.dedup();
            let exclusions: Vec<_> = args
                .globs
                .iter()
                .filter(|g| g.starts_with('!'))
                .cloned()
                .collect();
            if !test_globs.is_empty() && !test_args.patterns.is_empty() {
                test_args.globs = exclusions.iter().cloned().chain(test_globs).collect();
                let mut preview = Capture::preview_for(&query);
                match crate::run(&test_args, &mut io::sink(), false, Some(&mut preview)) {
                    Ok(code) => {
                        if code == 2 {
                            self.messages
                                .push("search error: review related tests may be partial".into());
                        }
                        related.extend(
                            self.review_file_candidates(
                                &test_args, preview, &query, session, repeat,
                            )
                            .into_iter()
                            .filter(|c| c.test),
                        );
                    }
                    Err(err) => self
                        .messages
                        .push(format!("error: review related tests: {err}")),
                }
            }
            // Cross-file fallback uses module provenance or non-generic source
            // identifiers, never the broad query words or bare stage/verify.
            let missing: BTreeSet<_> = aliases
                .into_iter()
                .filter(|name| {
                    !related.iter().any(|test| {
                        self.files[&test.path][test.span.clone()]
                            .iter()
                            .any(|line| line.contains(name))
                    })
                })
                .collect();
            if related.is_empty() || !missing.is_empty() {
                test_args.patterns = modules.into_iter().chain(missing).collect();
                test_args.globs = exclusions
                    .into_iter()
                    .chain(
                        [
                            "**/test/**",
                            "**/tests/**",
                            "**/__tests__/**",
                            "**/*test*.*",
                            "**/*spec*.*",
                        ]
                        .map(str::to_owned),
                    )
                    .collect();
                if !test_args.patterns.is_empty() {
                    let mut preview = Capture::preview_for(&query);
                    match crate::run(&test_args, &mut io::sink(), false, Some(&mut preview)) {
                        Ok(code) => {
                            if code == 2 {
                                self.messages.push(
                                    "search error: review test fallback may be partial".into(),
                                );
                            }
                            related.extend(
                                self.review_file_candidates(
                                    &test_args, preview, &query, session, repeat,
                                )
                                .into_iter()
                                .filter(|c| c.test),
                            );
                        }
                        Err(err) => self
                            .messages
                            .push(format!("error: review test fallback: {err}")),
                    }
                }
            }
        }
        let mut schema_related = Vec::new();
        let query_words = crate::rank::tokens(&query);
        if !selected.is_empty()
            && self.review_tests < 6
            && query_words
                .iter()
                .any(|s| ["json", "serialization", "serialize", "schema"].contains(&s.as_str()))
        {
            let keys: BTreeSet<_> = selected
                .iter()
                .flat_map(|c| Self::schema_keys(&self.files[&c.path][c.span.clone()]))
                .collect();
            if keys.len() >= 2 {
                let mut tests = args.clone();
                tests.intent = Intent::Locate;
                tests.pattern = None;
                tests.patterns = keys.iter().map(|s| format!("\"{s}\"")).collect();
                tests.regex = false;
                tests.ignore_case = true;
                tests.smart_case = false;
                tests.no_normalize = false;
                tests.context = 0;
                tests.before_context = 0;
                tests.after_context = 0;
                tests.block_context = false;
                tests.paths = vec![".".into()];
                tests.globs = args
                    .globs
                    .iter()
                    .filter(|g| g.starts_with('!'))
                    .cloned()
                    .chain(
                        [
                            "**/test/**",
                            "**/tests/**",
                            "**/__tests__/**",
                            "**/*test*.*",
                            "**/*spec*.*",
                        ]
                        .map(str::to_owned),
                    )
                    .collect();
                let schema_query = format!(
                    "json {}",
                    keys.iter().cloned().collect::<Vec<_>>().join(" ")
                );
                let mut preview = Capture::preview_for(&schema_query);
                match crate::run(&tests, &mut io::sink(), false, Some(&mut preview)) {
                    Ok(code) => {
                        if code == 2 {
                            self.messages
                                .push("search error: review schema tests may be partial".into());
                        }
                        schema_related.extend(
                            self.review_file_candidates(
                                &tests,
                                preview,
                                &schema_query,
                                session,
                                repeat,
                            )
                            .into_iter()
                            .filter(|c| {
                                c.test
                                    && keys
                                        .iter()
                                        .filter(|key| {
                                            self.files[&c.path][c.span.clone()]
                                                .iter()
                                                .any(|line| line.contains(&format!("\"{key}\"")))
                                        })
                                        .count()
                                        >= 2
                            }),
                        );
                    }
                    Err(e) => self
                        .messages
                        .push(format!("error: review schema tests: {e}")),
                }
            }
        }
        let related_query = format!(
            "{query} {}",
            terms.into_iter().collect::<Vec<_>>().join(" ")
        );
        let schema_related = self.review_novel(schema_related, session, repeat);
        let mut tests = self.review_pick(
            schema_related,
            &related_query,
            2usize.min(6usize.saturating_sub(self.review_tests)),
        );
        let related = self
            .review_novel(related, session, repeat)
            .into_iter()
            .filter(|c| !tests.iter().any(|t| t.path == c.path && t.span == c.span))
            .collect();
        tests.extend(self.review_pick(
            related,
            &related_query,
            6usize.saturating_sub(self.review_tests + tests.len()),
        ));
        for c in &selected {
            self.review_add(c);
            self.review_sources += 1;
        }
        for c in &tests {
            self.review_add(c);
            self.review_tests += 1;
        }
        // One-level same-file helper and constant references, never recursive closure.
        let mut helper_count = self.review_helpers;
        let mut constants = self.review_constants;
        let mut references: BTreeMap<String, String> = BTreeMap::new();
        for c in selected.iter().chain(&tests) {
            references
                .entry(c.path.clone())
                .or_default()
                .push_str(&self.files[&c.path][c.span.clone()].join("\n"));
        }
        let type_re =
            Regex::new(r"^\s*(?:export\s+)?(?:pub\s+)?(?:class|struct|enum)\s+([A-Za-z_]\w*)")
                .unwrap();
        let re = Regex::new(r"^\s*(?:export\s+)?(?:pub\s+)?(?:const|static)\s+([A-Z][A-Z0-9_]*)\b")
            .unwrap();
        for (path, body) in references {
            let lines = &self.files[&path];
            let source = lines.join("\n");
            let borrowed: Vec<_> = source.lines().collect();
            let ext = Path::new(&path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let mut helpers = Vec::new();
            for scope in Scope::parse(&source, &borrowed, ext)
                .into_iter()
                .filter(|s| s.function && s.end - s.start <= 64)
            {
                if selected.iter().chain(&tests).any(|c| {
                    c.path == path && c.span.start <= scope.start && scope.end <= c.span.end
                }) {
                    continue;
                }
                let name = scope.name(&borrowed);
                if name.is_empty()
                    || ["new", "run", "main", "test", "it", "describe"].contains(&name.as_str())
                {
                    continue;
                }
                let called = Regex::new(&format!(r"\b{}\s*\(", regex::escape(&name)))
                    .unwrap()
                    .is_match(&body);
                if called && helper_count < 8 {
                    let candidate = ReviewCandidate {
                        path: path.clone(),
                        span: scope.start..scope.end,
                        hits: vec![scope.open],
                        test: false,
                    };
                    helpers.push(candidate);
                    helper_count += 1;
                }
            }
            let body = format!(
                "{body}\n{}",
                helpers
                    .iter()
                    .map(|c| self.files[&c.path][c.span.clone()].join("\n"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            // Small referenced types make fixture/device substitutions and option
            // branches inspectable without returning an entire class/module.
            for (n, line) in borrowed.iter().enumerate() {
                if helper_count >= 8 {
                    break;
                }
                if let Some(cap) = type_re.captures(line) {
                    if !Regex::new(&format!(r"\b{}\b", regex::escape(&cap[1])))
                        .unwrap()
                        .is_match(&body)
                    {
                        continue;
                    }
                    if let Some(scope) = Scope::parse(&source, &borrowed, ext)
                        .into_iter()
                        .find(|s| s.open == n && !s.function && s.end - s.start <= 96)
                    {
                        helpers.push(ReviewCandidate {
                            path: path.clone(),
                            span: scope.start..scope.end,
                            hits: vec![scope.open],
                            test: false,
                        });
                        helper_count += 1;
                    }
                }
            }
            let mut constant_ranges = Vec::new();
            for (n, line) in lines.iter().enumerate() {
                if constants >= 16 {
                    break;
                }
                if let Some(cap) = re.captures(line) {
                    let name = &cap[1];
                    if Regex::new(&format!(r"\b{}\b", regex::escape(name)))
                        .unwrap()
                        .is_match(&body)
                    {
                        let end = (n..lines.len().min(n + 8))
                            .find(|i| lines[*i].contains(';'))
                            .map_or(n + 1, |i| i + 1);
                        constant_ranges.push((n..end, READ));
                        constants += 1;
                    }
                }
            }
            for c in &helpers {
                self.review_add(c);
            }
            self.ranges.entry(path).or_default().extend(constant_ranges);
        }
        self.review_helpers = helper_count;
        self.review_constants = constants;
        self.messages.push("review links are lexical; related tests are not proof of coverage or absence; omitted scopes are not pending".into());
        Ok(())
    }
    fn select_discovery(
        &mut self,
        tracked: &BTreeMap<String, String>,
        session: Option<&crate::session::Session>,
        repeat: bool,
    ) {
        let Some(query) = self.rank_query.as_ref() else {
            return;
        };
        let query = crate::rank::tokens(query);
        let mut index_count = 0;
        for group in std::mem::take(&mut self.discovery) {
            let mut candidates = Vec::new();
            for (path, ranges) in group {
                for range in
                    crate::session::Session::merge(ranges.into_iter().map(|(r, _)| r).collect())
                {
                    let mut start = range.start;
                    let mut size = 0;
                    for n in range.clone() {
                        let length = self.files[&path][n].chars().count() + 1;
                        if size > 0 && size + length > 4000 {
                            candidates.push((path.clone(), start..n));
                            start = n;
                            size = 0;
                        }
                        size += length;
                    }
                    candidates.push((path.clone(), start..range.end));
                }
            }
            // A repeated broad search advances to unseen candidates instead of filling
            // its top slots with rows already supplied by this call or the session.
            candidates.retain(|(path, range)| {
                range.clone().any(|n| {
                    let protected = self
                        .ranges
                        .get(path)
                        .is_some_and(|rs| rs.iter().any(|(r, _)| r.contains(&n)));
                    let seen = !repeat
                        && tracked
                            .get(path)
                            .is_some_and(|key| session.is_some_and(|s| s.seen(key, n)));
                    !protected && !seen
                })
            });
            let documents: Vec<_> = candidates
                .iter()
                .map(|(path, range)| {
                    crate::rank::tokens(&self.files[path][range.clone()].join("\n"))
                })
                .collect();
            let scores = crate::rank::bm25_scores(&documents, &query);
            let mut ranked: Vec<_> = candidates.into_iter().zip(scores).collect();
            ranked.sort_by(|((pa, ra), sa), ((pb, rb), sb)| {
                sb.total_cmp(sa)
                    .then_with(|| (pa, ra.start, ra.end).cmp(&(pb, rb.start, rb.end)))
            });
            for (i, ((path, range), _)) in ranked.into_iter().enumerate() {
                if i < self.discovery_limit {
                    self.discovery_selected += 1;
                    self.ranges
                        .entry(path.clone())
                        .or_default()
                        .push((range.clone(), DISCOVERY));
                    self.discovery_units.push((path, range, DISCOVERY));
                } else {
                    self.discovery_omitted += 1;
                    if index_count < 16 {
                        self.messages.push(format!(
                            "unselected {path}:{}-{} (narrow read/search; not pending)",
                            range.start + 1,
                            range.end
                        ));
                        index_count += 1;
                    }
                }
            }
        }
    }
    // Merge adjacent hit/context/read rows, while preserving independently
    // useful function boundaries for the constrained-budget fallback.
    fn units(&self) -> Vec<(String, Range<usize>, u8)> {
        let mut units = Vec::new();
        for (path, ranges) in &self.ranges {
            for priority in [SCOPE, HIT, READ, DISCOVERY] {
                let selected: Vec<_> = ranges
                    .iter()
                    .filter(|(_, p)| *p == priority)
                    .map(|(r, _)| r.clone())
                    .collect();
                let selected = if priority == SCOPE {
                    selected
                } else {
                    crate::session::Session::merge(selected)
                };
                units.extend(selected.into_iter().map(|r| (path.clone(), r, priority)));
            }
        }
        units.sort_by(|(pa, ra, ka), (pb, rb, kb)| {
            (ka, ra.len(), pa, ra.start, ra.end).cmp(&(kb, rb.len(), pb, rb.start, rb.end))
        });
        units.dedup();
        if self.discovery_active || self.review_active {
            units.retain(|(_, _, priority)| *priority != DISCOVERY);
            units.extend(self.discovery_units.clone());
            return units;
        }
        if let Some(query) = &self.rank_query {
            // Rank contiguous original passages, including requested context.
            // Split only at line boundaries; an individual long line stays intact.
            let mut passages = Vec::new();
            for (path, ranges) in &self.ranges {
                for range in
                    crate::session::Session::merge(ranges.iter().map(|(r, _)| r.clone()).collect())
                {
                    let mut start = range.start;
                    let mut size = 0;
                    for n in range.clone() {
                        let length = self.files[path][n].chars().count() + 1;
                        if size > 0 && size + length > 4000 {
                            passages.push((path.clone(), start..n, HIT));
                            start = n;
                            size = 0;
                        }
                        size += length;
                    }
                    passages.push((path.clone(), start..range.end, HIT));
                }
            }
            let documents: Vec<_> = passages
                .iter()
                .map(|(path, range, _)| {
                    crate::rank::tokens(&self.files[path][range.clone()].join("\n"))
                })
                .collect();
            let scores = crate::rank::bm25_scores(&documents, &crate::rank::tokens(query));
            let mut ranked: Vec<_> = passages.into_iter().zip(scores).collect();
            ranked.sort_by(|((pa, ra, _), sa), ((pb, rb, _), sb)| {
                sb.total_cmp(sa)
                    .then_with(|| (pa, ra.start, ra.end).cmp(&(pb, rb.start, rb.end)))
            });
            return ranked.into_iter().map(|(passage, _)| passage).collect();
        }
        units
    }
    fn escape(s: &str) -> String {
        s.chars()
            .flat_map(|c| {
                if c.is_control() {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect()
    }
    fn row_ranges(rows: &BTreeSet<usize>) -> Vec<Range<usize>> {
        crate::session::Session::merge(rows.iter().map(|n| *n..n + 1).collect())
    }
    fn body(&self, path: &str, rows: &[usize]) -> String {
        if rows.is_empty() {
            return String::new();
        }
        let ranges = Self::row_ranges(&rows.iter().copied().collect());
        let references = ranges
            .iter()
            .map(|r| format!("{}-{}", r.start + 1, r.end))
            .collect::<Vec<_>>()
            .join(",");
        let mut out = format!("@ {}:{references}\n", Self::escape(path));
        for n in rows {
            out.push_str(&format!(
                "{}:{}\n",
                n + 1,
                Self::escape(&self.files[path][*n])
            ));
        }
        out
    }
    /// Collect first, subtract delivered rows, then pack the final output. Partial
    /// ranges remain explicitly pending; only full original lines count as shown.
    pub fn render_tracked(
        mut self,
        tokenizer: &tiktoken_rs::CoreBPE,
        budget: usize,
        mut session: Option<&mut crate::session::Session>,
        repeat: bool,
    ) -> String {
        let counter = crate::budget::Counter::new(tokenizer);
        let mut tracked = BTreeMap::new();
        let mut changed = 0;
        let mut untracked = 0;
        if let Some(state) = session.as_deref_mut() {
            for path in self
                .ranges
                .keys()
                .chain(self.discovery.iter().flat_map(|group| group.keys()))
                .cloned()
                .collect::<BTreeSet<_>>()
            {
                if !self.complete_files.contains(&path) && self.load(&path).is_err() {
                    untracked += 1;
                    continue;
                }
                if let Ok(canonical) = fs::canonicalize(&path)
                    && let Some(key) = canonical.to_str()
                {
                    changed += usize::from(
                        state.refresh(key, crate::session::Session::signature(&self.files[&path])),
                    );
                    tracked.insert(path, key.to_owned());
                }
            }
        }
        self.select_discovery(&tracked, session.as_deref(), repeat);
        let units = self.units();
        let mut selected: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (path, range, _) in &units {
            selected
                .entry(path.clone())
                .or_default()
                .extend(range.clone().filter(|n| *n < self.files[path].len()));
        }
        let mapping = self
            .requirements
            .render(&self.files, &selected, &counter, budget);
        let errors = self
            .messages
            .iter()
            .filter(|s| s.contains("error:") || s.contains("search error"))
            .count();
        let footer = |pending: usize, seen: usize, partial: usize| {
            format!(
                "# scope=heuristic deferred={pending} errors={errors} capture_omitted={} seen={seen} pending={pending} partial={partial} changed={changed} untracked={untracked}\n",
                self.capture_omitted
            ) + &if self.review_active {
                format!(
                    "# bundle source={} tests={} helpers={} omitted={} (heuristic; not proof)\n",
                    self.review_sources,
                    self.review_tests,
                    self.review_helpers,
                    self.review_omitted
                ) + &format!(
                    "# review_files selected={} omitted={} preview_omitted={} (not pending)\n",
                    self.review_files, self.review_files_omitted, self.review_preview_omitted
                )
            } else {
                String::new()
            } + &if self.discovery_active {
                format!(
                    "# rank=bm25 discovery_selected={} discovery_omitted={} (not pending/absence)\n",
                    self.discovery_selected, self.discovery_omitted
                )
            } else if self.rank_query.is_some() {
                "# rank=bm25 candidates=captured (not an absence proof)\n".to_owned()
            } else {
                String::new()
            } + &if self.scope_limited > 0 {
                format!(
                    "# scope_limited={} (local scopes; not full functions)\n",
                    self.scope_limited
                )
            } else {
                String::new()
            }
        };
        // Reserve the worst bounded counters before packing any source rows.
        let reserve = mapping.text.clone() + &footer(999999, 999999, 999999);
        // Transport also limits output by a different token estimate. Keep
        // byte headroom before recording seen rows; never trim after tracking.
        let fits = |s: &str| {
            s.len() + reserve.len() <= 96_000 && counter.count(&format!("{s}{reserve}")) <= budget
        };
        let mut wanted: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut shown = mapping.delivered;
        let mut old: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut output = String::new();
        let mut partial = 0;
        let mut full_rows: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (path, range, _) in &units {
            for n in range.clone().filter(|n| *n < self.files[path].len()) {
                wanted.entry(path.clone()).or_default().insert(n);
                let seen = !repeat
                    && tracked.get(path).is_some_and(|key| {
                        session.as_deref().is_some_and(|state| state.seen(key, n))
                    });
                if seen {
                    old.entry(path.clone()).or_default().insert(n);
                } else if !shown.get(path).is_some_and(|rows| rows.contains(&n)) {
                    full_rows.entry(path.clone()).or_default().insert(n);
                }
            }
        }
        let mut complete = String::new();
        let mut bounded = true;
        for (path, rows) in &full_rows {
            complete.push_str(&self.body(path, &rows.iter().copied().collect::<Vec<_>>()));
            if complete.len() > budget.saturating_mul(32) {
                bounded = false;
                break;
            }
        }
        if self.rank_query.is_none() && bounded && fits(&complete) {
            output = complete;
            for (path, rows) in full_rows {
                shown.entry(path).or_default().extend(rows);
            }
        } else {
            for (path, range, _) in units {
                let rows: Vec<_> = range.filter(|n| *n < self.files[&path].len()).collect();
                wanted.entry(path.clone()).or_default().extend(&rows);
                let covered = shown.entry(path.clone()).or_default();
                let novel: Vec<_> = rows
                    .into_iter()
                    .filter(|n| {
                        let seen = !repeat
                            && tracked.get(&path).is_some_and(|key| {
                                session.as_deref().is_some_and(|s| s.seen(key, *n))
                            });
                        if seen {
                            old.entry(path.clone()).or_default().insert(*n);
                        }
                        !seen && !covered.contains(n)
                    })
                    .collect();
                if novel.is_empty() {
                    continue;
                }
                let body = self.body(&path, &novel);
                if fits(&format!("{output}{body}")) {
                    output.push_str(&body);
                    covered.extend(novel);
                    continue;
                }
                partial += 1;
                // A ranked passage may only fit partially. Preserve its query
                // evidence before filling from the original literal-hit context.
                if let Some(query) = &self.rank_query {
                    let documents: Vec<_> = novel
                        .iter()
                        .map(|n| crate::rank::tokens(&self.files[&path][*n]))
                        .collect();
                    let scores = crate::rank::bm25_scores(&documents, &crate::rank::tokens(query));
                    let mut relevant: Vec<_> = novel
                        .iter()
                        .copied()
                        .zip(scores)
                        .filter(|(_, score)| *score > 0.0)
                        .collect();
                    relevant.sort_by(|(na, sa), (nb, sb)| sb.total_cmp(sa).then(na.cmp(nb)));
                    let mut kept = BTreeSet::new();
                    for (n, _) in relevant {
                        kept.insert(n);
                        if !fits(&format!(
                            "{output}{}",
                            self.body(&path, &kept.iter().copied().collect::<Vec<_>>())
                        )) {
                            kept.remove(&n);
                        }
                    }
                    if !kept.is_empty() {
                        output
                            .push_str(&self.body(&path, &kept.iter().copied().collect::<Vec<_>>()));
                        covered.extend(kept);
                    }
                }
                let focus = self.focus.get(&path);
                let (near, rest): (Vec<_>, Vec<_>) = novel
                    .into_iter()
                    .filter(|n| !covered.contains(n))
                    .partition(|n| {
                        focus.is_some_and(|hits| {
                            hits.range(n.saturating_sub(3)..=n.saturating_add(3))
                                .next()
                                .is_some()
                        })
                    });
                for group in [near, rest] {
                    for range in Self::row_ranges(&group.iter().copied().collect()) {
                        let rows: Vec<_> = range.collect();
                        let mut low = 0;
                        let mut high = rows.len();
                        while low < high {
                            let mid = low + (high - low).div_ceil(2);
                            if fits(&format!("{output}{}", self.body(&path, &rows[..mid]))) {
                                low = mid;
                            } else {
                                high = mid - 1;
                            }
                        }
                        if low > 0 {
                            output.push_str(&self.body(&path, &rows[..low]));
                            covered.extend(rows[..low].iter().copied());
                        }
                    }
                }
            }
        }
        let mut pending_count = 0;
        for (path, rows) in &wanted {
            let delivered = shown.get(path).cloned().unwrap_or_default();
            let mut missing: BTreeSet<_> = rows.difference(&delivered).copied().collect();
            if let Some(key) = tracked.get(path)
                && let Some(state) = session.as_deref_mut()
            {
                missing.extend(state.previous_pending(key).into_iter().flatten());
                missing.retain(|n| !delivered.contains(n) && !state.seen(key, *n));
                state.record(
                    key,
                    Self::row_ranges(&delivered),
                    Self::row_ranges(&missing),
                );
            } else {
                if let Some(prior) = old.get(path) {
                    missing.retain(|n| !prior.contains(n));
                }
            }
            pending_count += Self::row_ranges(&missing).len();
        }
        if let Some(state) = session.as_deref() {
            pending_count = state.pending().iter().map(|(_, rs)| rs.len()).sum();
        }
        let suffix = footer(
            pending_count,
            old.values().map(BTreeSet::len).sum(),
            partial,
        );
        output = mapping.text + &output;
        // Notes are optional; the error/capture/partial counters always survive.
        for note in &self.messages {
            let line = format!("# {}\n", Self::escape(note));
            if output.len() + line.len() + suffix.len() <= 96_000
                && counter.count(&format!("{output}{line}{suffix}")) <= budget
            {
                output.push_str(&line);
            }
        }
        output.push_str(&suffix);
        output
    }
    pub fn render(self, tokenizer: &tiktoken_rs::CoreBPE, budget: usize) -> String {
        let escape = |s: &str| {
            s.chars()
                .flat_map(|c| {
                    if c.is_control() {
                        c.escape_default().collect::<Vec<_>>()
                    } else {
                        vec![c]
                    }
                })
                .collect::<String>()
        };
        let units = self.units();
        let mut output = String::new();
        let mut deferred = Vec::new();
        let errors = self
            .messages
            .iter()
            .filter(|s| s.contains("error:") || s.contains("search error"))
            .count();
        let footer = |refs: &Vec<String>| {
            format!(
                "# scope=heuristic deferred={} errors={errors} capture_omitted={}{}\n",
                refs.len(),
                self.capture_omitted,
                if refs.is_empty() {
                    String::new()
                } else {
                    format!(
                        " {}",
                        refs.iter()
                            .take(3)
                            .map(|r| format!("deferred {r}"))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                }
            ) + &if self.scope_limited > 0 {
                format!(
                    "# scope_limited={} (local scopes; not full functions)\n",
                    self.scope_limited
                )
            } else {
                String::new()
            }
        };
        let notes = self
            .messages
            .iter()
            .map(|s| format!("# {}\n", escape(s)))
            .collect::<String>();
        let mut seen: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (path, range, _) in units {
            let covered = seen.entry(path.clone()).or_default();
            let novel: Vec<_> = range.clone().filter(|n| !covered.contains(n)).collect();
            if novel.is_empty() {
                continue;
            }
            let reference = format!("{}:{}-{}", escape(&path), range.start + 1, range.end);
            let mut body = String::new();
            let mut cursor = 0;
            while cursor < novel.len() {
                let start = cursor;
                cursor += 1;
                while cursor < novel.len() && novel[cursor] == novel[cursor - 1] + 1 {
                    cursor += 1;
                }
                body.push_str(&format!(
                    "@ {}:{}-{}\n",
                    escape(&path),
                    novel[start] + 1,
                    novel[cursor - 1] + 1
                ));
                for &n in &novel[start..cursor] {
                    body.push_str(&format!("{}:{}\n", n + 1, escape(&self.files[&path][n])));
                }
            }
            // Reserve a short footer; never present half a requested block as complete.
            if tokenizer
                .encode_ordinary(&format!("{notes}{output}{body}{}", footer(&deferred)))
                .len()
                + 48
                <= budget
            {
                output.push_str(&body);
                covered.extend(novel);
            } else {
                deferred.push(reference);
            }
        }
        let mut suffix = footer(&deferred);
        if tokenizer
            .encode_ordinary(&format!("{output}{suffix}"))
            .len()
            > budget
            && !deferred.is_empty()
        {
            suffix = format!(
                "# scope=heuristic deferred={} errors={errors} capture_omitted={}\n",
                deferred.len(),
                self.capture_omitted
            );
            if self.scope_limited > 0 {
                suffix.push_str(&format!(
                    "# scope_limited={} (local scopes; not full functions)\n",
                    self.scope_limited
                ));
            }
        }
        let combined = format!("{notes}{output}{suffix}");
        if tokenizer.encode_ordinary(&combined).len() <= budget {
            combined
        } else if tokenizer
            .encode_ordinary(&format!("{output}{suffix}"))
            .len()
            <= budget
        {
            format!("{output}{suffix}")
        } else {
            format!("# deferred evidence exceeds budget ({budget})\n")
        }
    }
}

#[cfg(test)]
mod review_race_tests {
    use super::*;
    #[test]
    fn stale_capture_is_rejected_before_scopes_are_returned() {
        let d = tempfile::TempDir::new().unwrap();
        let path = d.path().join("x.rs");
        fs::write(&path, "fn current() {}\n").unwrap();
        let mut capture = Capture::default();
        capture.push(Line {
            path: std::sync::Arc::new(path.to_string_lossy().into_owned()),
            number: 1,
            text: "fn old() {}".into(),
            ranges: vec![],
            selected: true,
        });
        let mut e = Evidence::default();
        let candidates = e.review_candidates(capture, "old");
        assert!(candidates.is_empty());
        assert!(e.messages.iter().any(|m| m.contains("source changed")));
    }
}
