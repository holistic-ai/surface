//! Persistent state for token accounting.
//!
//! Two problems make naive token counting silently wrong, and this module
//! exists to solve both.
//!
//! **Duplicates.** Measured on a real Claude Code corpus: 18,268 usage records
//! reduce to 8,672 distinct `(requestId, message.id)` pairs. Counting them all
//! roughly doubles every figure. 89 of those keys appear in more than one
//! transcript file, so the dedup set has to survive between scans rather than
//! living for the duration of one pass.
//!
//! **Re-reading.** The same corpus is 717 MB and entirely inside a 30-day
//! window. Parsing it every 15 minutes would be pure waste, so each source
//! file carries a byte offset and only the appended tail is read.
//!
//! Both concerns are per-day, which conveniently bounds the file: pruning a
//! day drops its totals and its dedup keys together.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bump when the on-disk shape changes; an older ledger is discarded.
///
/// 2 added per-project attribution to [`DayState`]; 3 added per-session; 4
/// persisted session metadata, which incremental reads cannot re-derive; 5
/// persisted detected plans, which cannot either.
pub const LEDGER_VERSION: u32 = 5;

/// Namespace for session keys, so the same session id under a different
/// product yields a different key.
const SESSION_NAMESPACE: &str = "surface.session.v1";

/// Bucket for usage whose working directory could not be resolved to a
/// repository — most often a project the operator has since deleted.
///
/// Attribution must partition the spend rather than quietly dropping part of
/// it, so unresolved tokens are named instead of discarded. Otherwise the
/// per-project view would silently disagree with the daily totals.
pub const UNATTRIBUTED: &str = "(unattributed)";

/// Per-day cap on retained dedup keys. A heavy day is ~3k records, so this is
/// generous while still bounding a pathological one.
pub const MAX_KEYS_PER_DAY: usize = 50_000;

/// Normalised token counts. Every source maps onto this shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub reasoning: u64,
    pub messages: u64,
}

impl Tokens {
    pub fn add(&mut self, other: &Tokens) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_creation += other.cache_creation;
        self.reasoning += other.reasoning;
        self.messages += other.messages;
    }

    /// Everything billed, cache included. Useful for a single headline figure.
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_creation + self.reasoning
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0 && self.messages == 0
    }
}

/// What a session is, beyond its token counts.
///
/// Persisted rather than rebuilt each scan. Ingest is incremental — a scan
/// where nothing changed reads nothing — so metadata gathered during a read
/// has to survive, or every steady-state scan would report sessions with no
/// tool and no repository. That was a real bug, not a hypothetical: the first
/// scan after a ledger rebuild looked correct and every scan after it shipped
/// empty rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionMeta {
    pub tool: String,
    pub repo: String,
    /// Only ever populated while titles are enabled; see
    /// [`Ledger::sync_title_policy`].
    pub title: Option<String>,
}

/// What we last saw of one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceState {
    pub size: u64,
    pub mtime_unix: i64,
    /// Bytes consumed. Only ever advanced to a newline boundary.
    pub offset: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DayState {
    /// tool -> model -> tokens
    pub tools: BTreeMap<String, BTreeMap<String, Tokens>>,
    /// repository slug -> model -> tokens.
    ///
    /// Held per day so the existing window prune applies to it unchanged; the
    /// payload reports window totals rather than this daily granularity.
    pub projects: BTreeMap<String, BTreeMap<String, Tokens>>,
    /// session key -> model -> tokens. Per day for the same reason.
    ///
    /// Unrelated to [`Ledger::sessions`], which holds Codex's running totals.
    pub by_session: BTreeMap<String, BTreeMap<String, Tokens>>,
    /// Truncated hashes of `(requestId, message.id)` already counted.
    pub seen: BTreeSet<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    pub version: u32,
    pub sources: BTreeMap<String, SourceState>,
    /// For cumulative sources: last-seen running total per session.
    pub sessions: BTreeMap<String, u64>,
    /// `YYYY-MM-DD` -> totals.
    pub days: BTreeMap<String, DayState>,
    /// session key -> what that session is. Survives between scans.
    pub session_meta: BTreeMap<String, SessionMeta>,
    /// tool -> the subscription plan its transcripts most recently named,
    /// e.g. `codex -> team`. Persisted for the same reason as `session_meta`:
    /// a steady-state scan reads no bytes, so anything gathered during a read
    /// has to survive between scans or it only exists on cold ones.
    pub plans: BTreeMap<String, String>,
    /// Whether titles were being collected when this ledger was written.
    pub titles_enabled: bool,
    /// Cumulative counters, reported so a consumer can see dedup ran.
    pub duplicates_skipped: u64,
    pub undedupable_records: u64,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            sources: BTreeMap::new(),
            sessions: BTreeMap::new(),
            days: BTreeMap::new(),
            session_meta: BTreeMap::new(),
            plans: BTreeMap::new(),
            titles_enabled: false,
            duplicates_skipped: 0,
            undedupable_records: 0,
        }
    }
}

/// What to do with a source file this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPlan {
    /// Unchanged since last scan — do not even open it.
    Skip,
    /// Read from this byte offset to EOF.
    ReadFrom(u64),
}

impl Ledger {
    /// Load, discarding anything unreadable or from an older version.
    ///
    /// A corrupt ledger costs us history, not correctness: the next scan
    /// rebuilds from the transcripts, which are the source of truth.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Ledger::default();
        };
        // A version mismatch and a parse failure are the same recovery: start
        // fresh. The cost surfaces as a full re-read in the scan's bytes-read
        // figure rather than as a log line.
        match serde_json::from_slice::<Ledger>(&bytes) {
            Ok(ledger) if ledger.version == LEDGER_VERSION => ledger,
            _ => Ledger::default(),
        }
    }

    /// Write atomically, so a crash mid-write cannot leave a torn ledger that
    /// the next run then discards.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(self)?)?;
        std::fs::rename(&temp, path)
    }

    /// Decide how much of `path` needs reading.
    pub fn plan(&self, path: &Path, size: u64, mtime_unix: i64) -> ReadPlan {
        let key = path.to_string_lossy().to_string();
        let Some(state) = self.sources.get(&key) else {
            return ReadPlan::ReadFrom(0);
        };

        if state.size == size && state.mtime_unix == mtime_unix {
            return ReadPlan::Skip;
        }
        // Shrunk means truncated or rotated; the offset is meaningless now.
        if size < state.offset {
            return ReadPlan::ReadFrom(0);
        }
        ReadPlan::ReadFrom(state.offset)
    }

    /// Record how far we got. `offset` must be a newline boundary.
    pub fn commit_source(&mut self, path: &Path, size: u64, mtime_unix: i64, offset: u64) {
        self.sources.insert(
            path.to_string_lossy().to_string(),
            SourceState {
                size,
                mtime_unix,
                offset,
            },
        );
    }

    /// Claim a dedup key for `day`. `false` means it was already counted.
    ///
    /// `None` means the record carried no usable key — counted, but tallied
    /// separately so the ambiguity is visible rather than hidden.
    pub fn claim(&mut self, day: &str, key: Option<u64>) -> bool {
        let Some(key) = key else {
            self.undedupable_records += 1;
            return true;
        };

        let entry = self.days.entry(day.to_string()).or_default();
        if !entry.seen.insert(key) {
            self.duplicates_skipped += 1;
            return false;
        }
        // Past the cap we stop tracking rather than growing without bound.
        // Losing dedup on a pathological day is better than an unbounded file.
        if entry.seen.len() > MAX_KEYS_PER_DAY {
            entry.seen.pop_first();
        }
        true
    }

    pub fn add(&mut self, day: &str, tool: &str, model: &str, tokens: &Tokens) {
        self.days
            .entry(day.to_string())
            .or_default()
            .tools
            .entry(tool.to_string())
            .or_default()
            .entry(model.to_string())
            .or_default()
            .add(tokens);
    }

    /// Attribute tokens to a repository, alongside the day/tool/model totals.
    ///
    /// Always called with the same tokens as [`Ledger::add`], so the two views
    /// stay reconcilable: summing every project equals summing every tool.
    pub fn add_project(&mut self, day: &str, project: &str, model: &str, tokens: &Tokens) {
        self.days
            .entry(day.to_string())
            .or_default()
            .projects
            .entry(project.to_string())
            .or_default()
            .entry(model.to_string())
            .or_default()
            .add(tokens);
    }

    /// Reconcile the stored ledger with the current title setting.
    ///
    /// Returns `true` when the caller must re-read every source. Titles are
    /// captured during ingest, so flipping the option on would otherwise take
    /// effect only for sessions whose transcript happens to be appended to
    /// afterwards — the setting would appear not to work. Flipping it off
    /// drops stored titles immediately rather than leaving them to age out.
    pub fn sync_title_policy(&mut self, enabled: bool) -> bool {
        if self.titles_enabled == enabled {
            return false;
        }
        if !enabled {
            for meta in self.session_meta.values_mut() {
                meta.title = None;
            }
        }
        self.titles_enabled = enabled;
        // Clearing the offsets is what forces the re-read; the totals stay,
        // and dedup keys keep them from being counted twice.
        self.sources.clear();
        true
    }

    /// Record the plan a tool's transcript names. Last one wins: records are
    /// read in file order, so the latest read reflects a plan change — an
    /// upgrade mid-window should show the plan being paid for now.
    pub fn observe_plan(&mut self, tool: &str, plan: &str) {
        self.plans.insert(tool.to_string(), plan.to_string());
    }

    /// Record what a session is. Called during ingest, kept afterwards.
    pub fn observe_session(&mut self, key: &str, tool: &str, repo: &str, title: Option<&str>) {
        let entry = self.session_meta.entry(key.to_string()).or_default();
        if entry.tool.is_empty() {
            entry.tool = tool.to_string();
        }
        if entry.repo.is_empty() || entry.repo == UNATTRIBUTED {
            entry.repo = repo.to_string();
        }
        if self.titles_enabled && entry.title.is_none() {
            entry.title = title.map(str::to_string);
        }
    }

    /// Attribute tokens to a session, alongside the day/tool/model totals.
    pub fn add_session(&mut self, day: &str, session: &str, model: &str, tokens: &Tokens) {
        self.days
            .entry(day.to_string())
            .or_default()
            .by_session
            .entry(session.to_string())
            .or_default()
            .entry(model.to_string())
            .or_default()
            .add(tokens);
    }

    /// Window totals per session and model, for pricing in the viewers.
    ///
    /// The day granularity is collapsed, but the last day each session was
    /// active is returned alongside it — a session is a thing that happened at a
    /// time, and the totals alone cannot say when.
    pub fn by_session(&self) -> BTreeMap<String, (BTreeMap<String, Tokens>, String)> {
        let mut totals: BTreeMap<String, (BTreeMap<String, Tokens>, String)> = BTreeMap::new();
        for (day, state) in &self.days {
            for (session, models) in &state.by_session {
                let entry = totals.entry(session.clone()).or_default();
                for (model, tokens) in models {
                    entry.0.entry(model.clone()).or_default().add(tokens);
                }
                if *day > entry.1 {
                    entry.1 = day.clone();
                }
            }
        }
        totals
    }

    /// Window totals per repository and model, for pricing in the viewers.
    pub fn by_project(&self) -> BTreeMap<String, BTreeMap<String, Tokens>> {
        let mut totals: BTreeMap<String, BTreeMap<String, Tokens>> = BTreeMap::new();
        for state in self.days.values() {
            for (project, models) in &state.projects {
                let entry = totals.entry(project.clone()).or_default();
                for (model, tokens) in models {
                    entry.entry(model.clone()).or_default().add(tokens);
                }
            }
        }
        totals
    }

    /// The tools whose sessions ran in each repository, from the session
    /// metadata [`observe_session`](Self::observe_session) keeps.
    ///
    /// The projects map itself has no tool axis, so this answers "which tools
    /// ran here", not "which tool produced every token": attribution is per
    /// session and first-wins, so a session that moved between checkouts
    /// marks only the repository it started in.
    pub fn tools_by_repo(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut tools: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for meta in self.session_meta.values() {
            tools
                .entry(meta.repo.clone())
                .or_default()
                .insert(meta.tool.clone());
        }
        tools
    }

    /// Daily rows per repository, newest first — [`rows`](Self::rows) with the
    /// project axis in place of the tool one.
    ///
    /// The day granularity `by_project` collapses is what a per-project chart
    /// needs, and the model is carried through so the viewer can price each day.
    pub fn project_rows(&self) -> Vec<(String, String, String, Tokens)> {
        let mut rows = Vec::new();
        for (day, state) in self.days.iter().rev() {
            for (project, models) in &state.projects {
                for (model, tokens) in models {
                    if !tokens.is_empty() {
                        rows.push((day.clone(), project.clone(), model.clone(), *tokens));
                    }
                }
            }
        }
        rows
    }

    /// Delta for a cumulative source, and remember the new total.
    ///
    /// Codex writes a running total per session, so summing records overcounts
    /// by an order of magnitude — measured at 34.9M against a true 2.27M. Only
    /// the increase since the last scan is new usage.
    pub fn cumulative_delta(&mut self, session: &str, total: u64) -> u64 {
        let previous = self
            .sessions
            .insert(session.to_string(), total)
            .unwrap_or(0);
        if total >= previous {
            total - previous
        } else {
            // Going backwards means the session was reset or replaced under
            // the same id. The new total is all unaccounted usage.
            total
        }
    }

    /// Drop days outside the window, and any source or session no longer on
    /// disk, so the file cannot grow forever.
    pub fn prune(&mut self, oldest_day: &str, live_sources: &BTreeSet<String>) {
        self.days.retain(|day, _| day.as_str() >= oldest_day);
        self.sources.retain(|path, _| live_sources.contains(path));
        // Metadata outlives nothing: once a session's last day falls out of
        // the window it can never be reported again.
        let live_sessions: BTreeSet<&String> = self
            .days
            .values()
            .flat_map(|d| d.by_session.keys())
            .collect();
        self.session_meta
            .retain(|key, _| live_sessions.contains(key));
        // Sessions are only meaningful while their file exists.
        if self.sessions.len() > 10_000 {
            self.sessions.clear();
        }
    }

    /// Daily rows inside the window, newest first.
    pub fn rows(&self) -> Vec<(String, String, String, Tokens)> {
        let mut rows = Vec::new();
        for (day, state) in self.days.iter().rev() {
            for (tool, models) in &state.tools {
                for (model, tokens) in models {
                    if !tokens.is_empty() {
                        rows.push((day.clone(), tool.clone(), model.clone(), *tokens));
                    }
                }
            }
        }
        rows
    }

    pub fn totals_by_tool(&self) -> BTreeMap<String, Tokens> {
        let mut totals: BTreeMap<String, Tokens> = BTreeMap::new();
        for state in self.days.values() {
            for (tool, models) in &state.tools {
                let entry = totals.entry(tool.clone()).or_default();
                for tokens in models.values() {
                    entry.add(tokens);
                }
            }
        }
        totals
    }
}

/// Short, stable, one-way key for a session id.
///
/// Deliberately **not** [`dedup_key`]'s `DefaultHasher`: that is explicitly not
/// stable across Rust releases, which is harmless for dedup state the ledger
/// rebuilds anyway, but would be a real defect here. A backend correlates these
/// keys over time and across devices, so a toolchain upgrade must not silently
/// rekey every session, and two machines on different builds must agree.
///
/// One-way by design. The key identifies a session across scans without
/// letting anyone reach the transcript it came from.
pub fn session_key(session_id: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(SESSION_NAMESPACE.as_bytes());
    hasher.update(b":");
    hasher.update(session_id.trim().as_bytes());
    // 32 bits is ample to keep one device's few hundred sessions distinct, and
    // short enough to read in a terminal.
    format!("{:x}", hasher.finalize())[..8].to_string()
}

/// Stable 64-bit key for a `(requestId, message.id)` pair.
pub fn dedup_key(request_id: Option<&str>, message_id: Option<&str>) -> Option<u64> {
    use std::hash::{Hash, Hasher};

    // Both absent means we cannot tell two records apart at all.
    if request_id.is_none() && message_id.is_none() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request_id.unwrap_or("").hash(&mut hasher);
    0xffu8.hash(&mut hasher);
    message_id.unwrap_or("").hash(&mut hasher);
    Some(hasher.finish())
}

/// Path of the ledger inside the agent's state directory.
pub fn ledger_path(state_dir: &Path) -> PathBuf {
    state_dir.join("usage-ledger.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: u64, output: u64) -> Tokens {
        Tokens {
            input,
            output,
            messages: 1,
            ..Default::default()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("surface-ledger-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ------------------------------------------------------------- dedup

    #[test]
    fn a_session_key_is_stable_and_one_way() {
        // The whole point: a backend correlates these across scans, devices and
        // toolchains. If this ever changes, every historical row is orphaned.
        let id = "b9621bde-2428-4432-8a7a-3547085af7ee";
        let key = session_key(id);
        assert_eq!(key.len(), 8);
        assert_eq!(key, session_key(id), "not stable within one build");
        assert_eq!(
            key, "bb2edb21",
            "the key changed — every previously reported session is now orphaned"
        );
        assert!(!key.contains(&id[..8]), "the key echoes the id");
    }

    #[test]
    fn different_sessions_get_different_keys() {
        assert_ne!(session_key("a"), session_key("b"));
        assert_ne!(session_key(""), session_key("a"));
    }

    #[test]
    fn the_same_record_is_counted_once() {
        let mut ledger = Ledger::default();
        let key = dedup_key(Some("req-1"), Some("msg-1"));

        assert!(ledger.claim("2026-07-26", key));
        assert!(!ledger.claim("2026-07-26", key));
        assert_eq!(ledger.duplicates_skipped, 1);
    }

    #[test]
    fn dedup_survives_a_reload_because_duplicates_span_files() {
        // 89 keys in the measured corpus appear in more than one transcript,
        // and incremental reads may see them in different scans.
        let dir = temp_dir("persist");
        let path = ledger_path(&dir);
        let key = dedup_key(Some("req-1"), Some("msg-1"));

        let mut first = Ledger::default();
        assert!(first.claim("2026-07-26", key));
        first.save(&path).unwrap();

        let mut second = Ledger::load(&path);
        assert!(!second.claim("2026-07-26", key), "dedup did not persist");
    }

    #[test]
    fn records_with_no_key_are_counted_but_tallied() {
        let mut ledger = Ledger::default();
        assert!(ledger.claim("2026-07-26", None));
        assert!(ledger.claim("2026-07-26", None));
        assert_eq!(ledger.undedupable_records, 2);
        assert_eq!(ledger.duplicates_skipped, 0);
    }

    #[test]
    fn keys_differ_on_each_component() {
        let a = dedup_key(Some("req-1"), Some("msg-1"));
        assert_ne!(a, dedup_key(Some("req-2"), Some("msg-1")));
        assert_ne!(a, dedup_key(Some("req-1"), Some("msg-2")));
        assert_eq!(a, dedup_key(Some("req-1"), Some("msg-1")));
        assert_eq!(dedup_key(None, None), None);
        // A partial key is still usable.
        assert!(dedup_key(Some("req-1"), None).is_some());
    }

    #[test]
    fn concatenation_cannot_collide_two_different_pairs() {
        // Without a separator, ("ab","c") and ("a","bc") would hash alike.
        assert_ne!(
            dedup_key(Some("ab"), Some("c")),
            dedup_key(Some("a"), Some("bc"))
        );
    }

    // -------------------------------------------------------- incremental

    #[test]
    fn an_unchanged_file_is_skipped_entirely() {
        let mut ledger = Ledger::default();
        let path = Path::new("/tmp/a.jsonl");
        ledger.commit_source(path, 100, 42, 100);

        assert_eq!(ledger.plan(path, 100, 42), ReadPlan::Skip);
    }

    #[test]
    fn an_unseen_file_is_read_from_the_start() {
        let ledger = Ledger::default();
        assert_eq!(
            ledger.plan(Path::new("/tmp/new.jsonl"), 500, 1),
            ReadPlan::ReadFrom(0)
        );
    }

    #[test]
    fn an_appended_file_is_read_from_the_previous_offset() {
        let mut ledger = Ledger::default();
        let path = Path::new("/tmp/a.jsonl");
        ledger.commit_source(path, 100, 42, 100);

        assert_eq!(ledger.plan(path, 250, 43), ReadPlan::ReadFrom(100));
    }

    #[test]
    fn a_truncated_file_is_re_read_from_the_start() {
        let mut ledger = Ledger::default();
        let path = Path::new("/tmp/a.jsonl");
        ledger.commit_source(path, 500, 42, 500);

        // Rotated or cleared; the stored offset is past EOF now.
        assert_eq!(ledger.plan(path, 20, 99), ReadPlan::ReadFrom(0));
    }

    #[test]
    fn a_file_touched_without_growing_is_read_from_its_offset() {
        let mut ledger = Ledger::default();
        let path = Path::new("/tmp/a.jsonl");
        ledger.commit_source(path, 100, 42, 100);

        // Same size, new mtime: re-check rather than assume nothing changed.
        assert_eq!(ledger.plan(path, 100, 99), ReadPlan::ReadFrom(100));
    }

    // -------------------------------------------------------- cumulative

    #[test]
    fn a_cumulative_session_contributes_its_latest_total_once() {
        let mut ledger = Ledger::default();
        // Codex writes a running total; the first sighting is the whole total.
        assert_eq!(ledger.cumulative_delta("s1", 400), 400);
    }

    #[test]
    fn a_growing_cumulative_session_contributes_only_the_increase() {
        let mut ledger = Ledger::default();
        assert_eq!(ledger.cumulative_delta("s1", 400), 400);
        assert_eq!(ledger.cumulative_delta("s1", 900), 500);
        assert_eq!(ledger.cumulative_delta("s1", 900), 0);
    }

    #[test]
    fn summing_cumulative_records_would_overcount() {
        // Running totals from a real Codex session. Across its full 35 records
        // the naive sum reaches 34,935,584 against a true total of 2,274,321 —
        // a 15x overcount. This subsample shows the same failure mode.
        let mut ledger = Ledger::default();
        let running = [
            15_049u64, 30_346, 55_504, 88_544, 129_779, 179_034, 2_274_321,
        ];

        let counted: u64 = running
            .iter()
            .map(|t| ledger.cumulative_delta("s1", *t))
            .sum();

        // Deltas telescope to exactly the final running total.
        assert_eq!(counted, 2_274_321);
        assert!(
            counted < running.iter().sum::<u64>(),
            "summing running totals must overcount, or the fixture is wrong"
        );
    }

    #[test]
    fn a_reset_session_does_not_underflow() {
        let mut ledger = Ledger::default();
        ledger.cumulative_delta("s1", 900);
        // Session replaced by a shorter one under the same id.
        assert_eq!(ledger.cumulative_delta("s1", 100), 100);
    }

    // ------------------------------------------------------------ rollup

    #[test]
    fn rows_are_newest_first_and_skip_empty_entries() {
        let mut ledger = Ledger::default();
        ledger.add("2026-07-24", "claude_code", "opus-5", &tokens(1, 1));
        ledger.add("2026-07-26", "claude_code", "opus-5", &tokens(2, 2));
        ledger.add("2026-07-25", "claude_code", "empty", &Tokens::default());

        let rows = ledger.rows();
        assert_eq!(rows[0].0, "2026-07-26");
        assert_eq!(rows[1].0, "2026-07-24");
        assert_eq!(rows.len(), 2, "empty model row should not be reported");
    }

    // ------------------------------------------------------------ pruning

    #[test]
    fn days_outside_the_window_are_dropped_with_their_keys() {
        let mut ledger = Ledger::default();
        ledger.add("2026-06-01", "claude_code", "opus-5", &tokens(1, 1));
        ledger.claim("2026-06-01", dedup_key(Some("old"), Some("old")));
        ledger.add("2026-07-26", "claude_code", "opus-5", &tokens(2, 2));

        ledger.prune("2026-07-01", &BTreeSet::new());

        assert!(!ledger.days.contains_key("2026-06-01"));
        assert!(ledger.days.contains_key("2026-07-26"));
    }

    #[test]
    fn sources_that_no_longer_exist_are_forgotten() {
        let mut ledger = Ledger::default();
        ledger.commit_source(Path::new("/tmp/gone.jsonl"), 1, 1, 1);
        ledger.commit_source(Path::new("/tmp/here.jsonl"), 1, 1, 1);

        let live: BTreeSet<String> = ["/tmp/here.jsonl".to_string()].into_iter().collect();
        ledger.prune("2000-01-01", &live);

        assert_eq!(ledger.sources.len(), 1);
        assert!(ledger.sources.contains_key("/tmp/here.jsonl"));
    }

    #[test]
    fn the_dedup_set_is_capped_per_day() {
        let mut ledger = Ledger::default();
        for i in 0..(MAX_KEYS_PER_DAY as u64 + 100) {
            ledger.claim("2026-07-26", Some(i));
        }
        assert!(ledger.days["2026-07-26"].seen.len() <= MAX_KEYS_PER_DAY + 1);
    }

    // ------------------------------------------------------------ storage

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let path = ledger_path(&dir);

        let mut ledger = Ledger::default();
        ledger.add("2026-07-26", "claude_code", "opus-5", &tokens(100, 10));
        ledger.commit_source(Path::new("/tmp/a.jsonl"), 10, 20, 10);
        ledger.cumulative_delta("s1", 400);
        ledger.save(&path).unwrap();

        let loaded = Ledger::load(&path);
        assert_eq!(loaded.totals_by_tool()["claude_code"].input, 100);
        assert_eq!(loaded.sessions["s1"], 400);
        assert_eq!(
            loaded.plan(Path::new("/tmp/a.jsonl"), 10, 20),
            ReadPlan::Skip
        );
    }

    /// Plans are gathered during a read and steady-state scans read nothing,
    /// so a plan that fails to persist exists only on cold scans — the same
    /// trap session metadata fell into before it was persisted.
    #[test]
    fn a_detected_plan_survives_the_round_trip_and_the_last_one_wins() {
        let dir = temp_dir("plans");
        let path = ledger_path(&dir);
        let mut ledger = Ledger::default();
        ledger.observe_plan("codex", "pro");
        ledger.observe_plan("codex", "team");
        ledger.save(&path).unwrap();

        assert_eq!(
            Ledger::load(&path).plans.get("codex").map(String::as_str),
            Some("team")
        );
    }

    #[test]
    fn a_corrupt_ledger_is_rebuilt_rather_than_fatal() {
        let dir = temp_dir("corrupt");
        let path = ledger_path(&dir);
        std::fs::write(&path, b"{not json").unwrap();

        let ledger = Ledger::load(&path);
        assert!(ledger.days.is_empty());
        assert_eq!(ledger.version, LEDGER_VERSION);
    }

    #[test]
    fn a_ledger_from_a_future_version_is_discarded() {
        let dir = temp_dir("version");
        let path = ledger_path(&dir);
        std::fs::write(&path, br#"{"version":999,"days":{"2026-07-26":{}}}"#).unwrap();

        assert!(Ledger::load(&path).days.is_empty());
    }

    #[test]
    fn a_missing_ledger_starts_empty() {
        assert!(Ledger::load(Path::new("/nonexistent/usage-ledger.json"))
            .days
            .is_empty());
    }
}
