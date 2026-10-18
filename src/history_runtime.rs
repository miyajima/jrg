//! Per-request history integration. Source is always read from the current tree.
use crate::{
    cli::Args,
    history::{self, Mode, Repo, Store},
};
use clap::{CommandFactory, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

pub type Delivered = BTreeMap<String, Vec<(usize, String)>>;

pub struct Rendered {
    pub text: String,
    pub history: Option<Runtime>,
}

pub struct Runtime {
    pub request_id: String,
    pub context_id: String,
    repo: Repo,
    store: Store,
    mode: Mode,
    query_key: String,
    conditions: Value,
    started: Instant,
    ratings: Vec<Value>,
    results: Vec<Value>,
    applied: usize,
    decisions: Vec<Value>,
    pub tracking_omitted: usize,
    sources: BTreeMap<PathBuf, Option<Snapshot>>,
    file_operations: BTreeMap<PathBuf, BTreeSet<usize>>,
    current_operation: Option<usize>,
    prepared: bool,
}
struct Snapshot {
    path: String,
    file_hash: String,
    lines: Vec<String>,
    anchors: Vec<crate::syntax::Anchor>,
}

impl Runtime {
    pub fn begin(args: &Args, batch: Option<&Value>) -> Option<Self> {
        let mode = history::effective_mode(args.history_mode);
        if !args.smart || mode == Mode::Off {
            return None;
        }
        let build = || -> std::io::Result<Self> {
            let cwd = std::env::current_dir()?.canonicalize()?;
            let repo = Repo::discover(&cwd)?;
            let store = Store::open(&repo)?;
            let operations = batch.map(|v| Self::relative_value(v.clone(), &repo.root, &cwd));
            let requested_paths: Vec<String> = if let Some(operations) = operations.as_ref() {
                operations["operations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|op| {
                        if let Some(path) = op["read"]["path"].as_str() {
                            vec![path.to_owned()]
                        } else {
                            op["search"]["paths"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        }
                    })
                    .collect()
            } else {
                args.inputs(true)
                    .1
                    .iter()
                    .map(|p| Self::relative_path(p, &repo.root, &cwd))
                    .collect()
            };
            let reuse_allowed = requested_paths.iter().all(|p| {
                let relative = Path::new(p);
                if relative.is_absolute()
                    || relative
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return false;
                }
                let joined = repo.root.join(relative);
                for ancestor in joined.ancestors() {
                    if ancestor == repo.root {
                        break;
                    }
                    if ancestor.join(".git").exists() {
                        return false;
                    }
                }
                true
            });
            let conditions = json!({
                "policy":"history-score-v1", "syntax":"ast-grep-0.45.1-anchor-v1",
                "cwd":cwd.strip_prefix(&repo.root).unwrap_or(&cwd).to_string_lossy(),
                "embedding": if std::env::var_os("JRG_EMBEDDING_SOCKET").is_some() {
                    "google/embeddinggemma-2@914f7f89142e33e77833254d9c9b90c3cef7303b/Document/windows80-4000-overlap20"
                } else { "unconfigured" },
                "operations":operations,"requested_paths":requested_paths,
                "patterns":args.patterns,"pattern":args.pattern,
                "paths":args.paths.iter().map(|p| Self::relative_path(p,&repo.root,&cwd)).collect::<Vec<_>>(),
                "intent":args.intent.to_possible_value().unwrap().get_name(),
                "engine":args.engine.to_possible_value().unwrap().get_name(),
                "encoding":args.encoding.to_possible_value().unwrap().get_name(),
                "rank_query":args.rank_query, "context":[args.context,args.before_context,args.after_context],
                "scope":args.explain_scope.to_possible_value().unwrap().get_name(),
                "flags":[args.regex,args.no_normalize,args.no_kana,args.ignore_case,args.smart_case,
                    args.hidden,args.no_ignore,args.text,args.invert_match,args.fixed_strings],
                "globs":args.globs,"max_count":args.max_count,"max_tokens":args.max_tokens,
                "discovery_limit":args.discovery_limit,"block_context":args.block_context,
            });
            let query_key = history::digest(&conditions.to_string());
            let ratings = if mode == Mode::Auto && !args.resume && reuse_allowed {
                store.ratings(&query_key)?
            } else {
                Vec::new()
            };
            Ok(Self {
                request_id: history::new_id(),
                context_id: args.context_id.clone().unwrap_or_else(history::new_id),
                repo,
                store,
                mode,
                query_key,
                conditions,
                started: Instant::now(),
                ratings,
                results: Vec::new(),
                applied: 0,
                sources: BTreeMap::new(),
                file_operations: BTreeMap::new(),
                current_operation: Some(1),
                prepared: false,
                decisions: Vec::new(),
                tracking_omitted: 0,
            })
        };
        match build() {
            Ok(value) => Some(value),
            Err(e) => {
                eprintln!("jrg history unavailable: {e}");
                None
            }
        }
    }

    fn relative_path(path: &Path, root: &Path, cwd: &Path) -> String {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            cwd.join(path)
        };
        let path = path.canonicalize().unwrap_or(path);
        path.strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned()
    }
    fn relative_value(mut v: Value, root: &Path, cwd: &Path) -> Value {
        match &mut v {
            Value::Object(map) => {
                for (key, value) in map {
                    if key == "search" && value.is_array() {
                        let argv: Vec<_> = value
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(Value::as_str)
                            .collect();
                        if let Ok(matches) = Args::command().try_get_matches_from(
                            std::iter::once("jrg")
                                .chain(std::iter::once("--smart"))
                                .chain(argv),
                        ) {
                            let args = Args::prepared(&matches);
                            let (patterns, paths) = args.inputs(true);
                            *value = json!({"patterns":patterns,"paths":paths.iter().map(|p|Self::relative_path(p,root,cwd)).collect::<Vec<_>>(),
                            "intent":args.intent.to_possible_value().unwrap().get_name(),"regex":args.regex,
                            "ignore_case":args.ignore_case,"smart_case":args.smart_case,"normalize":!args.no_normalize,
                            "kana":!args.no_kana,"hidden":args.hidden,"no_ignore":args.no_ignore,"treat_binary_as_text":args.text,
                            "invert":args.invert_match,"encoding":args.encoding.to_possible_value().unwrap().get_name(),
                            "engine":args.engine.to_possible_value().unwrap().get_name(),"globs":args.globs,
                            "context":[args.context,args.before_context,args.after_context],"block_context":args.block_context,
                            "scope":args.explain_scope.to_possible_value().unwrap().get_name(),"max_count":args.max_count});
                        }
                    } else if key == "paths" {
                        if let Some(array) = value.as_array_mut() {
                            for path in array {
                                if let Some(s) = path.as_str() {
                                    *path = Self::relative_path(Path::new(s), root, cwd).into();
                                }
                            }
                        }
                    } else if key == "path" {
                        if let Some(s) = value.as_str() {
                            *value = Self::relative_path(Path::new(s), root, cwd).into();
                        }
                    } else {
                        *value = Self::relative_value(value.take(), root, cwd);
                    }
                }
            }
            Value::Array(array) => {
                for item in array {
                    *item = Self::relative_value(item.take(), root, cwd);
                }
            }
            _ => {}
        }
        v
    }

    pub fn operation(&mut self, number: Option<usize>) {
        self.current_operation = number;
    }
    pub fn note_file(&mut self, path: &Path) {
        if let Some(number) = self.current_operation {
            self.file_operations
                .entry(path.to_owned())
                .or_default()
                .insert(number);
        }
    }

    fn snapshot(&mut self, path: &str) -> Option<&Snapshot> {
        let path = fs::canonicalize(path).ok()?;
        if !self.sources.contains_key(&path) {
            let load = || -> Option<Snapshot> {
                if !path.starts_with(&self.repo.root)
                    || !path.is_file()
                    || fs::metadata(&path).ok()?.len() > 4 * 1024 * 1024
                {
                    return None;
                }
                let mut parent = path.parent();
                while let Some(dir) = parent {
                    if dir == self.repo.root {
                        break;
                    }
                    if dir.join(".git").exists() {
                        return None;
                    }
                    parent = dir.parent();
                }
                let raw = fs::read_to_string(&path).ok()?;
                let lines: Vec<String> = raw
                    .strip_prefix('\u{feff}')
                    .unwrap_or(&raw)
                    .lines()
                    .map(str::to_owned)
                    .collect();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                let anchors = crate::syntax::Analysis::get(&lines.join("\n"), ext)
                    .map_or_else(Vec::new, |a| a.anchors.clone());
                Some(Snapshot {
                    path: path
                        .strip_prefix(&self.repo.root)
                        .ok()?
                        .to_string_lossy()
                        .into_owned(),
                    file_hash: history::digest(&raw),
                    lines,
                    anchors,
                })
            };
            self.sources.insert(path.clone(), load());
        }
        self.sources.get(&path).and_then(Option::as_ref)
    }

    pub fn factor(&mut self, path: &str, range: Option<std::ops::Range<usize>>) -> f64 {
        if self.mode != Mode::Auto || self.ratings.is_empty() {
            return 1.0;
        }
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => return 1.0,
        };
        let Some(relative) = canonical
            .strip_prefix(&self.repo.root)
            .ok()
            .and_then(Path::to_str)
        else {
            return 1.0;
        };
        let ratings: Vec<_> = self
            .ratings
            .iter()
            .filter(|r| r["path"].as_str() == Some(relative))
            .cloned()
            .collect();
        if ratings.is_empty() {
            return 1.0;
        }
        let Some(source) = self.snapshot(path) else {
            return 1.0;
        };
        let mut values = BTreeMap::new();
        for rating in ratings {
            if rating["path"].as_str() != Some(&source.path) {
                continue;
            }
            let Some(value) = rating["value"].as_f64().filter(|v| v.is_finite()) else {
                continue;
            };
            let start = rating["start"].as_u64().unwrap_or(0) as usize;
            let end = rating["end"].as_u64().unwrap_or(0) as usize;
            let bounds = if rating["anchor_kind"].as_str() == Some("function") {
                let matches: Vec<_> = source
                    .anchors
                    .iter()
                    .filter(|a| {
                        Some(a.hash.as_str()) == rating["anchor_hash"].as_str()
                            && Some(a.context_hash.as_str()) == rating["context_hash"].as_str()
                    })
                    .collect();
                if matches.len() != 1 {
                    continue;
                }
                matches[0].start..matches[0].end
            } else {
                if rating["file_hash"].as_str() != Some(&source.file_hash) {
                    continue;
                }
                start..end
            };
            if range
                .as_ref()
                .is_some_and(|r| r.start >= bounds.end || r.end <= bounds.start)
            {
                continue;
            }
            values.insert(
                rating["anchor_key"].as_str().unwrap_or("").to_owned(),
                value.clamp(-1.0, 1.0),
            );
        }
        if values.is_empty() {
            return 1.0;
        }
        let value = values.values().sum::<f64>() / values.len() as f64;
        if value != 0.0 {
            self.applied += 1;
        }
        1.0 + 0.1 * value
    }

    pub fn adjust(
        &mut self,
        path: &str,
        range: Option<std::ops::Range<usize>>,
        score: f64,
        stage: &str,
    ) -> f64 {
        if score <= 0.0 {
            return score;
        }
        let adjusted = score * self.factor(path, range.clone());
        if self.decisions.len() < 128 && adjusted != score {
            self.decisions.push(
                json!({"stage":stage,"path":path,"range":range.map(|r|[r.start,r.end]),
                "base_score":score,"adjusted_score":adjusted}),
            );
        }
        adjusted
    }
    pub fn disable_reuse(&mut self, reason: &str) {
        self.ratings.clear();
        self.conditions["fallback"] = reason.into();
        self.query_key = history::digest(&self.conditions.to_string());
    }

    pub fn capture(&mut self, rows: Delivered, originals: Option<&BTreeMap<String, Vec<String>>>) {
        for (path, mut rows) in rows {
            let operation_ids = fs::canonicalize(&path)
                .ok()
                .and_then(|p| self.file_operations.get(&p))
                .map(|s| s.iter().copied().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![1]);
            let requirements = self.conditions["operations"]["requirements"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            rows.sort_by_key(|(n, _)| *n);
            rows.dedup_by_key(|(n, _)| *n);
            let Some(source) = self.snapshot(&path) else {
                continue;
            };
            if fs::read_to_string(&path)
                .ok()
                .is_none_or(|s| history::digest(&s) != source.file_hash)
            {
                continue;
            }
            if originals
                .and_then(|m| m.get(&path))
                .is_some_and(|lines| lines != &source.lines)
            {
                continue;
            }
            rows.retain(|(n, text)| source.lines.get(*n) == Some(text));
            let mut groups: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
            for (n, _) in rows {
                let anchor = source
                    .anchors
                    .iter()
                    .filter(|a| a.start <= n && n < a.end)
                    .min_by_key(|a| a.end - a.start);
                let key = anchor.map_or((n, n + 1), |a| (a.start, a.end));
                groups.entry(key).or_default().push(n);
            }
            let file_hash = source.file_hash.clone();
            let relative = source.path.clone();
            let mut records = Vec::new();
            for ((start, end), shown) in groups {
                let requirement_ids: Vec<_> = requirements
                    .iter()
                    .filter(|r| {
                        r["patterns"].as_array().is_some_and(|ps| {
                            ps.iter()
                                .filter_map(Value::as_str)
                                .any(|p| shown.iter().any(|n| source.lines[*n].contains(p)))
                        })
                    })
                    .filter_map(|r| r["id"].as_str())
                    .collect();
                let anchor = source
                    .anchors
                    .iter()
                    .find(|a| a.start == start && a.end == end);
                let (hash, context, kind) = anchor.map_or_else(
                    || {
                        (
                            history::digest(&source.lines[start..end].join("\n")),
                            history::digest(""),
                            "range",
                        )
                    },
                    |a| (a.hash.clone(), a.context_hash.clone(), "function"),
                );
                let anchor_key = history::digest(&format!("{relative}|{kind}|{hash}|{context}"));
                records.push(json!({"result_id":history::new_id(),"anchor_key":anchor_key,"path":relative,
                    "file_hash":file_hash,"anchor_hash":hash,"context_hash":context,"anchor_kind":kind,
                    "start":start,"end":end,"ranges":Self::ranges(&shown),"partial":shown.len()!=end-start,
                    "requirement_ids":requirement_ids,"file_operation_ids":operation_ids}));
            }
            self.results.extend(records);
        }
    }

    fn ranges(rows: &[usize]) -> Vec<[usize; 2]> {
        let mut out: Vec<[usize; 2]> = Vec::new();
        for &n in rows {
            if let Some(last) = out.last_mut().filter(|r| r[1] == n) {
                last[1] = n + 1;
            } else {
                out.push([n, n + 1]);
            }
        }
        out
    }

    pub fn prepare(&mut self, bytes: usize, code: u8) -> bool {
        let record = json!({"request_id":self.request_id,"query_key":self.query_key,"context_id":self.context_id,
            "conditions":self.conditions,"results":self.results,"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.0,
            "output_bytes":bytes,"exit_code":code,"history_applied":self.applied,"decisions":self.decisions,
            "tracking_omitted":self.tracking_omitted});
        match self.store.prepare(&record) {
            Ok(()) => {
                self.prepared = true;
                true
            }
            Err(e) => {
                eprintln!("jrg history unavailable: {e}");
                false
            }
        }
    }
    pub fn decorate(
        &self,
        text: &mut String,
        tokenizer: &tiktoken_rs::CoreBPE,
        budget: usize,
    ) -> Option<Value> {
        if !self.prepared {
            return None;
        }
        let cwd = self
            .repo
            .root
            .join(self.conditions["cwd"].as_str().unwrap_or(""));
        let meta = json!({"history":{"request_id":self.request_id,"mode":format!("{:?}",self.mode).to_lowercase(),
            "results":self.results.iter().map(|r| {
                let path=self.repo.root.join(r["path"].as_str().unwrap_or(""));
                let path=path.strip_prefix(&cwd).unwrap_or(&path).to_string_lossy();
                json!({"result_id":r["result_id"],"path":path,"repo_path":r["path"],"ranges":r["ranges"]})
            }).collect::<Vec<_>>(),
            "applied":self.applied}});
        let footer = format!(
            "# history request={} results={} applied={}\n",
            self.request_id,
            self.results.len(),
            self.applied
        );
        let combined = format!("{text}{footer}{}", meta);
        if combined.len() <= 96000 && tokenizer.encode_ordinary(&combined).len() <= budget {
            text.push_str(&footer);
            Some(meta)
        } else {
            None
        }
    }
    pub fn emitted(&mut self) {
        if self.prepared
            && let Err(e) = self.store.emitted(&self.request_id)
        {
            eprintln!("jrg history delivery unrecorded: {e}");
        }
    }
    pub fn rows_from(
        files: &BTreeMap<String, Vec<String>>,
        shown: &BTreeMap<String, BTreeSet<usize>>,
    ) -> Delivered {
        shown
            .iter()
            .map(|(path, rows)| {
                (
                    path.clone(),
                    rows.iter()
                        .filter_map(|n| files.get(path)?.get(*n).map(|s| (*n, s.clone())))
                        .collect(),
                )
            })
            .collect()
    }
}
