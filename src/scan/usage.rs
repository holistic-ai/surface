//! Token accounting for locally-installed AI tools.
//!
//! Answers "how much am I actually using, and on which models" from the
//! transcripts the tools already write: daily totals per tool and model,
//! deduplicated, incremental, and attributed to the repository the work happened
//! in.
//!
//! # Cost is applied later, not here
//!
//! This module counts tokens and records model ids. Pricing happens at read time
//! in [`crate::pricing`], against a table that can be refreshed — so history
//! re-prices itself when rates move, instead of being frozen at whatever the
//! rate was on the day it was counted.
//!
//! # Two ways this silently produces wrong numbers
//!
//! Both were measured on a real corpus during design, and both are handled
//! explicitly rather than assumed away.
//!
//! **Duplicates.** 18,268 Claude Code usage records reduce to 8,672 distinct
//! `(requestId, message.id)` pairs — count them all and every figure roughly
//! doubles. 89 of those keys span more than one file, so dedup state persists
//! in the ledger.
//!
//! **Cumulative versus delta.** Claude Code writes per-message deltas that
//! must be summed. Codex writes a *running total* per session; summing those
//! records gave 34,935,584 against a true 2,274,321, a 15x overcount. Each
//! source therefore declares its [`Accumulation`] and the runner respects it.
//!
//! # What is kept
//!
//! Date, tool, model and token counts, plus a per-repository rollup keyed by git
//! remote slug (`owner/name`) so spend can be attributed to a codebase.
//!
//! Never kept: prompts, responses, absolute file paths, git branch names, and
//! raw session identifiers. A working directory is resolved to a slug during
//! ingest and the path itself is never stored — see [`crate::repo`]. This
//! matters even with nowhere to send it, because the ledger persists to disk.
//!
//! Message content is not read at all. surface had an opt-in for it; `surface`
//! does not, which makes "no prose is ever read" unconditional rather than a
//! default someone can flip.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::UsageConfig;
use crate::ledger::{dedup_key, ledger_path, session_key, Ledger, ReadPlan, Tokens, UNATTRIBUTED};

// surface capped daily, per-repository and per-session rows here to bound
// the snapshot it put on the wire. `surface` puts nothing on a wire: the
// ledger is read in place by the views, which page through it. Reinstating a
// cap would only mean a table that quietly disagrees with its own totals.

/// Placeholder when a record does not name its model.
const UNKNOWN_MODEL: &str = "unknown";

/// Model names that are not real models and must not appear in a breakdown.
const NON_MODELS: &[&str] = &["<synthetic>", "synthetic", "<none>", ""];

/// How a source's records combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accumulation {
    /// Each record is a delta. Sum them, deduplicating by key.
    PerMessage,
    /// Each record is a running total for its session. Only the increase since
    /// the previous scan is new usage.
    CumulativePerSession,
}

/// One usage observation, normalised across tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub day: String,
    pub model: String,
    pub tokens: Tokens,
    /// Dedup key, for [`Accumulation::PerMessage`].
    pub key: Option<u64>,
    /// Session id, for [`Accumulation::CumulativePerSession`].
    pub session: Option<String>,
    /// Running total for cumulative sources.
    pub cumulative_total: u64,
    /// Working directory the record was produced in, when the format carries
    /// one. Resolved to a repository slug during ingest and never stored.
    pub cwd: Option<String>,
}

/// What the usage scan read, alongside the ledger it read into.
///
/// The ledger *is* the result — it already holds per-day, per-tool, per-model
/// and per-repo totals in typed form. surface flattened it into JSON for the
/// wire; nothing needs flattening here, so the views read it directly.
#[derive(Debug, Default)]
pub struct Usage {
    pub ledger: Ledger,
    /// Tools whose sources actually yielded new bytes this run.
    pub tools_read: Vec<&'static str>,
    pub sources_read: usize,
    pub bytes_read: u64,
    pub window_days: u64,
    /// Sources found but unreadable, and why. Surfaced rather than swallowed: a
    /// transcript we could not parse must not look like a quiet day.
    pub unreadable: Vec<Unreadable>,
    /// The ledger could not be written, so the next run re-reads everything.
    pub ledger_write_failed: bool,
    /// `scan = false`. Distinct from "scanned and found nothing".
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unreadable {
    pub tool: &'static str,
    pub reason: String,
}

/// Read every AI transcript on this machine into the usage ledger.
///
/// Incremental: a source unchanged since the last run is not even opened, and a
/// grown one is read only from its previous byte offset. A cold first run over a
/// large corpus is slow; every run after it is not.
pub fn scan(config: &UsageConfig, state_dir: &Path) -> Usage {
    if !config.scan {
        return Usage {
            disabled: true,
            window_days: config.window_days,
            ..Usage::default()
        };
    }

    let path = ledger_path(state_dir);
    let mut ledger = Ledger::load(&path);
    // `surface` never collects titles, so the policy is fixed off. Syncing it
    // still matters: a ledger inherited from a tool that did collect them has to
    // drop them rather than keep showing stale ones.
    ledger.sync_title_policy(false);
    // Read-time state, not persisted: the stored keys stay raw, so an edited
    // alias regroups the whole window on the next run.
    ledger.set_aliases(config.repo_aliases.clone());

    let mut usage = Usage {
        window_days: config.window_days,
        ..Usage::default()
    };
    let mut live: BTreeSet<String> = BTreeSet::new();
    let mut projects = Projects::new();

    for (tool, accumulation, files) in discover_sources() {
        let mut touched = false;
        for file in files {
            live.insert(file.to_string_lossy().to_string());
            match ingest_file(&mut ledger, tool, accumulation, &file, &mut projects) {
                Ok(0) => {}
                Ok(n) => {
                    usage.bytes_read += n;
                    usage.sources_read += 1;
                    touched = true;
                }
                Err(e) => usage.unreadable.push(Unreadable {
                    tool,
                    reason: e.to_string(),
                }),
            }
        }
        if touched && !usage.tools_read.contains(&tool) {
            usage.tools_read.push(tool);
        }
    }

    // OpenCode keeps its history in SQLite rather than JSONL, so it needs its
    // own reader rather than the byte-offset path above.
    #[cfg(feature = "sqlite")]
    for db in opencode_databases() {
        live.insert(db.to_string_lossy().to_string());
        match ingest_opencode(&mut ledger, &db, &mut projects) {
            Ok(0) => {}
            Ok(n) => {
                usage.bytes_read += n;
                usage.sources_read += 1;
                if !usage.tools_read.contains(&"opencode") {
                    usage.tools_read.push("opencode");
                }
            }
            Err(e) => usage.unreadable.push(Unreadable {
                tool: "opencode",
                reason: e.to_string(),
            }),
        }
    }

    // Without SQLite, OpenCode's token history is not merely absent — it is
    // unreadable, and saying so is the difference between "you did not use it"
    // and "this build cannot see it".
    #[cfg(not(feature = "sqlite"))]
    if opencode_store_present() {
        usage.unreadable.push(Unreadable {
            tool: "opencode",
            reason: format!(
                "{}: built without the sqlite feature",
                crate::reason::TOOL_UNAVAILABLE
            ),
        });
    }

    let cutoff = Utc::now() - chrono::Duration::days(config.window_days as i64);
    ledger.prune(&cutoff.format("%Y-%m-%d").to_string(), &live);

    // A failed write costs incrementality next run, not correctness now.
    usage.ledger_write_failed = ledger.save(&path).is_err();
    usage.ledger = ledger;
    usage
}

/// Every usage source on this machine, grouped by tool.
fn discover_sources() -> Vec<(&'static str, Accumulation, Vec<PathBuf>)> {
    let Some(home) = crate::paths::home() else {
        return Vec::new();
    };

    vec![
        (
            "claude_code",
            Accumulation::PerMessage,
            jsonl_under(&home.join(".claude/projects")),
        ),
        (
            "codex",
            Accumulation::CumulativePerSession,
            jsonl_under(&home.join(".codex/sessions")),
        ),
    ]
}

/// OpenCode message stores, if present.
///
/// Empty without the `sqlite` feature: the store is a SQLite database, so with
/// no SQLite there is nothing to look for. Reported as an unreadable source
/// rather than silently skipped — see [`scan`].
#[cfg(feature = "sqlite")]
pub(crate) fn opencode_databases() -> Vec<PathBuf> {
    opencode_candidates()
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

fn opencode_candidates() -> Vec<PathBuf> {
    let Some(home) = crate::paths::home() else {
        return Vec::new();
    };
    vec![
        home.join(".local/share/opencode/opencode.db"),
        home.join("Library/Application Support/opencode/opencode.db"),
    ]
}

/// Is there an OpenCode store we are choosing not to read?
#[cfg(not(feature = "sqlite"))]
fn opencode_store_present() -> bool {
    opencode_candidates().iter().any(|p| p.is_file())
}

/// Read OpenCode's `message` table.
///
/// Rows are per-message deltas whose parts are already disjoint — verified
/// against real data, where `input + output + reasoning + cache.read` equals
/// the row's own `total` — so they map straight onto [`Tokens`] and are summed
/// like Claude Code's.
///
/// SQLite has no append-only tail to seek into, so incrementality comes from
/// the dedup set instead: unchanged databases are skipped outright, and a
/// changed one is re-read with already-counted rows dropped by message id.
#[cfg(feature = "sqlite")]
fn ingest_opencode(ledger: &mut Ledger, path: &Path, projects: &mut Projects) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if ledger.plan(path, size, mtime) == ReadPlan::Skip {
        return Ok(0);
    }

    let connection = super::sites::open_readonly(path)
        .map_err(|e| anyhow::anyhow!("opening {}: {e}", path.display()))?;

    // OpenCode names its sessions in a table of its own, so titles come from
    // one query rather than a per-file head scan. Only `id` and `title` are
    // read; the same table also holds `directory`, which is a path.
    let mut titles: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if ledger.titles_enabled {
        if let Ok(mut statement) = connection
            .prepare("SELECT id, title FROM session WHERE title IS NOT NULL AND title != ''")
        {
            if let Ok(rows) = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                titles.extend(rows.flatten());
            }
        }
    }

    let mut statement = connection
        .prepare("SELECT id, session_id, data FROM message WHERE data LIKE '%\"tokens\"%'")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    for (id, session_id, data) in rows.flatten() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
            continue;
        };
        let Some(record) = parse_opencode(&id, &value) else {
            continue;
        };
        if !ledger.claim(&record.day, record.key) {
            continue;
        }
        ledger.add(&record.day, "opencode", &record.model, &record.tokens);
        let project = projects.slug(record.cwd.as_deref());
        ledger.add_project(&record.day, &project, &record.model, &record.tokens);

        let key = session_key(&session_id);
        ledger.add_session(&record.day, &key, &record.model, &record.tokens);
        ledger.observe_session(
            &key,
            "opencode",
            &project,
            titles.get(&session_id).map(String::as_str),
        );
    }

    ledger.commit_source(path, size, mtime, size);
    Ok(size)
}

/// One OpenCode message row.
///
/// A pure parser, so it is compiled and tested in both feature configurations —
/// only its caller needs SQLite to have a row to hand it.
#[cfg_attr(not(feature = "sqlite"), allow(dead_code))]
pub fn parse_opencode(id: &str, value: &serde_json::Value) -> Option<Record> {
    let tokens_json = value.get("tokens")?;
    let num = |key: &str| tokens_json.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    let cache = |key: &str| {
        tokens_json
            .get("cache")
            .and_then(|c| c.get(key))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    };

    let tokens = Tokens {
        input: num("input"),
        output: num("output"),
        reasoning: num("reasoning"),
        cache_read: cache("read"),
        cache_creation: cache("write"),
        messages: 1,
    };
    if tokens.total() == 0 {
        return None;
    }

    let model = value
        .get("modelID")
        .and_then(|m| m.as_str())
        .filter(|m| !m.is_empty())
        .unwrap_or(UNKNOWN_MODEL);
    if NON_MODELS.contains(&model) {
        return None;
    }

    // `time.created` is milliseconds since the Unix epoch.
    let created_ms = value
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_i64())?;
    let day = DateTime::from_timestamp_millis(created_ms)?
        .format("%Y-%m-%d")
        .to_string();

    Some(Record {
        day,
        model: model.to_string(),
        tokens,
        key: dedup_key(Some(id), None),
        session: value
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        cumulative_total: 0,
        cwd: value
            .get("path")
            .and_then(|p| p.get("cwd"))
            .and_then(|c| c.as_str())
            .map(str::to_string),
    })
}

/// Every `.jsonl` beneath `root`, recursively.
pub(crate) fn jsonl_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "jsonl") => {
                    found.push(path)
                }
                _ => {}
            }
        }
    }

    found.sort();
    found
}

/// Read the unread tail of one file into the ledger. Returns bytes consumed.
fn ingest_file(
    ledger: &mut Ledger,
    tool: &'static str,
    accumulation: Accumulation,
    path: &Path,
    projects: &mut Projects,
) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let start = match ledger.plan(path, size, mtime) {
        ReadPlan::Skip => return Ok(0),
        ReadPlan::ReadFrom(offset) => offset,
    };

    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start))?;

    // Codex names its model once, in the session header, and never repeats it
    // on the usage records themselves. Incremental reads start past that
    // header, so the header is re-read separately — bounded, and only for the
    // handful of session files that exist.
    let header = session_header(path);

    let mut consumed = start;
    let mut line = Vec::new();

    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        // A transcript being appended to right now ends mid-line. Stop before
        // it: parsing a partial record would fail *and* lose it, because the
        // offset would have moved past.
        if !line.ends_with(b"\n") {
            break;
        }
        consumed += n as u64;

        if let Some(mut record) = parse_line(tool, &line) {
            if record.model == UNKNOWN_MODEL {
                if let Some(model) = &header.model {
                    record.model = model.clone();
                }
            }
            let cwd = record.cwd.as_deref().or(header.cwd.as_deref());
            let project = projects.slug(cwd);
            // Codex names its session only in the header; Claude repeats it on
            // every record.
            let session = record
                .session
                .as_deref()
                .or(header.session.as_deref())
                .map(session_key)
                .unwrap_or_else(|| UNATTRIBUTED.to_string());
            apply(ledger, tool, accumulation, &record, &project, &session);
            ledger.observe_session(&session, tool, &project, header.title.as_deref());
        }
    }

    ledger.commit_source(path, size, mtime, consumed);
    Ok(consumed.saturating_sub(start))
}

/// Maps working directories to repository slugs, once each.
///
/// The same directory recurs across thousands of records, and resolving it
/// touches the filesystem, so every answer is cached. Paths live only inside
/// this struct: callers receive a slug and nothing else.
pub struct Projects {
    home: Option<PathBuf>,
    cache: std::collections::HashMap<String, String>,
}

impl Projects {
    pub fn new() -> Self {
        Self {
            home: crate::paths::home(),
            cache: std::collections::HashMap::new(),
        }
    }

    /// The repository slug for a working directory.
    ///
    /// Anything that does not resolve to a repository — a deleted checkout, a
    /// session run at home — becomes [`UNATTRIBUTED`] rather than vanishing,
    /// so per-project totals still add up to the daily ones.
    pub fn slug(&mut self, cwd: Option<&str>) -> String {
        let Some(cwd) = cwd.filter(|c| !c.is_empty()) else {
            return UNATTRIBUTED.to_string();
        };
        if let Some(hit) = self.cache.get(cwd) {
            return hit.clone();
        }
        let resolved = self
            .home
            .as_deref()
            .and_then(|home| crate::repo::resolve(Path::new(cwd), home))
            .and_then(|scope| match scope {
                crate::repo::Scope::Project(identity) => Some(identity.slug),
                crate::repo::Scope::Home => None,
            })
            .unwrap_or_else(|| UNATTRIBUTED.to_string());
        self.cache.insert(cwd.to_string(), resolved.clone());
        resolved
    }
}

impl Default for Projects {
    fn default() -> Self {
        Self::new()
    }
}

/// What a session's opening records declare, read once.
///
/// Three facts live in the head of a transcript rather than on its usage
/// records: Codex names its model and working directory only in `session_meta`,
/// and Claude Code writes an `ai-title` a few records in. Incremental reads
/// start past all of them, so the head is read separately each scan.
///
/// This used to be two functions opening the same file twice. Folding the
/// title in makes it three facts for one read rather than three reads.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Header {
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub session: Option<String>,
}

fn session_header(path: &Path) -> Header {
    use std::io::Read;

    const HEAD_BYTES: usize = 64 * 1024;

    let mut head = vec![0u8; HEAD_BYTES];
    let Ok(mut file) = std::fs::File::open(path) else {
        return Header::default();
    };
    let Ok(read) = file.read(&mut head) else {
        return Header::default();
    };
    head.truncate(read);

    let mut found = Header::default();
    for line in head.split(|b| *b == b'\n') {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };

        // Named keys only. A Codex header also carries `base_instructions` —
        // several kilobytes of system prompt — and a `git` object holding the
        // branch. A generic key scan would sweep both into the payload.
        let payload = value.get("payload");
        if found.model.is_none() {
            // `payload.model`, but not `model_provider` or
            // `model_context_window`.
            found.model = payload
                .and_then(|p| p.get("model"))
                .and_then(|m| m.as_str())
                .filter(|m| !m.is_empty())
                .map(str::to_string);
        }
        if found.cwd.is_none() {
            found.cwd = payload
                .and_then(|p| p.get("cwd"))
                .or_else(|| value.get("cwd"))
                .and_then(|c| c.as_str())
                .filter(|c| !c.is_empty())
                .map(str::to_string);
        }
        if found.title.is_none() && value.get("type").and_then(|t| t.as_str()) == Some("ai-title") {
            found.title = value
                .get("aiTitle")
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty())
                .map(str::to_string);
        }
        if found.session.is_none() {
            found.session = value
                .get("sessionId")
                .or_else(|| payload.and_then(|p| p.get("session_id")))
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(str::to_string);
        }

        if found.model.is_some()
            && found.cwd.is_some()
            && found.title.is_some()
            && found.session.is_some()
        {
            break;
        }
    }
    found
}

/// Fold one record into the ledger under its accumulation rule.
///
/// `project` is applied with exactly the same tokens as the tool totals, so
/// the two views of the ledger always reconcile.
fn apply(
    ledger: &mut Ledger,
    tool: &str,
    accumulation: Accumulation,
    record: &Record,
    project: &str,
    session: &str,
) {
    match accumulation {
        Accumulation::PerMessage => {
            if !ledger.claim(&record.day, record.key) {
                return;
            }
            ledger.add(&record.day, tool, &record.model, &record.tokens);
            ledger.add_project(&record.day, project, &record.model, &record.tokens);
            ledger.add_session(&record.day, session, &record.model, &record.tokens);
        }
        Accumulation::CumulativePerSession => {
            // The raw id, distinct from the hashed `session` key above: the
            // running-total bookkeeping is local to this device and never
            // leaves it, so it keys on the id the tool actually wrote.
            let raw = record.session.clone().unwrap_or_default();
            let delta = ledger.cumulative_delta(&raw, record.cumulative_total);
            if delta == 0 {
                return;
            }
            // The running total is a single number, so the split across token
            // kinds comes from the newest record's own breakdown.
            let scaled = scale_to_delta(&record.tokens, delta);
            ledger.add(&record.day, tool, &record.model, &scaled);
            ledger.add_project(&record.day, project, &record.model, &scaled);
            ledger.add_session(&record.day, session, &record.model, &scaled);
        }
    }
}

/// Apportion a cumulative delta across token kinds using the latest snapshot's
/// proportions, so a partial session still reports a plausible breakdown.
///
/// The parts are made to sum to exactly `delta`: integer division loses a few
/// tokens per record, and a token count that quietly drifts low is worse than
/// a breakdown that is a rounding off in one bucket.
fn scale_to_delta(snapshot: &Tokens, delta: u64) -> Tokens {
    let total = snapshot.total();
    if total == 0 {
        return Tokens {
            input: delta,
            messages: 1,
            ..Default::default()
        };
    }

    let share = |v: u64| ((v as u128 * delta as u128) / total as u128) as u64;
    let mut scaled = Tokens {
        input: share(snapshot.input),
        output: share(snapshot.output),
        cache_read: share(snapshot.cache_read),
        cache_creation: share(snapshot.cache_creation),
        reasoning: share(snapshot.reasoning),
        messages: 1,
    };

    // Give the truncation remainder to the largest bucket, where it distorts
    // the proportions least.
    let remainder = delta.saturating_sub(scaled.total());
    if remainder > 0 {
        let largest = [
            scaled.cache_read,
            scaled.input,
            scaled.output,
            scaled.cache_creation,
            scaled.reasoning,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);

        if largest == scaled.cache_read {
            scaled.cache_read += remainder;
        } else if largest == scaled.input {
            scaled.input += remainder;
        } else if largest == scaled.output {
            scaled.output += remainder;
        } else if largest == scaled.cache_creation {
            scaled.cache_creation += remainder;
        } else {
            scaled.reasoning += remainder;
        }
    }

    scaled
}

/// Parse one JSONL line into a normalised record, if it carries usage.
pub fn parse_line(tool: &str, line: &[u8]) -> Option<Record> {
    // Cheap prefilter: most transcript lines are not usage records, and
    // skipping the JSON parse for them is what keeps a full pass fast.
    if !line.windows(7).any(|w| w == b"\"usage\"") && !contains_codex_usage(line) {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    match tool {
        "codex" => parse_codex(&value),
        _ => parse_claude_code(&value),
    }
}

fn contains_codex_usage(line: &[u8]) -> bool {
    line.windows(18).any(|w| w == b"total_token_usage\"")
}

/// Claude Code: `message.usage` holds per-message deltas.
pub fn parse_claude_code(value: &serde_json::Value) -> Option<Record> {
    let message = value.get("message")?;
    let usage = message.get("usage")?.as_object()?;

    let model = message.get("model").and_then(|m| m.as_str()).unwrap_or("");
    if NON_MODELS.contains(&model) {
        return None;
    }

    let num = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    let tokens = Tokens {
        input: num("input_tokens"),
        output: num("output_tokens"),
        cache_read: num("cache_read_input_tokens"),
        cache_creation: num("cache_creation_input_tokens"),
        reasoning: 0,
        messages: 1,
    };
    if tokens.total() == 0 {
        return None;
    }

    Some(Record {
        day: day_of(value.get("timestamp").and_then(|t| t.as_str()))?,
        model: model.to_string(),
        tokens,
        key: dedup_key(
            value.get("requestId").and_then(|v| v.as_str()),
            message.get("id").and_then(|v| v.as_str()),
        ),
        session: None,
        cumulative_total: 0,
        cwd: value
            .get("cwd")
            .and_then(|c| c.as_str())
            .map(str::to_string),
    })
}

/// Codex: `total_token_usage` is a running total for the session.
pub fn parse_codex(value: &serde_json::Value) -> Option<Record> {
    let usage = find_key(value, "total_token_usage", 0)?.as_object()?;

    let num = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);

    // Codex nests its buckets: `cached_input_tokens` is part of
    // `input_tokens`, and `reasoning_output_tokens` is part of
    // `output_tokens` — verified against real data, where
    // input + output == total_tokens exactly while summing all five
    // over-counts by 46%.
    //
    // Anthropic's fields are already disjoint, so we subtract the nested
    // parts here and every tool then means the same thing by `input`.
    let input_all = num("input_tokens");
    let cached = num("cached_input_tokens").min(input_all);
    let output_all = num("output_tokens");
    let reasoning = num("reasoning_output_tokens").min(output_all);

    let tokens = Tokens {
        input: input_all - cached,
        output: output_all - reasoning,
        cache_read: cached,
        cache_creation: num("cache_write_input_tokens"),
        reasoning,
        messages: 1,
    };

    let cumulative_total = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| tokens.total());
    if cumulative_total == 0 {
        return None;
    }

    let model = find_key(value, "model", 0)
        .and_then(|m| m.as_str())
        .filter(|m| !m.is_empty())
        .unwrap_or(UNKNOWN_MODEL)
        .to_string();

    Some(Record {
        day: day_of(
            find_key(value, "timestamp", 0)
                .and_then(|t| t.as_str())
                .or_else(|| value.get("ts").and_then(|t| t.as_str())),
        )?,
        model,
        tokens,
        key: None,
        session: find_key(value, "session_id", 0)
            .or_else(|| find_key(value, "id", 0))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        cumulative_total,
        // Codex names the directory once in the session header, so this is
        // filled from there during ingest rather than parsed per record.
        cwd: None,
    })
}

/// First occurrence of `key` in a shallow-ish tree. Records nest their payload
/// a level or two down and the depth varies by version.
fn find_key<'a>(
    value: &'a serde_json::Value,
    key: &str,
    depth: usize,
) -> Option<&'a serde_json::Value> {
    if depth > 4 {
        return None;
    }
    let object = value.as_object()?;
    if let Some(found) = object.get(key) {
        return Some(found);
    }
    object
        .values()
        .find_map(|child| find_key(child, key, depth + 1))
}

/// `YYYY-MM-DD` in UTC from an RFC3339 timestamp.
fn day_of(timestamp: Option<&str>) -> Option<String> {
    let timestamp = timestamp?;
    let parsed = DateTime::parse_from_rfc3339(timestamp).ok()?;
    Some(parsed.with_timezone(&Utc).format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("surface-usage-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn claude_line(request: &str, msg: &str, model: &str, input: u64, output: u64) -> String {
        json!({
            "requestId": request,
            "timestamp": "2026-07-26T12:00:00.000Z",
            "type": "assistant",
            "sessionId": "sess-1",
            "cwd": "/Users/someone/repos/secret-client-project",
            "gitBranch": "feature/acme-migration",
            "message": {
                "id": msg,
                "model": model,
                "role": "assistant",
                "usage": {
                    "input_tokens": input,
                    "output_tokens": output,
                    "cache_read_input_tokens": 1000,
                    "cache_creation_input_tokens": 50
                }
            }
        })
        .to_string()
            + "\n"
    }

    fn codex_line(session: &str, total: u64) -> String {
        json!({
            "timestamp": "2026-07-26T12:00:00.000Z",
            "session_id": session,
            "payload": {
                "model": "gpt-5",
                "info": {
                    "total_token_usage": {
                        "input_tokens": total / 2,
                        "cached_input_tokens": total / 4,
                        "cache_write_input_tokens": 0,
                        "output_tokens": total / 4,
                        "reasoning_output_tokens": 0,
                        "total_tokens": total
                    }
                }
            }
        })
        .to_string()
            + "\n"
    }

    // -------------------------------------------------------------- parsing

    #[test]
    fn parses_a_claude_code_usage_record() {
        let line = claude_line("req-1", "msg-1", "claude-opus-5", 100, 20);
        let r = parse_line("claude_code", line.as_bytes()).unwrap();

        assert_eq!(r.day, "2026-07-26");
        assert_eq!(r.model, "claude-opus-5");
        assert_eq!(r.tokens.input, 100);
        assert_eq!(r.tokens.output, 20);
        assert_eq!(r.tokens.cache_read, 1000);
        assert!(r.key.is_some());
    }

    #[test]
    fn skips_lines_with_no_usage_block() {
        let line = json!({"type": "user", "message": {"role": "user"}}).to_string();
        assert!(parse_line("claude_code", line.as_bytes()).is_none());
    }

    #[test]
    fn excludes_the_synthetic_model() {
        let line = claude_line("req-1", "msg-1", "<synthetic>", 5, 5);
        assert!(parse_line("claude_code", line.as_bytes()).is_none());
    }

    #[test]
    fn skips_records_whose_tokens_are_all_zero() {
        let line = json!({
            "requestId": "r", "timestamp": "2026-07-26T12:00:00.000Z",
            "message": {"id": "m", "model": "claude-opus-5",
                        "usage": {"input_tokens": 0, "output_tokens": 0}}
        })
        .to_string();
        assert!(parse_line("claude_code", line.as_bytes()).is_none());
    }

    #[test]
    fn a_record_without_a_timestamp_is_skipped_rather_than_misdated() {
        let line = json!({
            "requestId": "r",
            "message": {"id": "m", "model": "claude-opus-5",
                        "usage": {"input_tokens": 10, "output_tokens": 1}}
        })
        .to_string();
        assert!(parse_line("claude_code", line.as_bytes()).is_none());
    }

    #[test]
    fn parses_a_nested_codex_running_total() {
        let r = parse_line("codex", codex_line("s1", 1000).as_bytes()).unwrap();
        assert_eq!(r.cumulative_total, 1000);
        assert_eq!(r.model, "gpt-5");
        assert_eq!(r.session.as_deref(), Some("s1"));
    }

    #[test]
    fn codex_nested_buckets_are_normalised_to_disjoint_ones() {
        // Real figures: input 14880 (of which 6912 cached), output 169 (of
        // which 39 reasoning), total 15049. Summing all five fields gives
        // 22000 — 46% too high — so the nested parts must be subtracted.
        let line = json!({
            "timestamp": "2026-07-26T12:00:00.000Z",
            "session_id": "s1",
            "payload": { "model": "gpt-5", "info": { "total_token_usage": {
                "input_tokens": 14880,
                "cached_input_tokens": 6912,
                "cache_write_input_tokens": 0,
                "output_tokens": 169,
                "reasoning_output_tokens": 39,
                "total_tokens": 15049
            }}}
        })
        .to_string();

        let r = parse_line("codex", line.as_bytes()).unwrap();

        assert_eq!(r.tokens.input, 14880 - 6912);
        assert_eq!(r.tokens.cache_read, 6912);
        assert_eq!(r.tokens.output, 169 - 39);
        assert_eq!(r.tokens.reasoning, 39);
        // Disjoint buckets must reconcile with the tool's own total.
        assert_eq!(r.tokens.total(), 15049);
        assert_eq!(r.tokens.total(), r.cumulative_total);
    }

    #[test]
    fn a_nested_bucket_larger_than_its_parent_does_not_underflow() {
        let line = json!({
            "timestamp": "2026-07-26T12:00:00.000Z",
            "session_id": "s1",
            "payload": { "info": { "total_token_usage": {
                "input_tokens": 10, "cached_input_tokens": 999,
                "output_tokens": 5, "reasoning_output_tokens": 999,
                "total_tokens": 15
            }}}
        })
        .to_string();

        let r = parse_line("codex", line.as_bytes()).unwrap();
        assert_eq!(r.tokens.input, 0);
        assert_eq!(r.tokens.cache_read, 10);
    }

    #[test]
    fn parses_an_opencode_message_row() {
        // The exact shape observed in a real opencode.db, where the parts are
        // already disjoint: 105 + 4217 + 480 + 11008 == 15810.
        let value = serde_json::json!({
            "role": "assistant",
            "modelID": "big-pickle",
            "providerID": "opencode",
            "tokens": {"total": 15810, "input": 105, "output": 4217, "reasoning": 480,
                       "cache": {"write": 0, "read": 11008}},
            "time": {"created": 1778677495265i64}
        });

        let r = parse_opencode("msg_abc", &value).unwrap();

        assert_eq!(r.model, "big-pickle");
        assert_eq!(r.tokens.input, 105);
        assert_eq!(r.tokens.output, 4217);
        assert_eq!(r.tokens.reasoning, 480);
        assert_eq!(r.tokens.cache_read, 11008);
        // Disjoint parts must reconcile with the row's own total.
        assert_eq!(r.tokens.total(), 15810);
        assert!(r.key.is_some(), "rows must be dedupable by message id");
    }

    #[test]
    fn opencode_rows_are_dated_from_a_millisecond_epoch() {
        let value = serde_json::json!({
            "modelID": "m",
            "tokens": {"input": 1, "output": 1},
            "time": {"created": 1_784_000_000_000i64}
        });
        // Seconds would land in 1970; milliseconds land in 2026.
        assert!(parse_opencode("id", &value)
            .unwrap()
            .day
            .starts_with("2026-"));
    }

    #[test]
    fn opencode_rows_without_tokens_or_a_time_are_skipped() {
        assert!(parse_opencode("id", &serde_json::json!({"modelID": "m"})).is_none());
        assert!(parse_opencode(
            "id",
            &serde_json::json!({"modelID": "m", "tokens": {"input": 0, "output": 0},
                                "time": {"created": 1_784_000_000_000i64}})
        )
        .is_none());
        assert!(parse_opencode(
            "id",
            &serde_json::json!({"modelID": "m", "tokens": {"input": 5}})
        )
        .is_none());
    }

    #[test]
    fn the_same_opencode_row_is_never_counted_twice() {
        // The database has no append-only tail, so a changed db is re-read in
        // full; dedup by message id is what keeps that correct.
        let value = serde_json::json!({
            "modelID": "m",
            "tokens": {"input": 10, "output": 5},
            "time": {"created": 1_784_000_000_000i64}
        });
        let mut ledger = Ledger::default();
        for _ in 0..3 {
            let r = parse_opencode("msg_1", &value).unwrap();
            if ledger.claim(&r.day, r.key) {
                ledger.add(&r.day, "opencode", &r.model, &r.tokens);
            }
        }
        assert_eq!(ledger.totals_by_tool()["opencode"].input, 10);
        assert_eq!(ledger.duplicates_skipped, 2);
    }

    #[test]
    fn malformed_json_is_skipped_not_fatal() {
        assert!(parse_line("claude_code", b"{\"usage\": broken\n").is_none());
    }

    // ------------------------------------------------------------ ingestion

    fn ingest(
        dir: &Path,
        name: &str,
        tool: &'static str,
        acc: Accumulation,
        body: &str,
        ledger: &mut Ledger,
    ) -> u64 {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        ingest_file(ledger, tool, acc, &path, &mut Projects::new()).unwrap()
    }

    #[test]
    fn sums_per_message_records() {
        let dir = temp_dir("sum");
        let mut ledger = Ledger::default();
        let body = claude_line("r1", "m1", "claude-opus-5", 100, 10)
            + &claude_line("r2", "m2", "claude-opus-5", 50, 5);

        ingest(
            &dir,
            "a.jsonl",
            "claude_code",
            Accumulation::PerMessage,
            &body,
            &mut ledger,
        );

        let totals = ledger.totals_by_tool();
        assert_eq!(totals["claude_code"].input, 150);
        assert_eq!(totals["claude_code"].output, 15);
        assert_eq!(totals["claude_code"].messages, 2);
    }

    #[test]
    fn the_same_record_in_two_files_is_counted_once() {
        // 89 keys in the measured corpus really do span files.
        let dir = temp_dir("crossfile");
        let mut ledger = Ledger::default();
        let line = claude_line("r1", "m1", "claude-opus-5", 100, 10);

        ingest(
            &dir,
            "a.jsonl",
            "claude_code",
            Accumulation::PerMessage,
            &line,
            &mut ledger,
        );
        ingest(
            &dir,
            "b.jsonl",
            "claude_code",
            Accumulation::PerMessage,
            &line,
            &mut ledger,
        );

        assert_eq!(ledger.totals_by_tool()["claude_code"].input, 100);
        assert_eq!(ledger.duplicates_skipped, 1);
    }

    #[test]
    fn a_cumulative_session_is_not_summed() {
        let dir = temp_dir("cumulative");
        let mut ledger = Ledger::default();
        // Running totals, as Codex writes them.
        let body = codex_line("s1", 100) + &codex_line("s1", 250) + &codex_line("s1", 400);

        ingest(
            &dir,
            "c.jsonl",
            "codex",
            Accumulation::CumulativePerSession,
            &body,
            &mut ledger,
        );

        let total = ledger.totals_by_tool()["codex"].total();
        assert_eq!(total, 400, "summing running totals would give 750");
    }

    #[test]
    fn a_scaled_delta_sums_to_exactly_the_delta() {
        // Integer division across five buckets loses tokens on every record;
        // over a long session that drift is a real undercount.
        for delta in [1u64, 7, 99, 1000, 2_274_321] {
            let snapshot = Tokens {
                input: 333,
                output: 333,
                cache_read: 333,
                cache_creation: 1,
                reasoning: 0,
                messages: 1,
            };
            assert_eq!(
                scale_to_delta(&snapshot, delta).total(),
                delta,
                "delta {delta}"
            );
        }
    }

    #[test]
    fn a_growing_cumulative_session_adds_only_the_increase() {
        let dir = temp_dir("cumulgrow");
        let mut ledger = Ledger::default();
        let path = dir.join("c.jsonl");

        std::fs::write(&path, codex_line("s1", 400)).unwrap();
        ingest_file(
            &mut ledger,
            "codex",
            Accumulation::CumulativePerSession,
            &path,
            &mut Projects::new(),
        )
        .unwrap();
        assert_eq!(ledger.totals_by_tool()["codex"].total(), 400);

        std::fs::write(&path, codex_line("s1", 400) + &codex_line("s1", 900)).unwrap();
        ingest_file(
            &mut ledger,
            "codex",
            Accumulation::CumulativePerSession,
            &path,
            &mut Projects::new(),
        )
        .unwrap();
        assert_eq!(ledger.totals_by_tool()["codex"].total(), 900);
    }

    // ---------------------------------------------------------- incremental

    #[test]
    fn appending_only_counts_the_new_records() {
        let dir = temp_dir("append");
        let path = dir.join("a.jsonl");
        let mut ledger = Ledger::default();

        std::fs::write(&path, claude_line("r1", "m1", "claude-opus-5", 100, 10)).unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        let mut body = std::fs::read_to_string(&path).unwrap();
        body += &claude_line("r2", "m2", "claude-opus-5", 7, 1);
        std::fs::write(&path, &body).unwrap();

        let read = ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        assert!(
            read > 0 && read < body.len() as u64,
            "should read only the tail"
        );
        assert_eq!(ledger.totals_by_tool()["claude_code"].input, 107);
        assert_eq!(
            ledger.duplicates_skipped, 0,
            "the tail must not re-read old records"
        );
    }

    #[test]
    fn an_unchanged_file_is_not_re_read() {
        let dir = temp_dir("unchanged");
        let path = dir.join("a.jsonl");
        std::fs::write(&path, claude_line("r1", "m1", "claude-opus-5", 100, 10)).unwrap();

        let mut ledger = Ledger::default();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();
        let second = ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        assert_eq!(second, 0, "an unchanged file must cost nothing");
        assert_eq!(ledger.totals_by_tool()["claude_code"].input, 100);
    }

    #[test]
    fn a_partial_trailing_line_is_left_for_the_next_pass() {
        let dir = temp_dir("partial");
        let path = dir.join("a.jsonl");
        let complete = claude_line("r1", "m1", "claude-opus-5", 100, 10);
        let partial = &claude_line("r2", "m2", "claude-opus-5", 7, 1)[..40];

        // Mid-write: one complete record plus a truncated one.
        std::fs::write(&path, format!("{complete}{partial}")).unwrap();
        let mut ledger = Ledger::default();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        assert_eq!(ledger.totals_by_tool()["claude_code"].input, 100);

        // The writer finishes the line.
        std::fs::write(
            &path,
            complete.clone() + &claude_line("r2", "m2", "claude-opus-5", 7, 1),
        )
        .unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        assert_eq!(
            ledger.totals_by_tool()["claude_code"].input,
            107,
            "the completed record must be picked up, not lost"
        );
    }

    #[test]
    fn a_truncated_file_is_re_read_from_the_start() {
        let dir = temp_dir("truncate");
        let path = dir.join("a.jsonl");
        let mut ledger = Ledger::default();

        std::fs::write(
            &path,
            claude_line("r1", "m1", "claude-opus-5", 100, 10)
                + &claude_line("r2", "m2", "claude-opus-5", 100, 10),
        )
        .unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        // Rotated away and replaced with something shorter.
        std::fs::write(&path, claude_line("r3", "m3", "claude-opus-5", 5, 1)).unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut Projects::new(),
        )
        .unwrap();

        assert_eq!(ledger.totals_by_tool()["claude_code"].input, 205);
    }

    // ------------------------------------------------------------- payload

    fn git_repo(at: &Path, origin: &str) {
        std::fs::create_dir_all(at.join(".git")).unwrap();
        std::fs::write(
            at.join(".git/config"),
            format!("[remote \"origin\"]\n\turl = {origin}\n"),
        )
        .unwrap();
    }

    /// A Claude line whose `cwd` points somewhere real, so it can resolve.
    fn claude_line_in(cwd: &Path, request: &str, msg: &str, input: u64, output: u64) -> String {
        json!({
            "requestId": request,
            "timestamp": "2026-07-26T12:00:00.000Z",
            "type": "assistant",
            "sessionId": "sess-x",
            "cwd": cwd.to_string_lossy(),
            "message": {
                "id": msg,
                "model": "claude-opus-5",
                "usage": { "input_tokens": input, "output_tokens": output },
            },
        })
        .to_string()
            + "\n"
    }

    #[test]
    fn tokens_are_attributed_to_the_repository_slug() {
        let dir = temp_dir("attribute");
        let repo = dir.join("home/repos/hai-neo");
        std::fs::create_dir_all(&repo).unwrap();
        git_repo(&repo, "git@github.com:holistic-ai/hai-neo.git");

        let mut ledger = Ledger::default();
        let mut projects = Projects {
            home: Some(dir.join("home")),
            cache: Default::default(),
        };
        let body = claude_line_in(&repo, "r1", "m1", 100, 10);
        let path = dir.join("a.jsonl");
        std::fs::write(&path, body).unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut projects,
        )
        .unwrap();

        let by_project = ledger.by_project();
        assert_eq!(by_project.len(), 1);
        let models = &by_project["holistic-ai/hai-neo"];
        assert_eq!(models["claude-opus-5"].input, 100);
        assert_eq!(models["claude-opus-5"].output, 10);
    }

    #[test]
    fn attribution_partitions_the_spend_rather_than_losing_it() {
        // The invariant that keeps the per-project view honest: every token in
        // the tool totals appears in exactly one project bucket. Without the
        // unattributed bucket, usage from a deleted checkout would silently
        // vanish and the two views would disagree.
        let dir = temp_dir("partition");
        let repo = dir.join("home/repos/real");
        std::fs::create_dir_all(&repo).unwrap();
        git_repo(&repo, "git@github.com:acme/real.git");

        let mut ledger = Ledger::default();
        let mut projects = Projects {
            home: Some(dir.join("home")),
            cache: Default::default(),
        };

        let body = claude_line_in(&repo, "r1", "m1", 100, 10)
            // A directory that no longer exists — the common case for a
            // project the operator has deleted.
            + &claude_line_in(&dir.join("home/repos/deleted"), "r2", "m2", 55, 5);
        let path = dir.join("a.jsonl");
        std::fs::write(&path, body).unwrap();
        ingest_file(
            &mut ledger,
            "claude_code",
            Accumulation::PerMessage,
            &path,
            &mut projects,
        )
        .unwrap();

        let tool_total: u64 = ledger.totals_by_tool().values().map(|t| t.total()).sum();
        let project_total: u64 = ledger
            .by_project()
            .values()
            .flat_map(|models| models.values())
            .map(|t| t.total())
            .sum();
        assert_eq!(tool_total, 170);
        assert_eq!(
            project_total, tool_total,
            "per-project totals must reconcile with per-tool totals"
        );

        let by_project = ledger.by_project();
        assert_eq!(by_project["acme/real"]["claude-opus-5"].total(), 110);
        assert_eq!(
            by_project[UNATTRIBUTED]["claude-opus-5"].total(),
            60,
            "usage from a vanished directory must be named, not dropped"
        );
    }

    #[test]
    fn a_working_directory_resolves_once_and_is_then_cached() {
        let dir = temp_dir("cache");
        let repo = dir.join("home/repos/cached");
        std::fs::create_dir_all(&repo).unwrap();
        git_repo(&repo, "git@github.com:acme/cached.git");

        let mut projects = Projects {
            home: Some(dir.join("home")),
            cache: Default::default(),
        };
        let cwd = repo.to_string_lossy().to_string();
        assert_eq!(projects.slug(Some(&cwd)), "acme/cached");
        assert_eq!(projects.cache.len(), 1);
        assert_eq!(projects.slug(Some(&cwd)), "acme/cached");
        assert_eq!(projects.cache.len(), 1, "resolved the same path twice");
    }

    #[test]
    fn a_record_with_no_working_directory_is_unattributed() {
        let mut projects = Projects {
            home: Some(PathBuf::from("/nonexistent-home")),
            cache: Default::default(),
        };
        assert_eq!(projects.slug(None), UNATTRIBUTED);
        assert_eq!(projects.slug(Some("")), UNATTRIBUTED);
    }
    // --- per-session breakdown ---

    /// A Claude line carrying a session id, plus an `ai-title` header record.

    #[test]
    fn an_unchanged_title_setting_does_not_force_a_re_read() {
        let mut ledger = Ledger::default();
        assert!(!ledger.sync_title_policy(false), "needless rebuild");
        ledger.sync_title_policy(true);
        assert!(!ledger.sync_title_policy(true), "needless rebuild");
    }

    #[test]
    fn one_head_read_yields_model_cwd_and_title_together() {
        let dir = temp_dir("header-merge");
        let path = dir.join("h.jsonl");
        std::fs::write(
            &path,
            [
                r#"{"type":"mode","mode":"default"}"#,
                r#"{"type":"ai-title","aiTitle":"Refactor the parser","sessionId":"s9"}"#,
                r#"{"type":"user","cwd":"/tmp/p","sessionId":"s9"}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let header = session_header(&path);
        assert_eq!(header.title.as_deref(), Some("Refactor the parser"));
        assert_eq!(header.cwd.as_deref(), Some("/tmp/p"));
        assert_eq!(header.session.as_deref(), Some("s9"));
    }

    // --------------------------------------------------------------- privacy
    //
    // surface asserted these against the JSON it put on the wire. `surface`
    // has no wire, so they are asserted against the ledger instead — which is
    // the thing that persists to disk and the thing the views read.

    /// The fixture line carries a client-named cwd, a git branch and a session
    /// id. None of the three may survive ingest.
    #[test]
    fn paths_branches_and_session_ids_never_reach_the_ledger() {
        let dir = temp_dir("privacy");
        let mut ledger = Ledger::default();
        let body = claude_line("r1", "m1", "claude-opus-5", 100, 10);

        ingest(
            &dir,
            "a.jsonl",
            "claude_code",
            Accumulation::PerMessage,
            &body,
            &mut ledger,
        );

        let persisted = serde_json::to_string(&ledger).unwrap();

        assert!(persisted.contains("claude-opus-5"), "model should be kept");
        assert!(
            !persisted.contains("secret-client-project"),
            "working directory leaked"
        );
        assert!(!persisted.contains("acme-migration"), "git branch leaked");
        assert!(!persisted.contains("sess-1"), "raw session id leaked");
        assert!(!persisted.contains("/Users/"), "a filesystem path leaked");
        assert!(!persisted.contains("/repos/"), "a filesystem path leaked");
    }

    /// Repository attribution is by remote slug. A repo with no remote is named
    /// as unversioned rather than by its directory path.
    #[test]
    fn the_repo_rollup_carries_slugs_and_never_paths() {
        let home = temp_dir("privacy-repo");
        let repo = home.join("repos/secret-client-project");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(
            repo.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:acme/widgets.git\n",
        )
        .unwrap();

        let mut projects = Projects {
            home: Some(home.clone()),
            cache: Default::default(),
        };
        let slug = projects.slug(Some(&repo.to_string_lossy()));

        assert_eq!(slug, "acme/widgets");
        assert!(!slug.contains('/') || !slug.starts_with('/'), "not a path");
        assert!(!slug.contains("secret-client-project"));
    }

    /// The counterpart: unresolvable usage is named, not dropped, or the daily
    /// totals and the per-repo view would silently disagree.
    #[test]
    fn unattributable_usage_is_named_rather_than_discarded() {
        let home = temp_dir("privacy-unattributed");
        let mut projects = Projects {
            home: Some(home),
            cache: Default::default(),
        };

        assert_eq!(projects.slug(None), UNATTRIBUTED);
        assert_eq!(
            projects.slug(Some("/nonexistent/gone")),
            UNATTRIBUTED,
            "a deleted project must not contribute its path"
        );
    }

    #[test]
    fn a_codex_header_never_leaks_its_system_prompt_or_branch() {
        // A real Codex header carries multiple kilobytes of `base_instructions`
        // and a `git` object. Only named keys may be read.
        let dir = temp_dir("codex-header");
        let path = dir.join("c.jsonl");
        std::fs::write(
            &path,
            json!({
                "type": "session_meta",
                "payload": {
                    "session_id": "codex-1",
                    "cwd": "/tmp/work",
                    "model": "gpt-5",
                    "base_instructions": {"text": "You are Codex, an agent based on GPT-5."},
                    "git": {"branch": "feat/unreleased-thing", "commit": "abc123"},
                },
            })
            .to_string(),
        )
        .unwrap();

        let header = session_header(&path);
        assert_eq!(header.model.as_deref(), Some("gpt-5"));
        assert_eq!(header.cwd.as_deref(), Some("/tmp/work"));
        assert_eq!(header.session.as_deref(), Some("codex-1"));

        let rendered = format!("{header:?}");
        assert!(
            !rendered.contains("You are Codex"),
            "system prompt captured"
        );
        assert!(
            !rendered.contains("unreleased-thing"),
            "git branch captured"
        );
    }
}
