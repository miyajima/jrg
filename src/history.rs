#[cfg(test)]
mod tests {
    use super::{Repo, Store, digest, new_id, set_private_directory_permissions};
    use serde_json::{Value, json};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Fixture {
        base: PathBuf,
        db: PathBuf,
        repo: Repo,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let base = std::env::temp_dir().join(format!("jrg-history-{label}-{}", new_id()));
            fs::create_dir_all(base.join("repo")).unwrap();
            set_private_directory_permissions(&base).unwrap();
            let root = base.join("repo");
            let repo = Repo {
                id: digest(&format!("repo-{label}")),
                worktree_id: digest(&format!("worktree-{label}")),
                root,
            };
            Self {
                db: base.join("history-v1.sqlite3"),
                base,
                repo,
            }
        }

        fn store(&self) -> Store {
            Store::open_at(&self.repo, &self.db).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn record(request_id: &str, query_key: &str, context_id: &str) -> Value {
        json!({
            "request_id": request_id,
            "query_key": query_key,
            "context_id": context_id,
            "results": [{
                "result_id": "result-1",
                "anchor_key": "anchor-1",
                "path": "src/lib.rs",
                "file_hash": "file-hash",
                "anchor_hash": "anchor-hash",
                "context_hash": "context-hash",
                "anchor_kind": "function",
                "start": 10,
                "end": 20
            }]
        })
    }

    fn add_emitted(store: &mut Store, request_id: &str, context_id: &str) {
        store
            .prepare(&record(request_id, "query-1", context_id))
            .unwrap();
        store.emitted(request_id).unwrap();
    }

    #[test]
    fn oversized_conditions_keep_query_identity_and_safe_scope() {
        let fixture = Fixture::new("bounded-conditions");
        let mut store = fixture.store();
        let mut request = record("large-conditions", "exact-original-query", "ctx");
        request["conditions"] =
            json!({"cwd":"", "requested_paths":["."], "patterns":["語".repeat(30_000)]});
        store.prepare(&request).unwrap();
        store.emitted("large-conditions").unwrap();
        let saved = store.show("large-conditions").unwrap()["request"].clone();
        assert!(serde_json::to_vec(&saved).unwrap().len() <= super::MAX_REQUEST_BYTES);
        assert_eq!(saved["query_key"], "exact-original-query");
        assert_eq!(saved["conditions"]["requested_paths"], json!(["."]));
        assert_eq!(
            saved["conditions"]["digest"],
            digest(&request["conditions"].to_string())
        );
        assert_eq!(saved["metadata_omitted"]["conditions"], true);
        assert_eq!(saved["results"], request["results"]);

        let mut request = record("large-paths", "other-query", "ctx");
        request["conditions"] = json!({"cwd":"", "requested_paths":["path/".repeat(15_000)]});
        store.prepare(&request).unwrap();
        let saved = store.show("large-paths").unwrap()["request"].clone();
        assert!(serde_json::to_vec(&saved).unwrap().len() <= super::MAX_REQUEST_BYTES);
        assert_eq!(saved["conditions"]["requested_paths"], json!([]));
        assert_eq!(saved["metadata_omitted"]["requested_paths"], true);
    }

    #[test]
    fn oversized_decisions_are_omitted_before_feedback_targets() {
        let fixture = Fixture::new("bounded-decisions");
        let mut store = fixture.store();
        let mut request = record("large-decisions", "query-1", "ctx");
        request["decisions"] = json!([{"stage":"review_scope", "path":"路".repeat(30_000)}]);
        store.prepare(&request).unwrap();
        store.emitted("large-decisions").unwrap();
        let saved = store.show("large-decisions").unwrap()["request"].clone();
        assert_eq!(saved["results"], request["results"]);
        assert_eq!(saved["decisions"], json!([]));
        assert_eq!(saved["metadata_omitted"]["decisions"], 1);
        assert_eq!(saved["metadata_omitted"]["results"], 0);
        assert!(store.feedback(&json!({"request_id":"large-decisions", "result_id":"result-1", "label":"helpful", "source":"human"})).unwrap()["accepted"] == true);
    }

    fn feedback(
        store: &mut Store,
        request_id: &str,
        label: &str,
        source: &str,
        context_id: &str,
        event_id: &str,
    ) -> Value {
        store
            .feedback(&json!({
                "request_id": request_id,
                "result_id": "result-1",
                "label": label,
                "source": source,
                "context_id": context_id,
                "event_id": event_id
            }))
            .unwrap()
    }

    #[test]
    fn records_and_clear_are_scoped_to_repo() {
        let first = Fixture::new("first");
        let second = Fixture {
            repo: Repo {
                id: digest("other-repo"),
                worktree_id: digest("other-tree"),
                root: first.repo.root.clone(),
            },
            db: first.db.clone(),
            base: first.base.clone(),
        };
        let mut first_store = first.store();
        first_store
            .prepare(&record("request-a", "query-1", "context-a"))
            .unwrap();
        first_store.emitted("request-a").unwrap();

        let second_store = second.store();
        assert!(second_store.show("request-a").is_err());
        assert_eq!(
            second_store.list(10).unwrap()["requests"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        drop(second_store);

        let mut second_store = second.store();
        second_store.clear().unwrap();
        assert_eq!(
            first_store.show("request-a").unwrap()["request"]["request_id"],
            "request-a"
        );
    }

    #[test]
    fn opens_through_a_safe_ancestor_path_alias() {
        let fixture = Fixture::new("ancestor-alias");
        let canonical_base = fs::canonicalize(&fixture.base).unwrap();
        if canonical_base == fixture.base {
            return;
        }
        assert!(
            !fs::symlink_metadata(&fixture.base)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        let store = fixture.store();
        assert_eq!(store.status().unwrap()["repo_id"], fixture.repo.id);
    }

    #[test]
    fn feedback_event_is_idempotent_and_conflicting_reuse_fails() {
        let fixture = Fixture::new("idempotence");
        let mut store = fixture.store();
        add_emitted(&mut store, "request-1", "request-context");

        let first = feedback(
            &mut store,
            "request-1",
            "helpful",
            "human",
            "reviewer",
            "event-1",
        );
        let retry = feedback(
            &mut store,
            "request-1",
            "helpful",
            "human",
            "reviewer",
            "event-1",
        );
        assert_eq!(first, retry);
        assert!(
            store
                .feedback(&json!({
                    "request_id": "request-1", "result_id": "result-1", "label": "not_helpful",
                    "source": "human", "context_id": "reviewer", "event_id": "event-1"
                }))
                .is_err()
        );
    }

    #[test]
    fn oversized_result_is_not_advertised_or_accepted_for_feedback() {
        let fixture = Fixture::new("bounded-result");
        let mut store = fixture.store();
        let mut request = record("large-result", "query-1", "ctx");
        let mut oversized = request["results"][0].clone();
        oversized["result_id"] = "omitted-result".into();
        oversized["ranges"] = json!(vec![[10, 11]; 20_000]);
        request["results"]
            .as_array_mut()
            .unwrap()
            .insert(0, oversized);
        let saved = store.prepare(&request).unwrap();
        assert_eq!(saved["metadata_omitted"]["results"], 1);
        assert_eq!(saved["results"].as_array().unwrap().len(), 1);
        assert_eq!(saved["results"][0]["result_id"], "result-1");
        store.emitted("large-result").unwrap();
        assert!(store.feedback(&json!({"request_id":"large-result", "result_id":"omitted-result", "label":"helpful", "source":"human"})).is_err());
        assert!(store.feedback(&json!({"request_id":"large-result", "result_id":"result-1", "label":"helpful", "source":"human"})).unwrap()["accepted"] == true);
    }

    #[test]
    fn feedback_requires_an_emitted_request_and_returned_result() {
        let fixture = Fixture::new("emitted-gate");
        let mut store = fixture.store();
        store
            .prepare(&record("request-1", "query-1", "context-1"))
            .unwrap();
        assert!(store.feedback(&json!({
            "request_id": "request-1", "result_id": "result-1", "label": "helpful", "source": "human"
        })).is_err());
        store.emitted("request-1").unwrap();
        assert!(store.feedback(&json!({
            "request_id": "request-1", "result_id": "invented-result", "label": "helpful", "source": "human"
        })).is_err());
    }

    #[test]
    fn agent_value_uses_latest_label_per_context_then_averages() {
        let fixture = Fixture::new("agent-average");
        let mut store = fixture.store();
        add_emitted(&mut store, "request-1", "default");
        feedback(
            &mut store,
            "request-1",
            "not_helpful",
            "agent",
            "a",
            "agent-a-old",
        );
        feedback(
            &mut store,
            "request-1",
            "helpful",
            "agent",
            "a",
            "agent-a-new",
        );
        feedback(
            &mut store,
            "request-1",
            "not_helpful",
            "agent",
            "b",
            "agent-b",
        );
        feedback(&mut store, "request-1", "helpful", "agent", "c", "agent-c");
        let ratings = store.ratings("query-1").unwrap();
        assert_eq!(ratings.len(), 1);
        assert!((ratings[0]["value"].as_f64().unwrap() - (1.0 / 12.0)).abs() < 1e-9);
    }

    #[test]
    fn unknown_retracts_its_human_context_and_remaining_human_rating_wins() {
        let fixture = Fixture::new("human-priority");
        let mut store = fixture.store();
        add_emitted(&mut store, "request-1", "default");
        feedback(
            &mut store,
            "request-1",
            "helpful",
            "agent",
            "agent",
            "agent",
        );
        feedback(
            &mut store,
            "request-1",
            "helpful",
            "human",
            "retracted",
            "human-old",
        );
        feedback(
            &mut store,
            "request-1",
            "unknown",
            "human",
            "retracted",
            "human-retract",
        );
        feedback(
            &mut store,
            "request-1",
            "not_helpful",
            "human",
            "reviewer",
            "human-live",
        );
        let ratings = store.ratings("query-1").unwrap();
        assert_eq!(ratings.len(), 1);
        assert_eq!(ratings[0]["value"].as_f64().unwrap(), -1.0);
    }

    #[test]
    fn adding_feedback_prunes_expired_feedback_without_a_new_search() {
        let fixture = Fixture::new("feedback-retention");
        let mut store = fixture.store();
        add_emitted(&mut store, "request-1", "default");
        feedback(
            &mut store,
            "request-1",
            "helpful",
            "human",
            "reviewer",
            "expired-event",
        );
        let expired_at = now_ms() - 91 * 24 * 60 * 60 * 1000;
        store
            .conn
            .execute(
                "UPDATE feedback SET created_at = ?1 WHERE event_id = 'expired-event'",
                rusqlite::params![expired_at],
            )
            .unwrap();

        // The first search after expiry must not reuse the stale rating, even
        // before a new write triggers physical retention cleanup.
        assert!(store.ratings("query-1").unwrap().is_empty());

        feedback(
            &mut store,
            "request-1",
            "not_helpful",
            "human",
            "reviewer",
            "current-event",
        );

        let expired_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM feedback WHERE event_id = 'expired-event'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(expired_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_direct_database_and_state_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let fixture = Fixture::new("symlinks");
        let target = fixture.base.join("target.sqlite3");
        fs::write(&target, b"").unwrap();
        let database_alias = fixture.base.join("database-alias.sqlite3");
        symlink(&target, &database_alias).unwrap();
        assert!(Store::open_at(&fixture.repo, &database_alias).is_err());

        let real_state = fixture.base.join("real-state");
        fs::create_dir_all(&real_state).unwrap();
        set_private_directory_permissions(&real_state).unwrap();
        let state_alias = fixture.base.join("state-alias");
        symlink(&real_state, &state_alias).unwrap();
        assert!(Store::open_at(&fixture.repo, &state_alias.join("history-v1.sqlite3")).is_err());
    }

    #[test]
    fn corrupt_and_future_schema_databases_fail_closed() {
        let corrupt = Fixture::new("corrupt-db");
        fs::write(&corrupt.db, b"not a SQLite database").unwrap();
        assert!(Store::open_at(&corrupt.repo, &corrupt.db).is_err());

        let future = Fixture::new("future-db");
        let connection = rusqlite::Connection::open(&future.db).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 2;")
            .unwrap();
        drop(connection);
        assert!(Store::open_at(&future.repo, &future.db).is_err());
    }

    #[test]
    fn emitted_feedback_survives_reopening_with_private_wal_files() {
        let fixture = Fixture::new("reopen");
        {
            let mut store = fixture.store();
            add_emitted(&mut store, "request-1", "ctx");
            feedback(
                &mut store,
                "request-1",
                "helpful",
                "human",
                "ctx",
                "event-1",
            );
        }
        let store = fixture.store();
        assert_eq!(
            store.show("request-1").unwrap()["request"]["status"],
            "emitted"
        );
        assert_eq!(store.ratings("query-1").unwrap()[0]["value"], 1.0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for suffix in ["", "-wal", "-shm"] {
                let path = super::sidecar_path(&fixture.db, suffix);
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }

    #[test]
    fn logical_capacity_counts_utf8_bytes_instead_of_characters() {
        let fixture = Fixture::new("utf8-capacity");
        let mut store = fixture.store();
        let mut value = record("request-1", "query-1", "ctx");
        value["conditions"] = json!({"pattern":"日本語の検索"});
        store.prepare(&value).unwrap();
        let payload: String = store
            .conn
            .query_row(
                "SELECT payload FROM requests WHERE request_id='request-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            store.status().unwrap()["requests"]["logical_bytes"],
            payload.len()
        );
    }

    #[test]
    fn preparation_prunes_oldest_requests_above_ten_thousand() {
        let fixture = Fixture::new("request-cap");
        let mut store = fixture.store();
        let baseline = now_ms() - 20_000;
        let repo_id = store.repo_id.clone();
        let worktree_id = store.worktree_id.clone();
        let tx = store.conn.transaction().unwrap();
        for index in 0..10_001 {
            let request_id = format!("retained-{index:05}");
            let created_at = baseline + index as i64;
            let payload = serde_json::to_string(&json!({
                "request_id": request_id,
                "query_key": "query-1",
                "context_id": "context-1",
                "created_at": created_at,
                "repo_id": repo_id,
                "worktree_id": worktree_id,
                "status": "emitted",
                "results": []
            }))
            .unwrap();
            tx.execute(
                "INSERT INTO requests(request_id, repo_id, worktree_id, status, created_at, query_key, context_id, payload)
                 VALUES (?1, ?2, ?3, 'emitted', ?4, 'query-1', 'context-1', ?5)",
                rusqlite::params![request_id, repo_id, worktree_id, created_at, payload],
            )
            .unwrap();
        }
        tx.commit().unwrap();

        store
            .prepare(&record("fresh-request", "query-1", "context-1"))
            .unwrap();

        let count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM requests WHERE repo_id = ?1",
                rusqlite::params![store.repo_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count <= 10_000);
        assert!(store.show("retained-00000").is_err());
        assert!(store.show("fresh-request").is_ok());
    }

    #[test]
    fn prepare_prunes_requests_older_than_retention_window() {
        let fixture = Fixture::new("retention");
        let mut store = fixture.store();
        let old = now_ms() - 91 * 24 * 60 * 60 * 1000;
        let mut request = record("old-request", "query-1", "context-1");
        request["created_at"] = json!(old);
        store.prepare(&request).unwrap();
        assert!(store.show("old-request").is_err());
    }
}

use clap::ValueEnum;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 1;
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_REQUESTS: i64 = 10_000;
const MAX_LOGICAL_BYTES: i64 = 64 * 1024 * 1024;
const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const FEEDBACK_LIMIT: usize = 2_048;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    #[default]
    Auto,
    Record,
    Off,
}

pub fn effective_mode(mode: Mode) -> Mode {
    if env::var("JRG_HISTORY")
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("off"))
    {
        Mode::Off
    } else {
        mode
    }
}

pub fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Clone, Debug)]
pub struct Repo {
    pub id: String,
    pub worktree_id: String,
    pub root: PathBuf,
}

impl Repo {
    // Standard repositories/worktrees need no child process. Unusual Git
    // configuration falls back to Git's own resolver below.
    fn standard(root: &Path) -> Option<Self> {
        let marker = root.join(".git");
        let git_dir = if marker.is_dir() {
            marker
        } else {
            if fs::metadata(&marker).ok()?.len() > 4096 {
                return None;
            }
            let text = fs::read_to_string(&marker).ok()?;
            root.join(text.trim().strip_prefix("gitdir: ")?)
                .canonicalize()
                .ok()?
        };
        if !git_dir.join("HEAD").is_file() || git_dir.join("config.worktree").exists() {
            return None;
        }
        let common_file = git_dir.join("commondir");
        let common = if common_file.exists() {
            if fs::metadata(&common_file).ok()?.len() > 4096 {
                return None;
            }
            git_dir
                .join(fs::read_to_string(common_file).ok()?.trim())
                .canonicalize()
                .ok()?
        } else {
            git_dir.canonicalize().ok()?
        };
        let config = common.join("config");
        if !common.join("objects").is_dir() || fs::metadata(&config).ok()?.len() > 1024 * 1024 {
            return None;
        }
        let config = fs::read_to_string(config).ok()?;
        for line in config.lines() {
            let line = line.trim().to_ascii_lowercase();
            if line.starts_with("[include") {
                return None;
            }
            if let Some((key, value)) = line.split_once('=')
                && (key.trim() == "worktree" || (key.trim() == "bare" && value.trim() != "false"))
            {
                return None;
            }
        }
        Some(Self {
            id: digest(&common.to_string_lossy()),
            worktree_id: digest(&root.to_string_lossy()),
            root: root.to_owned(),
        })
    }

    pub fn discover(root: &Path) -> io::Result<Repo> {
        let requested_root = fs::canonicalize(root)?;
        if !requested_root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "repository root must be a directory",
            ));
        }
        let special_env = [
            "GIT_CEILING_DIRECTORIES",
            "GIT_DISCOVERY_ACROSS_FILESYSTEM",
            "GIT_CONFIG_COUNT",
        ]
        .iter()
        .any(|key| env::var_os(key).is_some());
        if !special_env {
            let mut marker_found = false;
            for ancestor in requested_root.ancestors() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if fs::metadata(ancestor)?.dev() != fs::metadata(&requested_root)?.dev() {
                        break;
                    }
                }
                if ancestor.join(".git").exists() {
                    marker_found = true;
                    if let Some(repo) = Self::standard(ancestor) {
                        return Ok(repo);
                    }
                    break;
                }
            }
            if !marker_found {
                let id = digest(&requested_root.to_string_lossy());
                return Ok(Self {
                    id: id.clone(),
                    worktree_id: id,
                    root: requested_root,
                });
            }
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(&requested_root)
            .args([
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
                "--show-toplevel",
            ])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .output();

        let (repo_id, worktree_id, canonical_root) = match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8(output.stdout).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("git returned invalid UTF-8: {error}"),
                    )
                })?;
                let mut lines = stdout.lines();
                let common_dir = lines
                    .next()
                    .filter(|line| !line.is_empty())
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "git did not return a common directory",
                        )
                    })?;
                let worktree_root =
                    lines
                        .next()
                        .filter(|line| !line.is_empty())
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "git did not return a worktree root",
                            )
                        })?;
                let common_dir = fs::canonicalize(common_dir)?;
                let worktree_root = fs::canonicalize(worktree_root)?;
                (
                    digest(&common_dir.to_string_lossy()),
                    digest(&worktree_root.to_string_lossy()),
                    worktree_root,
                )
            }
            _ => {
                let root_text = requested_root.to_string_lossy();
                (digest(&root_text), digest(&root_text), requested_root)
            }
        };

        Ok(Repo {
            id: repo_id,
            worktree_id,
            root: canonical_root,
        })
    }
}

pub struct Store {
    conn: Connection,
    repo_id: String,
    worktree_id: String,
    path: PathBuf,
}

impl Store {
    pub fn open(repo: &Repo) -> io::Result<Store> {
        Self::open_at(repo, &state_database_path()?)
    }

    fn open_at(repo: &Repo, database_path: &Path) -> io::Result<Store> {
        let requested_database_path = if database_path.is_absolute() {
            database_path.to_path_buf()
        } else {
            env::current_dir()?.join(database_path)
        };
        let parent = requested_database_path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "history database path has no parent directory",
            )
        })?;
        ensure_private_directory(parent)?;
        reject_symlink_file(&requested_database_path)?;
        let file_name = requested_database_path.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "history database path has no file name",
            )
        })?;
        let canonical_parent = fs::canonicalize(parent)?;
        let database_path = canonical_parent.join(file_name);
        reject_symlink_file(&database_path)?;

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        reject_symlink_file(&sidecar_path(&database_path, "-wal"))?;
        reject_symlink_file(&sidecar_path(&database_path, "-shm"))?;
        let mut conn = Connection::open_with_flags(&database_path, flags).map_err(db_error)?;
        set_private_file_permissions(&database_path)?;
        // CLI calls are short-lived. Let WAL autocheckpoint do bounded work
        // rather than synchronously checkpointing twice for every search.
        conn.set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .map_err(db_error)?;
        conn.busy_timeout(Duration::from_millis(25))
            .map_err(db_error)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(db_error)?;
        initialize_schema(&mut conn)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA wal_autocheckpoint = 256;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(db_error)?;
        // Aggregate capacity from a small covering index, not every JSON body.
        conn.execute_batch("CREATE INDEX IF NOT EXISTS requests_metadata_size ON requests(length(CAST(payload AS BLOB)));
                            CREATE INDEX IF NOT EXISTS feedback_metadata_size ON feedback(length(CAST(payload AS BLOB)));
                            CREATE INDEX IF NOT EXISTS feedback_created ON feedback(created_at);")
            .map_err(db_error)?;
        reject_symlink_file(&database_path)?;
        set_private_file_permissions(&database_path)?;

        Ok(Store {
            conn,
            repo_id: repo.id.clone(),
            worktree_id: repo.worktree_id.clone(),
            path: database_path,
        })
    }

    pub fn prepare(&mut self, record: &Value) -> io::Result<Value> {
        let input = record
            .as_object()
            .ok_or_else(|| invalid_data("request must be a JSON object"))?;
        for reserved in ["repo_id", "worktree_id", "status"] {
            if input.contains_key(reserved) {
                return Err(invalid_data(format!(
                    "request field {reserved} is backend-owned"
                )));
            }
        }

        let request_id = required_string(input, "request_id")?;
        let query_key = required_string(input, "query_key")?;
        let context_id = required_string(input, "context_id")?;
        validate_identifier(request_id, "request_id")?;
        validate_identifier(query_key, "query_key")?;
        validate_identifier(context_id, "context_id")?;
        validate_results(
            input
                .get("results")
                .ok_or_else(|| invalid_data("request results are required"))?,
        )?;
        reject_source_content(record)?;

        let created_at = match input.get("created_at") {
            Some(value) => value.as_i64().ok_or_else(|| {
                invalid_data("created_at must be an integer timestamp in milliseconds")
            })?,
            None => now_ms()?,
        };
        let mut stored = record.clone();
        let object = stored.as_object_mut().expect("validated request object");
        object.insert("repo_id".to_owned(), Value::String(self.repo_id.clone()));
        object.insert(
            "worktree_id".to_owned(),
            Value::String(self.worktree_id.clone()),
        );
        object.insert("status".to_owned(), Value::String("prepared".to_owned()));
        object.insert("created_at".to_owned(), Value::Number(created_at.into()));

        let payload = Self::bounded_payload(&mut stored)?;

        let tx = self.conn.transaction().map_err(db_error)?;
        tx.execute(
            "INSERT INTO requests(request_id, repo_id, worktree_id, status, created_at, query_key, context_id, payload)
             VALUES (?1, ?2, ?3, 'prepared', ?4, ?5, ?6, ?7)",
            params![request_id, self.repo_id, self.worktree_id, created_at, query_key, context_id, payload],
        )
        .map_err(db_error)?;
        prune(&tx, now_ms()?)?;
        tx.commit().map_err(db_error)?;
        maintain_database(&self.conn)?;
        Ok(stored)
    }

    fn bounded_payload(stored: &mut Value) -> io::Result<String> {
        let payload = serde_json::to_string(stored).map_err(json_error)?;
        if payload.len() <= MAX_REQUEST_BYTES {
            return Ok(payload);
        }
        // Keep exact query identity and complete result receipts. Never truncate
        // an identifier, a path or an individual source range into a new value.
        let results = std::mem::take(stored["results"].as_array_mut().unwrap());
        let decisions = stored
            .get_mut("decisions")
            .and_then(Value::as_array_mut)
            .map_or(0, |items| std::mem::take(items).len());
        stored["metadata_omitted"] = json!({"results":results.len(), "decisions":decisions,
            "conditions":false, "requested_paths":false});
        if serde_json::to_vec(stored).map_err(json_error)?.len() > MAX_REQUEST_BYTES {
            let conditions = stored["conditions"].take();
            stored["conditions"] = json!({"cwd":conditions["cwd"],
                "requested_paths":conditions["requested_paths"],
                "digest":digest(&conditions.to_string()), "details_omitted":true});
            stored["metadata_omitted"]["conditions"] = true.into();
            if serde_json::to_vec(stored).map_err(json_error)?.len() > MAX_REQUEST_BYTES {
                // An incomplete path list must not grant a narrower MCP server
                // access to a request that originally searched outside its root.
                stored["conditions"]["requested_paths"] = json!([]);
                stored["metadata_omitted"]["requested_paths"] = true.into();
            }
        }
        let mut size = serde_json::to_vec(stored).map_err(json_error)?.len();
        let mut kept = Vec::new();
        let total = results.len();
        for result in results {
            let cost = serde_json::to_vec(&result).map_err(json_error)?.len()
                + usize::from(!kept.is_empty());
            if size.saturating_add(cost) <= MAX_REQUEST_BYTES {
                size += cost;
                kept.push(result);
            }
        }
        stored["metadata_omitted"]["results"] = (total - kept.len()).into();
        stored["results"] = kept.into();
        let payload = serde_json::to_string(stored).map_err(json_error)?;
        if payload.len() > MAX_REQUEST_BYTES {
            return Err(invalid_data("request metadata exceeds the 64 KiB limit"));
        }
        Ok(payload)
    }

    pub fn emitted(&mut self, request_id: &str) -> io::Result<()> {
        validate_identifier(request_id, "request_id")?;
        let tx = self.conn.transaction().map_err(db_error)?;
        let payload: Option<String> = tx
            .query_row(
                "SELECT payload FROM requests WHERE request_id = ?1 AND repo_id = ?2",
                params![request_id, self.repo_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        let payload = payload
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "history request not found"))?;
        let mut value: Value = serde_json::from_str(&payload).map_err(json_error)?;
        let status = value.get("status").and_then(Value::as_str).unwrap_or("");
        if status == "emitted" {
            tx.commit().map_err(db_error)?;
            return Ok(());
        }
        if status != "prepared" {
            return Err(invalid_data("stored request has an invalid status"));
        }
        value
            .as_object_mut()
            .ok_or_else(|| invalid_data("stored request is not a JSON object"))?
            .insert("status".to_owned(), Value::String("emitted".to_owned()));
        let payload = serde_json::to_string(&value).map_err(json_error)?;
        tx.execute(
            "UPDATE requests SET status = 'emitted', payload = ?1 WHERE request_id = ?2 AND repo_id = ?3",
            params![payload, request_id, self.repo_id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)
    }

    pub fn list(&self, limit: usize) -> io::Result<Value> {
        let limit = limit.min(1_000) as i64;
        let mut statement = self
            .conn
            .prepare(
                "SELECT request_id, query_key, status, created_at, context_id, worktree_id
                 FROM requests WHERE repo_id = ?1
                 ORDER BY created_at DESC, request_id DESC LIMIT ?2",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map(params![self.repo_id, limit], |row| {
                Ok(json!({
                    "request_id": row.get::<_, String>(0)?,
                    "query_key": row.get::<_, String>(1)?,
                    "status": row.get::<_, String>(2)?,
                    "created_at": row.get::<_, i64>(3)?,
                    "context_id": row.get::<_, String>(4)?,
                    "worktree_id": row.get::<_, String>(5)?
                }))
            })
            .map_err(db_error)?;
        let mut requests = Vec::new();
        for row in rows {
            requests.push(row.map_err(db_error)?);
        }
        Ok(json!({"schema_version": SCHEMA_VERSION, "requests": requests}))
    }

    pub fn show(&self, id: &str) -> io::Result<Value> {
        validate_identifier(id, "request_id")?;
        let payload: Option<String> = self
            .conn
            .query_row(
                "SELECT payload FROM requests WHERE request_id = ?1 AND repo_id = ?2",
                params![id, self.repo_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        let payload = payload
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "history request not found"))?;
        let request: Value = serde_json::from_str(&payload).map_err(json_error)?;
        Ok(json!({"schema_version": SCHEMA_VERSION, "request": request}))
    }

    pub fn feedback(&mut self, input: &Value) -> io::Result<Value> {
        let object = input
            .as_object()
            .ok_or_else(|| invalid_data("feedback must be a JSON object"))?;
        let request_id = required_string(object, "request_id")?;
        let result_id = required_string(object, "result_id")?;
        let label = required_string(object, "label")?;
        let source = required_string(object, "source")?;
        validate_identifier(request_id, "request_id")?;
        validate_identifier(result_id, "result_id")?;
        if !matches!(label, "helpful" | "not_helpful" | "unknown") {
            return Err(invalid_data(
                "label must be helpful, not_helpful, or unknown",
            ));
        }
        if !matches!(source, "human" | "agent") {
            return Err(invalid_data("source must be human or agent"));
        }

        let tx = self.conn.transaction().map_err(db_error)?;
        let stored_request: Option<(String, String, String, String)> = tx
            .query_row(
                "SELECT payload, query_key, context_id, worktree_id
                 FROM requests WHERE request_id = ?1 AND repo_id = ?2 AND status = 'emitted'",
                params![request_id, self.repo_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_error)?;
        let (request_payload, query_key, default_context, worktree_id) = stored_request
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "emitted history request not found")
            })?;
        let request_json: Value = serde_json::from_str(&request_payload).map_err(json_error)?;
        let result = find_result(&request_json, result_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "result was not returned by this request",
            )
        })?;
        let anchor_key = result
            .get("anchor_key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid_data("stored result has no anchor key"))?
            .to_owned();
        let context_id = match object.get("context_id") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| invalid_data("context_id must be a string"))?
                .to_owned(),
            None => default_context,
        };
        validate_identifier(&context_id, "context_id")?;
        let event_id = match object.get("event_id") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| invalid_data("event_id must be a string"))?
                .to_owned(),
            None => new_id(),
        };
        validate_identifier(&event_id, "event_id")?;

        let existing: Option<(String, String, String, String, String, i64, String)> = tx
            .query_row(
                "SELECT request_id, result_id, label, source, context_id, created_at, payload
                 FROM feedback WHERE event_id = ?1",
                params![event_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        if let Some((
            old_request,
            old_result,
            old_label,
            old_source,
            old_context,
            created_at,
            payload,
        )) = existing
        {
            if old_request == request_id
                && old_result == result_id
                && old_label == label
                && old_source == source
                && old_context == context_id
            {
                tx.commit().map_err(db_error)?;
                let mut response: Value = serde_json::from_str(&payload).map_err(json_error)?;
                response["accepted"] = Value::Bool(true);
                response["created_at"] = Value::Number(created_at.into());
                return Ok(response);
            }
            return Err(invalid_data(
                "event_id is already used for a different feedback payload",
            ));
        }

        let created_at = now_ms()?;
        let feedback_value = json!({
            "event_id": event_id,
            "request_id": request_id,
            "result_id": result_id,
            "query_key": query_key,
            "anchor_key": anchor_key,
            "label": label,
            "source": source,
            "context_id": context_id,
            "repo_id": self.repo_id,
            "worktree_id": worktree_id,
            "created_at": created_at
        });
        let payload = serde_json::to_string(&feedback_value).map_err(json_error)?;
        if payload.len() > 4 * 1024 {
            return Err(invalid_data("feedback metadata exceeds the 4 KiB limit"));
        }
        tx.execute(
            "INSERT INTO feedback(event_id, request_id, repo_id, query_key, result_id, anchor_key,
                                  label, source, context_id, created_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                event_id,
                request_id,
                self.repo_id,
                query_key,
                result_id,
                anchor_key,
                label,
                source,
                context_id,
                created_at,
                payload
            ],
        )
        .map_err(db_error)?;
        prune(&tx, now_ms()?)?;
        tx.commit().map_err(db_error)?;
        maintain_database(&self.conn)?;

        let mut response = feedback_value;
        response["accepted"] = Value::Bool(true);
        Ok(response)
    }

    pub fn ratings(&self, query_key: &str) -> io::Result<Vec<Value>> {
        validate_identifier(query_key, "query_key")?;
        let mut statement = self
            .conn
            .prepare(
                "SELECT f.seq, f.anchor_key, f.result_id, f.label, f.source, f.context_id, r.request_id
                 FROM feedback AS f
                 JOIN requests AS r ON r.request_id = f.request_id AND r.repo_id = f.repo_id
                 WHERE f.repo_id = ?1 AND f.query_key = ?2 AND r.status = 'emitted'
                   AND f.created_at >= ?4 AND r.created_at >= ?4
                 ORDER BY f.seq DESC LIMIT ?3",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map(
                params![
                    self.repo_id,
                    query_key,
                    FEEDBACK_LIMIT as i64,
                    now_ms()?.saturating_sub(RETENTION_MS)
                ],
                |row| {
                    Ok(RatingEvent {
                        seq: row.get(0)?,
                        anchor_key: row.get(1)?,
                        result_id: row.get(2)?,
                        label: row.get(3)?,
                        source: row.get(4)?,
                        context_id: row.get(5)?,
                        request_id: row.get(6)?,
                    })
                },
            )
            .map_err(db_error)?;
        let mut grouped: HashMap<String, Vec<RatingEvent>> = HashMap::new();
        for row in rows {
            let event = row.map_err(db_error)?;
            grouped
                .entry(event.anchor_key.clone())
                .or_default()
                .push(event);
        }

        let mut output = Vec::new();
        let mut anchors: Vec<_> = grouped.into_iter().collect();
        anchors.sort_by(|a, b| a.0.cmp(&b.0));
        for (anchor_key, events) in anchors {
            let mut latest: HashMap<(&'static str, String), RatingEvent> = HashMap::new();
            for event in events {
                let source_key = if event.source == "human" {
                    "human"
                } else {
                    "agent"
                };
                let key = (source_key, event.context_id.clone());
                let replace = latest
                    .get(&key)
                    .is_none_or(|current| event.seq > current.seq);
                if replace {
                    latest.insert(key, event);
                }
            }

            let humans: Vec<_> = latest
                .values()
                .filter(|event| event.source == "human" && event.label != "unknown")
                .collect();
            let (value, representative) =
                if let Some(event) = humans.into_iter().max_by_key(|event| event.seq) {
                    (label_value(&event.label), event)
                } else {
                    let agents: Vec<_> = latest
                        .values()
                        .filter(|event| event.source == "agent" && event.label != "unknown")
                        .collect();
                    if agents.is_empty() {
                        continue;
                    }
                    let mean = agents
                        .iter()
                        .map(|event| label_value(&event.label))
                        .sum::<f64>()
                        / agents.len() as f64;
                    let representative = agents
                        .into_iter()
                        .max_by_key(|event| event.seq)
                        .expect("non-empty agent set");
                    (mean * 0.25, representative)
                };
            if value.abs() < f64::EPSILON {
                continue;
            }
            let request_payload: String = self
                .conn
                .query_row(
                    "SELECT payload FROM requests WHERE request_id=?1 AND repo_id=?2",
                    params![representative.request_id, self.repo_id],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            let request: Value = serde_json::from_str(&request_payload).map_err(json_error)?;
            let Some(result) = find_result(&request, &representative.result_id) else {
                continue;
            };
            let mut trusted_result = result.clone();
            let object = trusted_result
                .as_object_mut()
                .ok_or_else(|| invalid_data("stored result is not a JSON object"))?;
            if object.get("anchor_key").and_then(Value::as_str) != Some(anchor_key.as_str()) {
                continue;
            }
            object.insert("value".to_owned(), json!(value));
            output.push(trusted_result);
        }
        Ok(output)
    }

    pub fn export(&self) -> io::Result<String> {
        let mut output = String::new();
        let mut request_statement = self
            .conn
            .prepare(
                "SELECT payload FROM requests WHERE repo_id = ?1 ORDER BY created_at ASC, request_id ASC",
            )
            .map_err(db_error)?;
        let requests = request_statement
            .query_map(params![self.repo_id], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        for payload in requests {
            let request: Value =
                serde_json::from_str(&payload.map_err(db_error)?).map_err(json_error)?;
            output.push_str(
                &serde_json::to_string(&json!({
                    "schema_version": SCHEMA_VERSION,
                    "type": "request",
                    "request": request
                }))
                .map_err(json_error)?,
            );
            output.push('\n');
        }
        drop(request_statement);

        let mut feedback_statement = self
            .conn
            .prepare(
                "SELECT payload FROM feedback WHERE repo_id = ?1 ORDER BY created_at ASC, seq ASC",
            )
            .map_err(db_error)?;
        let events = feedback_statement
            .query_map(params![self.repo_id], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        for payload in events {
            let event: Value =
                serde_json::from_str(&payload.map_err(db_error)?).map_err(json_error)?;
            output.push_str(
                &serde_json::to_string(&json!({
                    "schema_version": SCHEMA_VERSION,
                    "type": "feedback",
                    "event": event
                }))
                .map_err(json_error)?,
            );
            output.push('\n');
        }
        Ok(output)
    }

    pub fn clear(&mut self) -> io::Result<Value> {
        let tx = self.conn.transaction().map_err(db_error)?;
        let requests: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM requests WHERE repo_id = ?1",
                params![self.repo_id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        let feedback: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM feedback WHERE repo_id = ?1",
                params![self.repo_id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        tx.execute(
            "DELETE FROM requests WHERE repo_id = ?1",
            params![self.repo_id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        maintain_database(&self.conn)?;
        Ok(json!({
            "schema_version": SCHEMA_VERSION,
            "cleared": true,
            "requests_removed": requests,
            "feedback_removed": feedback
        }))
    }

    pub fn status(&self) -> io::Result<Value> {
        let requests: (i64, i64, i64, i64) = self
            .conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN status = 'prepared' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN status = 'emitted' THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(length(CAST(payload AS BLOB))), 0)
                 FROM requests WHERE repo_id = ?1",
                params![self.repo_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(db_error)?;
        let feedback: (i64, i64) = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM feedback WHERE repo_id = ?1",
                params![self.repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;
        let page_count: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .map_err(db_error)?;
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .map_err(db_error)?;
        let free_pages: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .map_err(db_error)?;

        Ok(json!({
            "schema_version": SCHEMA_VERSION,
            "repo_id": self.repo_id,
            "worktree_id": self.worktree_id,
            "requests": {
                "total": requests.0,
                "prepared": requests.1,
                "emitted": requests.2,
                "logical_bytes": requests.3
            },
            "feedback": {
                "total": feedback.0,
                "logical_bytes": feedback.1
            },
            "storage": {
                "database_bytes": file_size(&self.path),
                "wal_bytes": file_size(&sidecar_path(&self.path, "-wal")),
                "shm_bytes": file_size(&sidecar_path(&self.path, "-shm")),
                "page_count": page_count,
                "page_size": page_size,
                "free_pages": free_pages
            }
        }))
    }
}

struct RatingEvent {
    seq: i64,
    anchor_key: String,
    result_id: String,
    label: String,
    source: String,
    context_id: String,
    request_id: String,
}

fn state_database_path() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("JRG_STATE_DIR") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "JRG_STATE_DIR must not be empty",
            ));
        }
        let absolute = if path.is_absolute() {
            path
        } else {
            env::current_dir()?.join(path)
        };
        return Ok(absolute.join("history-v1.sqlite3"));
    }
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path.join("jrg").join("history-v1.sqlite3"));
        }
    }
    let home = env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is unavailable and no absolute XDG_STATE_HOME is set",
        )
    })?;
    if !home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HOME must be an absolute path",
        ));
    }
    Ok(home
        .join(".local")
        .join("state")
        .join("jrg")
        .join("history-v1.sqlite3"))
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    if path.parent().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "filesystem root cannot be used as the history state directory",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "history state directory must not be a symlink",
                ));
            }
            if !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "history state path is not a directory",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o1000 != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "shared sticky directories cannot be used for history storage",
                    ));
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "history state directory must be a real directory",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    set_private_directory_permissions(path)
}

fn reject_symlink_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "history database must not be a symlink",
        )),
        Ok(metadata) if !metadata.is_file() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "history database path is not a regular file",
        )),
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() > 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "history database must not be hard-linked",
                    ));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if fs::metadata(path)?.nlink() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "history database must not be hard-linked",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn initialize_schema(conn: &mut Connection) -> io::Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_error)?;
    if version > SCHEMA_VERSION {
        return Err(invalid_data(format!(
            "history database schema {version} is newer than supported schema {SCHEMA_VERSION}"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version != 0 {
        return Err(invalid_data(format!(
            "unsupported history database schema version {version}"
        )));
    }

    let existing_objects: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type IN ('table', 'index', 'view', 'trigger') AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if existing_objects != 0 {
        return Err(invalid_data("unversioned history database is not empty"));
    }
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")
        .map_err(db_error)?;
    conn.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE requests (
             request_id TEXT PRIMARY KEY,
             repo_id TEXT NOT NULL,
             worktree_id TEXT NOT NULL,
             status TEXT NOT NULL CHECK (status IN ('prepared', 'emitted')),
             created_at INTEGER NOT NULL,
             query_key TEXT NOT NULL,
             context_id TEXT NOT NULL,
             payload TEXT NOT NULL
         );
         CREATE INDEX requests_repo_created ON requests(repo_id, created_at DESC, request_id);
         CREATE INDEX requests_created ON requests(created_at ASC, request_id);
         CREATE TABLE feedback (
             seq INTEGER PRIMARY KEY AUTOINCREMENT,
             event_id TEXT NOT NULL UNIQUE,
             request_id TEXT NOT NULL REFERENCES requests(request_id) ON DELETE CASCADE,
             repo_id TEXT NOT NULL,
             query_key TEXT NOT NULL,
             result_id TEXT NOT NULL,
             anchor_key TEXT NOT NULL,
             label TEXT NOT NULL CHECK (label IN ('helpful', 'not_helpful', 'unknown')),
             source TEXT NOT NULL CHECK (source IN ('human', 'agent')),
             context_id TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             payload TEXT NOT NULL
         );
         CREATE INDEX feedback_repo_query_seq ON feedback(repo_id, query_key, seq DESC);
         CREATE INDEX feedback_request ON feedback(request_id);
         PRAGMA user_version = 1;
         COMMIT;",
    )
    .map_err(db_error)
}

fn validate_results(results: &Value) -> io::Result<()> {
    let results = results
        .as_array()
        .ok_or_else(|| invalid_data("results must be an array"))?;
    let mut ids = std::collections::HashSet::new();
    for result in results {
        let object = result
            .as_object()
            .ok_or_else(|| invalid_data("each result must be a JSON object"))?;
        for field in [
            "result_id",
            "anchor_key",
            "path",
            "file_hash",
            "anchor_hash",
            "context_hash",
            "anchor_kind",
        ] {
            let value = required_string(object, field)?;
            validate_identifier(value, field)?;
        }
        let start = object
            .get("start")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid_data("result start must be an integer"))?;
        let end = object
            .get("end")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid_data("result end must be an integer"))?;
        if start < 0 || end < start {
            return Err(invalid_data("result range is invalid"));
        }
        let result_id = required_string(object, "result_id")?;
        if !ids.insert(result_id.to_owned()) {
            return Err(invalid_data("result_id must be unique within a request"));
        }
    }
    Ok(())
}

fn reject_source_content(value: &Value) -> io::Result<()> {
    const FORBIDDEN: &[&str] = &[
        "body",
        "source_body",
        "source_text",
        "snippet",
        "excerpt",
        "content",
        "contents",
        "line_text",
        "matched_text",
        "query",
        "raw",
        "text",
        "code",
        "preview",
    ];
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let normalized = key.to_ascii_lowercase();
                if FORBIDDEN.contains(&normalized.as_str()) {
                    return Err(invalid_data(format!(
                        "request field {key} is not allowed in history metadata"
                    )));
                }
                reject_source_content(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                reject_source_content(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn find_result<'a>(request: &'a Value, result_id: &str) -> Option<&'a Value> {
    request
        .get("results")?
        .as_array()?
        .iter()
        .find(|result| result.get("result_id").and_then(Value::as_str) == Some(result_id))
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str) -> io::Result<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_data(format!("{key} must be a string")))
}

fn validate_identifier(value: &str, field: &str) -> io::Result<()> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(invalid_data(format!(
            "{field} must be a non-empty identifier of at most 512 bytes"
        )));
    }
    Ok(())
}

fn label_value(label: &str) -> f64 {
    match label {
        "helpful" => 1.0,
        "not_helpful" => -1.0,
        _ => 0.0,
    }
}

fn prune(tx: &Transaction<'_>, now: i64) -> io::Result<()> {
    let cutoff = now.saturating_sub(RETENTION_MS);
    tx.execute(
        "DELETE FROM feedback WHERE created_at < ?1",
        params![cutoff],
    )
    .map_err(db_error)?;
    tx.execute(
        "DELETE FROM requests WHERE created_at < ?1",
        params![cutoff],
    )
    .map_err(db_error)?;

    loop {
        let (request_count, request_bytes): (i64, i64) = tx
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM requests INDEXED BY requests_metadata_size",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;
        let feedback_bytes: i64 = tx
            .query_row(
                "SELECT COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM feedback INDEXED BY feedback_metadata_size",
                [],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if request_count <= MAX_REQUESTS
            && request_bytes.saturating_add(feedback_bytes) <= MAX_LOGICAL_BYTES
        {
            break;
        }
        let removed = tx
            .execute(
                "DELETE FROM requests
                 WHERE request_id IN (
                     SELECT request_id FROM requests
                     ORDER BY created_at ASC, request_id ASC LIMIT 128
                 )",
                [],
            )
            .map_err(db_error)?;
        if removed == 0 {
            break;
        }
    }
    Ok(())
}

fn maintain_database(conn: &Connection) -> io::Result<()> {
    conn.execute_batch("PRAGMA incremental_vacuum(100);")
        .map_err(db_error)
}

fn now_ms() -> io::Result<i64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock is before Unix epoch: {error}")))?
        .as_millis();
    i64::try_from(millis)
        .map_err(|_| invalid_data("system timestamp does not fit in i64 milliseconds"))
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn db_error(error: rusqlite::Error) -> io::Error {
    io::Error::other(format!("history database error: {error}"))
}

fn json_error(error: serde_json::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid history JSON: {error}"),
    )
}

fn command_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_owned())
}

pub fn command(argv: &[String]) -> io::Result<u8> {
    let (root, args) = extract_root(argv)?;
    let Some(command) = args.first().map(String::as_str) else {
        return Err(command_error(
            "expected one of: list, show, feedback, export, clear, status",
        ));
    };

    let root = match root {
        Some(root) => root,
        None => env::current_dir()?,
    };
    let repo = Repo::discover(&root)?;
    let mut store = Store::open(&repo)?;

    let output = match command {
        "list" if args.len() == 1 => store.list(50)?,
        "show" if args.len() == 2 => store.show(&args[1])?,
        "feedback" => run_feedback_command(&mut store, &args[1..])?,
        "export" if args.len() == 1 => {
            use io::Write;
            io::stdout().write_all(store.export()?.as_bytes())?;
            return Ok(0);
        }
        "clear" if args.len() == 1 => store.clear()?,
        "status" if args.len() == 1 => store.status()?,
        "list" | "show" | "export" | "clear" | "status" => {
            return Err(command_error("invalid arguments for history command"));
        }
        _ => {
            return Err(command_error(
                "expected one of: list, show, feedback, export, clear, status",
            ));
        }
    };

    use io::Write;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output).map_err(json_error)?;
    stdout.write_all(b"\n")?;
    Ok(0)
}

fn extract_root(argv: &[String]) -> io::Result<(Option<PathBuf>, Vec<String>)> {
    let mut root = None;
    let mut args = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--root" {
            index += 1;
            let value = argv
                .get(index)
                .ok_or_else(|| command_error("--root requires a directory"))?;
            if root.replace(PathBuf::from(value)).is_some() {
                return Err(command_error("--root may be specified only once"));
            }
        } else if let Some(value) = arg.strip_prefix("--root=") {
            if value.is_empty() || root.replace(PathBuf::from(value)).is_some() {
                return Err(command_error(
                    "--root requires one directory and may be specified only once",
                ));
            }
        } else {
            args.push(arg.clone());
        }
        index += 1;
    }
    Ok((root, args))
}

fn run_feedback_command(store: &mut Store, args: &[String]) -> io::Result<Value> {
    if args.len() < 3 {
        return Err(command_error(
            "feedback requires REQUEST_ID RESULT_ID LABEL",
        ));
    }
    let mut input = json!({
        "request_id": args[0],
        "result_id": args[1],
        "label": args[2],
        "source": "human"
    });
    let mut seen_options = std::collections::HashSet::new();
    let mut index = 3;
    while index < args.len() {
        let option = args[index].as_str();
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| command_error("feedback option requires a value"))?;
        index += 1;
        let key = match option {
            "--source" => "source",
            "--context-id" => "context_id",
            "--event-id" => "event_id",
            _ => return Err(command_error("unsupported feedback option")),
        };
        if !seen_options.insert(key) {
            return Err(command_error("feedback option may be specified only once"));
        }
        input[key] = Value::String(value.clone());
    }
    store.feedback(&input)
}
