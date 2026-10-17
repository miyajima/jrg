//! Optional offline candidate discovery. Only an explicit local Unix socket is used.
use crate::{cli::Args, search::Line};
use std::{collections::BTreeMap, fs, io, path::Path, sync::Arc};
pub struct Candidates {
    pub lines: Vec<Line>,
    pub ranks: BTreeMap<String, usize>,
    pub note: String,
}
pub struct Client;
impl Client {
    #[cfg(unix)]
    pub fn search(args: &Args, query: &str) -> io::Result<Option<Candidates>> {
        use std::{
            io::{BufRead, BufReader, Read, Write},
            os::unix::net::UnixStream,
            time::Duration,
        };
        let Some(socket) = std::env::var_os("JRG_EMBEDDING_SOCKET") else {
            return Ok(None);
        };
        let mut overrides = ignore::overrides::OverrideBuilder::new(std::env::current_dir()?);
        for glob in &args.globs {
            overrides.add(glob).map_err(io::Error::other)?;
        }
        let mut paths = args.paths.iter();
        let Some(first) = paths.next() else {
            return Ok(None);
        };
        let mut walker = ignore::WalkBuilder::new(first);
        for path in paths {
            walker.add(path);
        }
        walker
            .standard_filters(!args.no_ignore)
            .hidden(!args.hidden)
            .require_git(false)
            .overrides(overrides.build().map_err(io::Error::other)?);
        let mut allowed = BTreeMap::new();
        for entry in walker.build() {
            let entry = entry.map_err(io::Error::other)?;
            if !entry.file_type().is_some_and(|t| t.is_file())
                || !matches!(
                    entry.path().extension().and_then(|s| s.to_str()),
                    Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "md" | "json")
                )
            {
                continue;
            }
            let key = fs::canonicalize(entry.path())?
                .to_string_lossy()
                .into_owned();
            allowed.insert(key, entry.path().to_string_lossy().into_owned());
            if allowed.len() > 8192 {
                return Err(io::Error::other("embedding file limit 8192"));
            }
        }
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(120)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let request = serde_json::json!({"version":1,"query":query,"files":allowed.keys().collect::<Vec<_>>()});
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(io::Error::other("embedding request exceeds 2 MiB"));
        }
        stream.write_all(&bytes)?;
        let mut data = String::new();
        BufReader::new(stream)
            .take(1024 * 1024 + 1)
            .read_line(&mut data)?;
        if data.len() > 1024 * 1024 {
            return Err(io::Error::other("embedding response exceeds 1 MiB"));
        }
        Self::decode(&data, &allowed).map(Some)
    }
    #[cfg(not(unix))]
    pub fn search(_args: &Args, _query: &str) -> io::Result<Option<Candidates>> {
        if std::env::var_os("JRG_EMBEDDING_SOCKET").is_some() {
            return Err(io::Error::other("Unix sockets unavailable"));
        }
        Ok(None)
    }
    fn decode(data: &str, allowed: &BTreeMap<String, String>) -> io::Result<Candidates> {
        let value: serde_json::Value = serde_json::from_str(data)?;
        if value["version"] != 1
            || value["model"] != "google/embeddinggemma-2"
            || value["revision"] != "914f7f89142e33e77833254d9c9b90c3cef7303b"
        {
            return Err(io::Error::other("unverified EmbeddingGemma 2 response"));
        }
        let hits = value["hits"]
            .as_array()
            .filter(|h| h.len() <= 24)
            .ok_or_else(|| io::Error::other("invalid embedding hits"))?;
        let mut lines = Vec::new();
        let mut ranks = BTreeMap::new();
        for hit in hits {
            let Some(path) = hit["path"].as_str() else {
                continue;
            };
            let Some(display) = allowed.get(path) else {
                continue;
            };
            if Path::new(path).symlink_metadata()?.file_type().is_symlink() {
                continue;
            }
            let Some(number) = hit["line"].as_u64().filter(|n| *n > 0) else {
                continue;
            };
            let Some(text) = hit["text"].as_str() else {
                continue;
            };
            let source = fs::read_to_string(path)?;
            if source
                .strip_prefix('\u{feff}')
                .unwrap_or(&source)
                .lines()
                .nth(number as usize - 1)
                != Some(text)
            {
                return Err(io::Error::other("stale embedding source"));
            }
            let next = ranks.len();
            ranks.entry(display.clone()).or_insert(next);
            lines.push(Line {
                path: Arc::new(display.clone()),
                number,
                text: text.to_owned(),
                ranges: vec![],
                selected: true,
            });
        }
        let note = format!(
            "embedding=google/embeddinggemma-2 candidates={} files={} (local; heuristic)",
            lines.len(),
            ranks.len()
        );
        Ok(Candidates { lines, ranks, note })
    }
}
