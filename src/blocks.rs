//! Bounded lexical source scopes, not an AST or a guarantee of semantic closure.
use crate::{
    cli::{Args, Encoding, ExplainScope},
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

#[derive(Default)]
pub struct Capture {
    pub lines: Vec<Line>,
    pub omitted: usize,
    bytes: usize,
}
impl Capture {
    pub fn push(&mut self, line: Line) {
        if self.lines.len() >= 4096 || self.bytes.saturating_add(line.text.len()) > MAX_CAPTURE {
            self.omitted += 1;
        } else {
            self.bytes += line.text.len();
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

fn header(lines: &[&str], open: usize, rust: bool) -> (usize, bool) {
    for start in (open.saturating_sub(12)..=open).rev() {
        if start < open
            && (lines[start].contains(['{', '}']) || lines[start].trim_end().ends_with(';'))
        {
            break;
        }
        if declaration(lines[start], rust) {
            let mut first = start;
            while first > 0
                && (lines[first - 1].trim_start().starts_with("#[")
                    || lines[first - 1].trim_start().starts_with("///"))
            {
                first -= 1;
            }
            return (first, true);
        }
    }
    (open, false)
}

fn brace_scopes(source: &str, lines: &[&str], rust: bool) -> Vec<Scope> {
    let b = source.as_bytes();
    let mut i = 0;
    let mut line = 0;
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
            let (start, function) = header(lines, line, rust);
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
    fn parse(source: &str, lines: &[&str], extension: &str) -> Vec<Self> {
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

#[derive(Default)]
pub struct Evidence {
    pub rank_query: Option<String>,
    files: BTreeMap<String, Vec<String>>,
    ranges: BTreeMap<String, Vec<(Range<usize>, u8)>>,
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
    // Merge adjacent hit/context/read rows, while preserving independently
    // useful function boundaries for the constrained-budget fallback.
    fn units(&self) -> Vec<(String, Range<usize>, u8)> {
        let mut units = Vec::new();
        for (path, ranges) in &self.ranges {
            for priority in [SCOPE, HIT, READ] {
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
        let mut tracked = BTreeMap::new();
        let mut changed = 0;
        let mut untracked = 0;
        if let Some(state) = session.as_deref_mut() {
            for path in self.ranges.keys().cloned().collect::<Vec<_>>() {
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
        let errors = self
            .messages
            .iter()
            .filter(|s| s.contains("error:") || s.contains("search error"))
            .count();
        let footer = |pending: usize, seen: usize, partial: usize| {
            format!(
                "# scope=heuristic deferred={pending} errors={errors} capture_omitted={} seen={seen} pending={pending} partial={partial} changed={changed} untracked={untracked}\n",
                self.capture_omitted
            ) + &if self.rank_query.is_some() {
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
        let reserve = footer(999999, 999999, 999999);
        let fits = |s: &str| tokenizer.encode_ordinary(&format!("{s}{reserve}")).len() <= budget;
        let units = self.units();
        let mut wanted: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut shown: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
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
                } else {
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
            shown = full_rows;
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
        // Notes are optional; the error/capture/partial counters always survive.
        for note in &self.messages {
            let line = format!("# {}\n", Self::escape(note));
            if tokenizer
                .encode_ordinary(&format!("{output}{line}{suffix}"))
                .len()
                <= budget
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
