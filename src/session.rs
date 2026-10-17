//! Explicit conversation-local delivery receipts. No source text is persisted.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Entry {
    signature: String,
    pub seen: Vec<Range<usize>>,
    pub pending: Vec<Range<usize>>,
}
pub struct Session {
    path: PathBuf,
    root: PathBuf,
    lock: PathBuf,
    entries: BTreeMap<String, Entry>,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock);
    }
}
impl Session {
    pub fn open(path: &Path) -> io::Result<Self> {
        let absolute = std::path::absolute(path)?;
        let parent = fs::canonicalize(
            absolute
                .parent()
                .ok_or_else(|| io::Error::other("invalid session path"))?,
        )?;
        let path = parent.join(
            absolute
                .file_name()
                .ok_or_else(|| io::Error::other("invalid session filename"))?,
        );
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_file()) {
            return Err(io::Error::other(
                "session must be a regular non-symlink file",
            ));
        }
        let lock = PathBuf::from(format!("{}.lock", path.display()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&lock)
            .map_err(|e| io::Error::other(format!("session lock unavailable: {e}")))?;
        writeln!(file, "{}", std::process::id())?;
        let mut session = Self {
            path,
            root: fs::canonicalize(std::env::current_dir()?)?,
            lock,
            entries: BTreeMap::new(),
        };
        match File::open(&session.path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
            Ok(file) => {
                let mut text = String::new();
                file.take(8 * 1024 * 1024 + 1).read_to_string(&mut text)?;
                if text.len() > 8 * 1024 * 1024 {
                    return Err(io::Error::other("session exceeds 8 MiB"));
                }
                let data: Value = serde_json::from_str(&text)?;
                if data["version"] != 1
                    || data["tool"] != env!("CARGO_PKG_VERSION")
                    || data["root"].as_str() != session.root.to_str()
                {
                    return Err(io::Error::other(
                        "session version or workspace mismatch; use a new session",
                    ));
                }
                let entries = data["files"]
                    .as_object()
                    .filter(|m| m.len() <= 4096)
                    .ok_or_else(|| io::Error::other("invalid session files"))?;
                for (path, value) in entries {
                    if !Path::new(path).is_absolute() {
                        return Err(io::Error::other("invalid session file path"));
                    }
                    let signature = value["signature"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("invalid session signature"))?
                        .to_owned();
                    session.entries.insert(
                        path.clone(),
                        Entry {
                            signature,
                            seen: Self::parse_ranges(&value["seen"])?,
                            pending: Self::parse_ranges(&value["pending"])?,
                        },
                    );
                }
            }
        }
        Ok(session)
    }
    fn parse_ranges(value: &Value) -> io::Result<Vec<Range<usize>>> {
        let values = value
            .as_array()
            .filter(|v| v.len() <= 200000)
            .ok_or_else(|| io::Error::other("invalid session ranges"))?;
        values
            .iter()
            .map(|v| {
                let a = v
                    .as_array()
                    .filter(|v| v.len() == 2)
                    .ok_or_else(|| io::Error::other("invalid session range"))?;
                let start = a[0]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| io::Error::other("invalid range start"))?;
                let end = a[1]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| io::Error::other("invalid range end"))?;
                if start >= end || end > 200000 {
                    return Err(io::Error::other("invalid session range bounds"));
                }
                Ok(start..end)
            })
            .collect()
    }
    pub fn signature(lines: &[String]) -> String {
        // Scoped to an exact tool version above; a content fingerprint, not an authenticity proof.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        lines.hash(&mut hash);
        format!("{:016x}", hash.finish())
    }
    pub fn refresh(&mut self, path: &str, signature: String) -> bool {
        let entry = self.entries.entry(path.to_owned()).or_default();
        let changed = !entry.signature.is_empty() && entry.signature != signature;
        if entry.signature != signature {
            entry.seen.clear();
            entry.pending.clear();
            entry.signature = signature;
        }
        changed
    }
    pub fn matches_source(&self, path: &str, signature: &str) -> bool {
        self.entries
            .get(path)
            .is_some_and(|entry| entry.signature == signature)
    }
    pub fn seen(&self, path: &str, n: usize) -> bool {
        self.entries
            .get(path)
            .is_some_and(|e| e.seen.iter().any(|r| r.contains(&n)))
    }
    pub fn pending(&self) -> Vec<(String, Vec<Range<usize>>)> {
        self.entries
            .iter()
            .filter(|(_, e)| !e.pending.is_empty())
            .map(|(p, e)| (p.clone(), e.pending.clone()))
            .collect()
    }
    pub fn record(&mut self, path: &str, shown: Vec<Range<usize>>, pending: Vec<Range<usize>>) {
        let entry = self.entries.get_mut(path).unwrap();
        entry.seen.extend(shown);
        entry.seen = Self::merge(std::mem::take(&mut entry.seen));
        entry.pending = Self::merge(pending);
    }
    pub fn previous_pending(&self, path: &str) -> Vec<Range<usize>> {
        self.entries
            .get(path)
            .map(|e| e.pending.clone())
            .unwrap_or_default()
    }
    pub fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
        ranges.sort_by_key(|r| (r.start, r.end));
        let mut merged: Vec<Range<usize>> = vec![];
        for range in ranges {
            if let Some(last) = merged.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        merged
    }
    pub fn save(&self) -> io::Result<()> {
        let files:serde_json::Map<String,Value>=self.entries.iter().map(|(path,e)| {
            let ranges=|rs:&Vec<Range<usize>>|rs.iter().map(|r|json!([r.start,r.end])).collect::<Vec<_>>();
            (path.clone(),json!({"signature":e.signature,"seen":ranges(&e.seen),"pending":ranges(&e.pending)}))
        }).collect();
        if files.len() > 4096 {
            return Err(io::Error::other("session exceeds file limit"));
        }
        let data = serde_json::to_vec(
            &json!({"version":1,"tool":env!("CARGO_PKG_VERSION"),"root":self.root,"files":files}),
        )?;
        if data.len() > 8 * 1024 * 1024 {
            return Err(io::Error::other("session exceeds 8 MiB"));
        }
        let temp = PathBuf::from(format!(
            "{}.tmp-{}",
            self.path.display(),
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(&data)?;
            file.sync_all()?;
            fs::rename(&temp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}
