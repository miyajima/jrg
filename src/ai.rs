//! Loss-aware, bounded collection and deterministic rendering for model input.
use crate::search::Line;
use std::collections::{BTreeMap, VecDeque};
use tiktoken_rs::CoreBPE;
use unicode_segmentation::UnicodeSegmentation;

const MAX_GROUPS: usize = 4096;
const MAX_REFERENCE_RANGES: usize = 256;
type GroupKey = (bool, String, String, u64);
type DisplayKey = (String, u64, bool);

pub struct AiOutput {
    groups: BTreeMap<GroupKey, Record>,
    matched: u64,
    context: u64,
    snippet_chars: usize,
}

#[derive(Clone)]
struct Record {
    path: String,
    numbers: Locations,
    selected: bool,
    snippet: Snippet,
}

#[derive(Clone)]
struct Snippet {
    body: String,
    column: usize,
    focus: usize,
    left: bool,
    right: bool,
}

#[derive(Clone, Default)]
struct Locations(BTreeMap<u64, u64>);

impl Locations {
    fn insert(&mut self, number: u64) {
        let mut start = number;
        let mut end = number;
        if let Some((&left, &right)) = self.0.range(..=number).next_back() {
            if right >= number {
                return;
            }
            if right.checked_add(1) == Some(number) {
                start = left;
                self.0.remove(&left);
            }
        }
        if let Some((&left, &right)) = self.0.range(number..).next()
            && number.checked_add(1) == Some(left)
        {
            end = right;
            self.0.remove(&left);
        }
        self.0.insert(start, end);
        if self.0.len() > MAX_REFERENCE_RANGES {
            self.0.pop_last();
        }
    }

    fn first(&self) -> u64 {
        *self.0.first_key_value().unwrap().0
    }

    fn count(&self) -> u64 {
        self.0.iter().map(|(start, end)| end - start + 1).sum()
    }

    fn keep_first(&mut self, mut count: u64) {
        let mut kept = BTreeMap::new();
        for (&start, &end) in &self.0 {
            if count == 0 {
                break;
            }
            let take = (end - start + 1).min(count);
            kept.insert(start, start + (take - 1));
            count -= take;
        }
        self.0 = kept;
    }

    fn text(&self) -> String {
        self.0
            .iter()
            .map(|(start, end)| {
                if start == end {
                    start.to_string()
                } else {
                    format!("{start}-{end}")
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Snippet {
    fn new(text: &str, focus: usize, limit: usize) -> Self {
        let mut boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(text.len());
        let length = boundaries.len() - 1;
        let focus_index = boundaries
            .partition_point(|i| *i <= focus)
            .saturating_sub(1);
        let start = focus_index
            .saturating_sub(limit / 3)
            .min(length.saturating_sub(limit));
        let end = start.saturating_add(limit).min(length);
        let (start, end) = (boundaries[start], boundaries[end]);
        Self {
            body: text[start..end].to_owned(),
            column: text[..start].chars().count() + 1,
            focus: focus.saturating_sub(start).min(end - start),
            left: start > 0,
            right: end < text.len(),
        }
    }

    fn shrink(&mut self, limit: usize) {
        let mut smaller = Self::new(&self.body, self.focus, limit);
        smaller.column += self.column - 1;
        smaller.left |= self.left;
        smaller.right |= self.right;
        *self = smaller;
    }

    fn clipped(&self) -> bool {
        self.left || self.right
    }

    fn text(&self) -> String {
        format!(
            "{}{}{}",
            if self.left { "…" } else { "" },
            escape_controls(&self.body),
            if self.right { "…" } else { "" }
        )
    }
}

impl AiOutput {
    pub fn new(snippet_chars: usize) -> Self {
        Self {
            groups: BTreeMap::new(),
            matched: 0,
            context: 0,
            snippet_chars,
        }
    }

    pub fn push(&mut self, line: Line) {
        if line.selected {
            self.matched = self.matched.saturating_add(1);
        } else {
            self.context = self.context.saturating_add(1);
        }
        let snippet = Snippet::new(
            &line.text,
            line.ranges.first().map_or(0, |r| r.start),
            self.snippet_chars,
        );
        // Clipped excerpts can be equal while their full source lines differ.
        // Only complete, identical source lines are deduplicated.
        let key = (
            !line.selected,
            (*line.path).clone(),
            snippet.body.clone(),
            if snippet.clipped() { line.number } else { 0 },
        );
        if let Some(record) = self.groups.get_mut(&key) {
            record.numbers.insert(line.number);
            return;
        }
        if self.groups.len() == MAX_GROUPS
            && self
                .groups
                .last_key_value()
                .is_some_and(|(last, _)| &key > last)
        {
            return;
        }
        let mut numbers = Locations::default();
        numbers.insert(line.number);
        let record = Record {
            path: (*line.path).clone(),
            numbers,
            selected: line.selected,
            snippet,
        };
        self.groups.insert(key, record);
        if self.groups.len() > MAX_GROUPS {
            self.groups.pop_last();
        }
    }

    pub fn finish(self, tokenizer: &CoreBPE, max_tokens: usize) -> String {
        self.finish_impl(tokenizer, max_tokens, false).0
    }

    pub fn finish_smart(self, tokenizer: &CoreBPE, max_tokens: usize) -> (String, bool) {
        self.finish_impl(tokenizer, max_tokens, true)
    }

    fn finish_impl(self, tokenizer: &CoreBPE, max_tokens: usize, detailed: bool) -> (String, bool) {
        let mut files: BTreeMap<String, Vec<Record>> = BTreeMap::new();
        for record in self.groups.into_values() {
            files.entry(record.path.clone()).or_default().push(record);
        }
        let mut chosen: BTreeMap<DisplayKey, Record> = BTreeMap::new();
        // Spend on matches before context, with one group per file per round.
        for selected in [true, false] {
            let mut queues: Vec<VecDeque<Record>> = files
                .values()
                .map(|records| {
                    let mut records: Vec<_> = records
                        .iter()
                        .filter(|r| r.selected == selected)
                        .cloned()
                        .collect();
                    records.sort_by_key(|r| r.numbers.first());
                    records.into()
                })
                .collect();
            while queues.iter().any(|q| !q.is_empty()) {
                for queue in &mut queues {
                    let Some(mut record) = queue.pop_front() else {
                        continue;
                    };
                    let key = (
                        record.path.clone(),
                        record.numbers.first(),
                        !record.selected,
                    );
                    let mut limit = self.snippet_chars;
                    loop {
                        chosen.insert(key.clone(), record.clone());
                        let output =
                            render(&chosen, self.matched, detailed.then_some(self.context));
                        // Retokenize the whole payload: BPE merges at boundaries
                        // mean that sums of per-line token counts are not exact.
                        if tokenizer.encode_ordinary(&output).len() <= max_tokens {
                            break;
                        }
                        chosen.remove(&key);
                        if limit <= 16 {
                            if record.numbers.count() > 1 {
                                let keep = (record.numbers.count() / 2).max(1);
                                record.numbers.keep_first(keep);
                                continue;
                            }
                            break;
                        }
                        limit = (limit / 2).max(16);
                        record.snippet.shrink(limit);
                    }
                }
            }
        }
        let shown: u64 = chosen
            .values()
            .filter(|r| r.selected)
            .map(|r| r.numbers.count())
            .sum();
        let context: u64 = chosen
            .values()
            .filter(|r| !r.selected)
            .map(|r| r.numbers.count())
            .sum();
        let complete = shown == self.matched
            && context == self.context
            && chosen.values().all(|r| !r.snippet.clipped());
        (
            render(&chosen, self.matched, detailed.then_some(self.context)),
            complete,
        )
    }
}

fn render(records: &BTreeMap<DisplayKey, Record>, matched: u64, context: Option<u64>) -> String {
    let mut out = String::new();
    let mut path = None;
    let mut shown = 0_u64;
    let mut clipped = 0_u64;
    for record in records.values() {
        if path != Some(&record.path) {
            out.push_str("@ ");
            if record.path.chars().any(char::is_control) {
                out.push_str(&serde_json::to_string(&record.path).unwrap());
            } else {
                out.push_str(&record.path);
            }
            out.push('\n');
            path = Some(&record.path);
        }
        if !record.selected {
            out.push('~');
        }
        out.push_str(&record.numbers.text());
        out.push(':');
        out.push_str(&record.snippet.column.to_string());
        out.push(':');
        out.push_str(&record.snippet.text());
        out.push('\n');
        if record.selected {
            shown += record.numbers.count();
            if record.snippet.clipped() {
                clipped += record.numbers.count();
            }
        }
    }
    out.push_str(&format!(
        "# matched={matched} shown={shown} omitted={} clipped={clipped}\n",
        matched.saturating_sub(shown)
    ));
    if let Some(expected) = context {
        let shown: u64 = records
            .values()
            .filter(|r| !r.selected)
            .map(|r| r.numbers.count())
            .sum();
        let clipped: u64 = records
            .values()
            .filter(|r| !r.selected && r.snippet.clipped())
            .map(|r| r.numbers.count())
            .sum();
        out.push_str(&format!(
            "# context_omitted={} context_clipped={clipped}\n",
            expected.saturating_sub(shown)
        ));
    }
    out
}

fn escape_controls(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, OnceLock};

    fn tokenizer() -> &'static CoreBPE {
        static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();
        TOKENIZER.get_or_init(|| tiktoken_rs::o200k_base().unwrap())
    }

    #[test]
    fn cap_and_reference_limit_are_reported_in_omitted_count() {
        let mut output = AiOutput::new(160);
        for n in 1..=300 {
            output.push(Line {
                path: Arc::new("a".into()),
                number: n * 2,
                text: "hit".into(),
                ranges: std::iter::once(0..3).collect(),
                selected: true,
            });
        }
        let rendered = output.finish(tokenizer(), 4000);
        assert!(rendered.contains("matched=300 shown=256 omitted=44"));
        let mut output = AiOutput::new(160);
        for n in 0..MAX_GROUPS + 5 {
            output.push(Line {
                path: Arc::new("a".into()),
                number: n as u64 + 1,
                text: format!("hit {n}"),
                ranges: std::iter::once(0..3).collect(),
                selected: true,
            });
        }
        assert_eq!(output.groups.len(), MAX_GROUPS);
        assert_eq!(output.matched, (MAX_GROUPS + 5) as u64);
    }

    #[test]
    fn contiguous_duplicate_locations_cost_constant_space() {
        let mut locations = Locations::default();
        for n in 1..=100_000 {
            locations.insert(n);
        }
        assert_eq!(locations.0.len(), 1);
        assert_eq!(locations.count(), 100_000);
        assert_eq!(locations.text(), "1-100000");
        let mut unordered = Locations::default();
        for n in [5, 1, 2, 4, 3, 3] {
            unordered.insert(n);
        }
        assert_eq!(unordered.text(), "1-5");
    }

    #[test]
    fn even_extreme_counts_fit_the_minimum_budget() {
        let mut output = AiOutput::new(160);
        output.matched = u64::MAX;
        let rendered = output.finish(tokenizer(), 64);
        assert!(tokenizer().encode_ordinary(&rendered).len() <= 64);
    }

    #[test]
    fn cropping_keeps_graphemes_and_original_character_columns() {
        let source = format!(
            "{}前か\u{3099}👩‍💻hit後{}",
            "あ".repeat(100),
            "い".repeat(100)
        );
        let snippet = Snippet::new(&source, source.find("hit").unwrap(), 16);
        assert!(snippet.body.contains("か\u{3099}👩‍💻hit"));
        assert!(snippet.column > 1);
        assert!(snippet.left && snippet.right);
        assert_eq!(snippet.body.graphemes(true).count(), 16);
    }
}
