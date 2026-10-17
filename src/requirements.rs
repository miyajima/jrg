//! Deterministic request-to-source links. Never a semantic coverage judgment.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    ops::Range,
    path::Path,
};

#[derive(Clone, Default)]
pub struct Requirements(Vec<Item>);
#[derive(Clone)]
struct Item {
    id: String,
    description: String,
    patterns: Vec<String>,
}
struct Record {
    score: usize,
    path: String,
    rows: Vec<usize>,
    condition: bool,
    incomplete: bool,
    reference: Range<usize>,
}
pub struct MapOutput {
    pub text: String,
    pub delivered: BTreeMap<String, BTreeSet<usize>>,
}
struct ItemRecords {
    matches: usize,
    conditions: usize,
    incomplete: usize,
    unavailable: bool,
    omitted: usize,
    records: VecDeque<Record>,
}

impl Requirements {
    pub fn parse(value: Option<&Value>, budget: usize) -> io::Result<Self> {
        let invalid = |s: &str| io::Error::new(io::ErrorKind::InvalidInput, s);
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let array = value
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 16)
            .ok_or_else(|| invalid("requirements needs 1..16 items"))?;
        let mut ids = BTreeSet::new();
        let mut items = Vec::new();
        for value in array {
            let object = value
                .as_object()
                .filter(|o| {
                    o.len() == 3
                        && o.contains_key("id")
                        && o.contains_key("description")
                        && o.contains_key("patterns")
                })
                .ok_or_else(|| invalid("requirement needs only id, description, patterns"))?;
            let id = object["id"]
                .as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 32
                        && s.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                })
                .ok_or_else(|| {
                    invalid("requirement id needs 1..32 ASCII letters, digits, _ or -")
                })?;
            if !ids.insert(id) {
                return Err(invalid("duplicate requirement id"));
            }
            let description = object["description"]
                .as_str()
                .filter(|s| {
                    !s.trim().is_empty()
                        && s.chars().count() <= 160
                        && !s.chars().any(char::is_control)
                })
                .ok_or_else(|| {
                    invalid("requirement description needs 1..160 non-control characters")
                })?;
            let patterns = object["patterns"]
                .as_array()
                .filter(|a| !a.is_empty() && a.len() <= 16)
                .ok_or_else(|| invalid("requirement patterns needs 1..16 literal strings"))?;
            let patterns = patterns
                .iter()
                .map(|p| {
                    p.as_str()
                        .filter(|s| {
                            !s.trim().is_empty()
                                && s.chars().count() <= 128
                                && !s.chars().any(char::is_control)
                        })
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            invalid("requirement pattern needs 1..128 non-control characters")
                        })
                })
                .collect::<io::Result<Vec<_>>>()?;
            items.push(Item {
                id: id.into(),
                description: description.into(),
                patterns,
            });
        }
        let result = Self(items);
        let tokenizer = tiktoken_rs::o200k_base_singleton();
        // All item statuses and the existing retrieval footer must survive.
        if tokenizer.encode_ordinary(&result.skeleton()).len() + 256 > budget {
            return Err(invalid(
                "max_tokens too small for requirement statuses and retrieval footer",
            ));
        }
        Ok(result)
    }
    fn header(&self) -> String {
        "# requirement_map literal links; not semantic coverage. Check every item and condition in the answer. Conditions may repeat seen source.\n".into()
    }
    fn skeleton(&self) -> String {
        let mut text = self.header();
        for item in &self.0 {
            text.push_str(&format!("# item {} {} state=unresolved matches=999999 conditions=999999 conditions_incomplete=999999 omitted=999999 syntax=unavailable\n", item.id, item.description));
        }
        text
    }
    pub fn render(
        &self,
        files: &BTreeMap<String, Vec<String>>,
        selected: &BTreeMap<String, BTreeSet<usize>>,
        counter: &crate::budget::Counter<'_>,
        budget: usize,
    ) -> MapOutput {
        let mut output = MapOutput {
            text: String::new(),
            delivered: BTreeMap::new(),
        };
        if self.0.is_empty() {
            return output;
        }
        let skeleton = self.skeleton();
        let base = counter.count(&skeleton);
        let limit = (budget / 3).min(2048).max(base + 32);
        let mut details = String::new();
        let mut statuses = self.header();
        let predicates: BTreeMap<_, _> = selected
            .keys()
            .map(|path| {
                let ext = Path::new(path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                (
                    path,
                    crate::syntax::Analysis::get(&files[path].join("\n"), ext)
                        .map(|a| a.predicates.clone()),
                )
            })
            .collect();
        let mut groups = Vec::new();
        for item in &self.0 {
            let mut records = Vec::new();
            let mut matches = 0;
            let mut conditions = 0;
            let mut incomplete = 0;
            let mut unavailable = false;
            for (path, rows) in selected {
                let hits: Vec<_> = rows
                    .iter()
                    .copied()
                    .filter(|n| item.patterns.iter().any(|p| files[path][*n].contains(p)))
                    .collect();
                if hits.is_empty() {
                    continue;
                }
                matches += hits.len();
                let mut linked = BTreeSet::new();
                if let Some(predicates) = &predicates[path] {
                    for predicate in predicates {
                        if !hits.iter().any(|n| predicate.branch.contains(n)) {
                            continue;
                        }
                        conditions += 1;
                        let complete = predicate.rows.clone().all(|n| rows.contains(&n));
                        incomplete += usize::from(!complete);
                        if complete {
                            linked.extend(predicate.rows.clone());
                        }
                        records.push(Record {
                            score: item
                                .patterns
                                .iter()
                                .filter(|p| {
                                    predicate.rows.clone().any(|n| {
                                        rows.contains(&n) && files[path][n].contains(p.as_str())
                                    })
                                })
                                .count(),
                            path: path.clone(),
                            rows: if complete {
                                predicate.rows.clone().collect()
                            } else {
                                vec![]
                            },
                            condition: true,
                            incomplete: !complete,
                            reference: predicate.rows.clone(),
                        });
                    }
                } else {
                    unavailable = true;
                }
                for n in hits.into_iter().filter(|n| !linked.contains(n)) {
                    records.push(Record {
                        score: 0,
                        path: path.clone(),
                        rows: vec![n],
                        condition: false,
                        incomplete: false,
                        reference: n..n + 1,
                    });
                }
            }
            // Whole predicates precede standalone matched lines. Each record is atomic.
            records.sort_by_key(|r| (!r.condition, std::cmp::Reverse(r.score)));
            groups.push(ItemRecords {
                matches,
                conditions,
                incomplete,
                unavailable,
                omitted: 0,
                records: records.into(),
            });
        }
        // Give each requested item its strongest record before filling extras.
        while groups.iter().any(|g| !g.records.is_empty()) {
            for (item, group) in self.0.iter().zip(&mut groups) {
                let Some(record) = group.records.pop_front() else {
                    continue;
                };
                let shared = !record.rows.is_empty()
                    && output
                        .delivered
                        .get(&record.path)
                        .is_some_and(|rows| record.rows.iter().all(|n| rows.contains(n)));
                let escaped_path = record
                    .path
                    .chars()
                    .flat_map(|c| {
                        if c.is_control() {
                            c.escape_default().collect::<Vec<_>>()
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>();
                let mut block = format!(
                    "# {} {} {}:{}-{}{}{}\n",
                    if record.condition {
                        "condition"
                    } else {
                        "evidence"
                    },
                    item.id,
                    escaped_path,
                    record.reference.start + 1,
                    record.reference.end,
                    if record.incomplete {
                        " not_fully_retrieved"
                    } else {
                        ""
                    },
                    if shared { " shared_source" } else { "" }
                );
                if !record.rows.is_empty() && !shared {
                    block.push_str(&format!(
                        "@ {escaped_path}:{}-{}\n",
                        record.reference.start + 1,
                        record.reference.end
                    ));
                    for n in &record.rows {
                        let line = files[&record.path][*n]
                            .chars()
                            .flat_map(|c| {
                                if c.is_control() {
                                    c.escape_default().collect::<Vec<_>>()
                                } else {
                                    vec![c]
                                }
                            })
                            .collect::<String>();
                        block.push_str(&format!("{}:{line}\n", n + 1));
                    }
                }
                if skeleton.len() + details.len() + block.len() <= 48_000
                    && counter.count(&format!("{skeleton}{details}{block}")) <= limit
                {
                    details.push_str(&block);
                    output
                        .delivered
                        .entry(record.path)
                        .or_default()
                        .extend(record.rows);
                } else {
                    group.omitted += 1;
                }
            }
        }
        for (item, group) in self.0.iter().zip(groups) {
            statuses.push_str(&format!("# item {} {} state={} matches={} conditions={} conditions_incomplete={} omitted={} syntax={}\n", item.id, item.description, if group.matches == 0 { "unresolved" } else { "matched" }, group.matches, group.conditions, group.incomplete, group.omitted, if group.matches == 0 { "not_checked" } else if group.unavailable { "unavailable" } else { "ast" }));
        }
        output.text = statuses + &details;
        output
    }
}
