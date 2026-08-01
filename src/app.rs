//! Dashboard state and the row builders the views read.
//!
//! surface's TUI dug every figure out of a `serde_json::Value` because it read
//! a generic snapshot from a file the agent had written. `surface` scans in
//! process, so this walks the typed [`Scan`] and the [`Ledger`] directly — which
//! is both less code and checked by the compiler.
//!
//! Derived rows are built once per scan and cached, not recomputed per frame:
//! `by_project` over a month of usage is a real walk, and the draw path runs on
//! every keypress.

use std::collections::BTreeMap;

use crate::config::CostConfig;
use crate::ledger::{Ledger, Tokens};
use crate::pricing::{Cost, Prices};
use crate::scan::{Scan, Timings};
use crate::ui::chart::Bucket;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Tools,
    Sites,
    Usage,
    Cost,
    Projects,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[
        Tab::Overview,
        Tab::Tools,
        Tab::Sites,
        Tab::Usage,
        Tab::Cost,
        Tab::Projects,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Tools => "Tools",
            Tab::Sites => "Sites",
            Tab::Usage => "Usage",
            Tab::Cost => "Cost",
            Tab::Projects => "Projects",
        }
    }
}

/// Active days a window needs before a half-against-half comparison is worth
/// showing. Below this the delta swings on single days and misleads.
pub const TREND_MIN_DAYS: usize = 6;

/// Which unit the Overview's chart is in. A display preference, so it survives a
/// change of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Spend,
    Tokens,
}

/// The window's second half against its first.
#[derive(Debug, Clone, Copy)]
pub struct Trend {
    /// Signed fraction: `0.42` is up 42%.
    pub change: f64,
    /// How many days each half covers, so the card can say what it compared.
    pub days: u64,
}

/// How many models the charts name before folding the rest together. One less
/// than the palette, because `other` takes the last slot.
pub const MODEL_SLOTS: usize = 6;

/// The bucket every model past [`MODEL_SLOTS`] is charted under. Not a real model
/// name, and deliberately lower case so it cannot collide with one.
pub const OTHER_MODELS: &str = "other";

/// Which of a view's two tables the keyboard is driving.
///
/// Only Projects has two. Everywhere else this is always `Primary`, which is why
/// the movers ask [`App::focus`] rather than reading the field: it answers for
/// the view actually on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Primary,
    Sessions,
}

/// `(day, series name -> value, day total)` rows, ready for [`bucket`]. The
/// unit is whatever the builder put in: tokens, or micro-dollars.
type DailySeries = Vec<(String, BTreeMap<String, u64>, u64)>;

/// How daily rows are grouped in the charts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Week,
    Month,
}

impl Granularity {
    pub fn label(&self) -> &'static str {
        match self {
            Granularity::Day => "day",
            Granularity::Week => "week",
            Granularity::Month => "month",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Granularity::Day => Granularity::Week,
            Granularity::Week => Granularity::Month,
            Granularity::Month => Granularity::Day,
        }
    }

    /// Bucket key for a `YYYY-MM-DD` date.
    fn bucket_of(&self, date: &str) -> String {
        match self {
            Granularity::Day => date.to_string(),
            Granularity::Month => date.get(..7).unwrap_or(date).to_string(),
            Granularity::Week => iso_week(date).unwrap_or_else(|| date.to_string()),
        }
    }
}

/// `2026-07-28` -> `2026-W31`. ISO weeks, so a week never straddles two labels.
fn iso_week(date: &str) -> Option<String> {
    use chrono::{Datelike, NaiveDate};
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let week = parsed.iso_week();
    Some(format!("{}-W{:02}", week.year(), week.week()))
}

/// One AI tool that is installed.
#[derive(Debug, Clone)]
pub struct ToolRow {
    pub name: &'static str,
    pub vendor: &'static str,
    pub kind: &'static str,
    pub autonomous: bool,
    pub evidence: Vec<String>,
}

/// One model's usage and cost over the window.
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub tool: String,
    pub model: String,
    pub tokens: Tokens,
    pub cost: Cost,
}

/// One repository's spend over the window.
#[derive(Debug, Clone)]
pub struct RepoRow {
    pub repo: String,
    pub tokens: u64,
    pub messages: u64,
    pub usd: f64,
    /// Models under this repo with no price. A total above them is a floor.
    pub unpriced: usize,
    /// Last day this repository saw any usage, `YYYY-MM-DD`. Empty only if the
    /// row somehow has no days under it, which the builder cannot produce.
    pub last_day: String,
    /// The tools whose sessions ran in this repository, in stable order.
    /// Derived from session metadata: the projects map itself has no tool
    /// axis, and two slugs like `HAI Neo` and `owner/hai-neo` are otherwise
    /// indistinguishable as "the Codex one" and "the Claude Code one".
    pub tools: Vec<String>,
}

/// One session's usage over the window, for the breakdown under a project.
///
/// A session is the unit a person actually recognises — "that afternoon on the
/// billing bug" — which the repository total flattens away. The key is the
/// ledger's one-way hash, so this identifies a session without being able to
/// reach the transcript it came from.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub key: String,
    pub tool: String,
    pub repo: String,
    /// Only ever present when title collection is enabled; see
    /// `Ledger::sync_title_policy`.
    pub title: Option<String>,
    /// Every model this session used, in ledger order.
    pub models: Vec<String>,
    pub tokens: Tokens,
    pub usd: f64,
    /// Models in this session with no price. A total above them is a floor.
    pub unpriced: usize,
    /// Last day this session saw any usage, `YYYY-MM-DD`.
    pub last_day: String,
}

impl SessionRow {
    /// What to show in a column too narrow for the whole story: the title if we
    /// have one, otherwise a short form of the key so the rows stay tellable
    /// apart.
    pub fn label(&self) -> String {
        match &self.title {
            Some(title) if !title.is_empty() => title.clone(),
            _ => format!("session {}", self.key.chars().take(8).collect::<String>()),
        }
    }
}

/// A subscription against what the same usage would have cost on API rates.
#[derive(Debug, Clone)]
pub struct SubscriptionRow {
    pub tool: String,
    pub monthly: f64,
    /// The monthly figure is a published list price, not something configured.
    pub estimated: bool,
    pub api_equivalent: f64,
}

impl SubscriptionRow {
    /// Positive when the subscription is cheaper than paying per token.
    pub fn saving(&self) -> f64 {
        self.api_equivalent - self.monthly
    }
}

pub struct App {
    pub scan: Scan,
    pub timings: Timings,
    pub prices: Prices,
    pub cost_config: CostConfig,

    pub tab: Tab,
    pub selected: usize,
    pub granularity: Granularity,
    pub show_help: bool,
    /// Which pane j/k drives. Read through [`App::focus`], never directly.
    focus: Pane,
    /// Cursor within the selected project's sessions. Reset, not clamped, when
    /// the project cursor moves — see the note above `focus`.
    session_selected: usize,
    /// The bucket the charts point at, counted *back from the newest*.
    ///
    /// Back from the newest rather than an index, because the index space moves
    /// under it constantly: regrouping by week collapses thirty buckets into
    /// five, and a narrower panel drops the oldest ones off the left. "Three
    /// buckets ago" survives both; "bucket 17" does not.
    bucket_back: Option<usize>,
    /// Which unit the Overview charts. Spend by default: it is what the tool is
    /// for, and what the cards lead with.
    pub unit: Unit,
    /// Whether the token-split detail line is drawn under each row in the
    /// tables that have one. On by default: the figures it carries are billed,
    /// and they were invisible before it existed.
    pub detail: bool,
    pub should_quit: bool,
    pub status_line: Option<String>,

    // Derived once per scan.
    tools: Vec<ToolRow>,
    models: Vec<ModelRow>,
    repos: Vec<RepoRow>,
    sessions: Vec<SessionRow>,
    series: Vec<String>,
    /// The models the Usage and Cost charts name, biggest first, with everything
    /// past the palette folded into one `other` entry. This is the list both the
    /// chart segments and the table's model tints index into, which is what makes
    /// the two agree.
    model_names: Vec<String>,
    /// The same, split by model rather than tool.
    model_token_days: DailySeries,
    model_cost_days: DailySeries,
    /// repository slug -> its own daily token rows, for the per-project chart.
    project_days: BTreeMap<String, DailySeries>,
}

impl App {
    pub fn new(scan: Scan, timings: Timings, prices: Prices, cost_config: CostConfig) -> Self {
        let mut app = Self {
            scan,
            timings,
            prices,
            cost_config,
            tab: Tab::Overview,
            selected: 0,
            granularity: Granularity::Day,
            show_help: false,
            focus: Pane::Primary,
            session_selected: 0,
            bucket_back: None,
            unit: Unit::Spend,
            detail: true,
            should_quit: false,
            status_line: None,
            tools: Vec::new(),
            models: Vec::new(),
            repos: Vec::new(),
            sessions: Vec::new(),
            series: Vec::new(),
            model_names: Vec::new(),
            model_token_days: Vec::new(),
            model_cost_days: Vec::new(),
            project_days: BTreeMap::new(),
        };
        app.rebuild();
        app
    }

    fn ledger(&self) -> &Ledger {
        &self.scan.usage.ledger
    }

    /// Recompute every derived view. Called once, after a scan.
    fn rebuild(&mut self) {
        self.tools = self
            .scan
            .tools
            .iter()
            .map(|d| ToolRow {
                name: d.tool.name,
                vendor: d.tool.vendor,
                kind: d.tool.kind.label(),
                autonomous: d.tool.autonomous,
                evidence: d.evidence.clone(),
            })
            .collect();

        // Series slots are assigned by tool identity in a stable order, never by
        // rank — so a tool keeps its colour between scans and between the two
        // charts.
        self.series = self.ledger().totals_by_tool().into_keys().collect();

        let mut models: Vec<ModelRow> = Vec::new();
        let mut per_model: BTreeMap<(String, String), Tokens> = BTreeMap::new();
        for (_day, tool, model, tokens) in self.ledger().rows() {
            per_model.entry((tool, model)).or_default().add(&tokens);
        }
        for ((tool, model), tokens) in per_model {
            let cost = self.prices.cost(&model, &tokens);
            models.push(ModelRow {
                tool,
                model,
                tokens,
                cost,
            });
        }
        // Most expensive first, then most tokens: the reader is here for spend.
        models.sort_by(|a, b| {
            b.cost
                .usd()
                .total_cmp(&a.cost.usd())
                .then(b.tokens.total().cmp(&a.tokens.total()))
        });
        self.models = models;

        // One entry per model name, however many tools ran it, so the same model
        // under two tools reads as one thing in both the chart and the table.
        //
        // Ranked by tokens rather than sorted by name, because this list is also
        // the chart's legend: the six models actually worth naming should be the
        // six the reader is looking at. Everything past the palette folds into
        // `other` rather than being given a colour it would share.
        let mut totals: BTreeMap<String, u64> = BTreeMap::new();
        for row in &self.models {
            *totals.entry(row.model.clone()).or_default() += row.tokens.total();
        }
        let mut ranked: Vec<(String, u64)> = totals.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let named = MODEL_SLOTS.min(ranked.len());
        let mut names: Vec<String> = ranked[..named].iter().map(|(m, _)| m.clone()).collect();
        if ranked.len() > named {
            names.push(OTHER_MODELS.to_string());
        }
        self.model_names = names;

        // Daily rows per project, in tokens. One series per chart — the project
        // itself — because a repository's models run well past the three colour
        // slots, and a repeated texture would claim two models are the same one.
        let mut tokens_by_project: BTreeMap<String, BTreeMap<String, BTreeMap<String, u64>>> =
            BTreeMap::new();
        let mut last_day: BTreeMap<String, String> = BTreeMap::new();
        let mut every_day: Vec<String> = Vec::new();
        for (day, repo, _model, tokens) in self.ledger().project_rows() {
            *tokens_by_project
                .entry(repo.clone())
                .or_default()
                .entry(day.clone())
                .or_default()
                .entry(repo.clone())
                .or_default() += tokens.total();
            if every_day.last() != Some(&day) && !every_day.contains(&day) {
                every_day.push(day.clone());
            }
            let seen = last_day.entry(repo).or_default();
            if day > *seen {
                *seen = day;
            }
        }
        // Pad every project out to the same days. A project is typically idle
        // most of the window, and without the zeros its chart would show its
        // active days shoulder to shoulder — a fortnight's gap between two bars
        // reading as if they were consecutive.
        self.project_days = tokens_by_project
            .into_iter()
            .map(|(repo, mut by_day)| {
                for day in &every_day {
                    by_day.entry(day.clone()).or_default();
                }
                (repo, flatten(by_day))
            })
            .collect();

        // Which tools ran in each repository, from the session metadata the
        // ledger already keeps. Slots follow `self.series`, so a repo's tools
        // list in the same order (and texture) as the usage chart's legend.
        let mut tools_by_repo: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for tool in &self.series {
            for meta in self.ledger().session_meta.values() {
                if meta.tool == *tool {
                    let entry = tools_by_repo.entry(meta.repo.clone()).or_default();
                    if !entry.contains(tool) {
                        entry.push(tool.clone());
                    }
                }
            }
        }

        let mut repos: Vec<RepoRow> = self
            .ledger()
            .by_project()
            .into_iter()
            .map(|(repo, models)| {
                let mut tokens = Tokens::default();
                let mut usd = 0.0;
                let mut unpriced = 0;
                for (model, t) in &models {
                    tokens.add(t);
                    let cost = self.prices.cost(model, t);
                    usd += cost.usd();
                    if cost.is_unpriced() {
                        unpriced += 1;
                    }
                }
                RepoRow {
                    last_day: last_day.get(&repo).cloned().unwrap_or_default(),
                    tools: tools_by_repo.remove(&repo).unwrap_or_default(),
                    repo,
                    tokens: tokens.total(),
                    messages: tokens.messages,
                    usd,
                    unpriced,
                }
            })
            .collect();
        repos.sort_by(|a, b| b.usd.total_cmp(&a.usd).then(b.tokens.cmp(&a.tokens)));
        self.repos = repos;

        // Sessions, costed the same way the repositories above are, so the
        // breakdown adds up to the row it sits under.
        let mut sessions: Vec<SessionRow> = self
            .ledger()
            .by_session()
            .into_iter()
            .map(|(key, (models, last_day))| {
                let mut tokens = Tokens::default();
                let mut usd = 0.0;
                let mut unpriced = 0;
                for (model, t) in &models {
                    tokens.add(t);
                    let cost = self.prices.cost(model, t);
                    usd += cost.usd();
                    if cost.is_unpriced() {
                        unpriced += 1;
                    }
                }
                let meta = self.ledger().session_meta.get(&key);
                SessionRow {
                    tool: meta.map(|m| m.tool.clone()).unwrap_or_default(),
                    repo: meta.map(|m| m.repo.clone()).unwrap_or_default(),
                    title: meta.and_then(|m| m.title.clone()),
                    models: models.into_keys().collect(),
                    key,
                    tokens,
                    usd,
                    unpriced,
                    last_day,
                }
            })
            .collect();
        sessions.sort_by(|a, b| {
            b.usd
                .total_cmp(&a.usd)
                .then(b.tokens.total().cmp(&a.tokens.total()))
        });
        self.sessions = sessions;

        // Daily rows, in both units. Cost is accumulated in micro-dollars so the
        // shared integer stacking maths serves both charts.
        // Daily rows in both units, split by model — which is what every chart
        // with a palette is keyed by. A per-*tool* pair of these used to be built
        // here as well and rendered by nothing at all: the charts moved to models,
        // `bucket_count` was the last caller and it was naming the wrong series
        // anyway. The tool axis is served by `spend_by_tool`, which folds over
        // `models()` on demand and needs no cached series.
        let mut model_tokens_by_day: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        let mut model_cost_by_day: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        for (day, _tool, model, tokens) in self.ledger().rows() {
            let micro = (self.prices.cost(&model, &tokens).usd() * 1_000_000.0).round() as u64;
            let named = self.fold_model(&model);
            *model_tokens_by_day
                .entry(day.clone())
                .or_default()
                .entry(named.clone())
                .or_default() += tokens.total();
            *model_cost_by_day
                .entry(day)
                .or_default()
                .entry(named)
                .or_default() += micro;
        }
        self.model_token_days = flatten(model_tokens_by_day);
        self.model_cost_days = flatten(model_cost_by_day);
    }

    // ------------------------------------------------------------- accessors

    pub fn tools(&self) -> &[ToolRow] {
        &self.tools
    }

    pub fn model_names(&self) -> &[String] {
        &self.model_names
    }

    /// The name this model is charted under: itself, or `other` if it did not
    /// make the palette.
    pub fn fold_model(&self, model: &str) -> String {
        if self.model_names.iter().any(|known| known == model) {
            model.to_string()
        } else {
            OTHER_MODELS.to_string()
        }
    }

    pub fn model_token_buckets(&self) -> Vec<Bucket> {
        bucket(&self.model_token_days, self.granularity)
    }

    pub fn model_cost_buckets(&self) -> Vec<Bucket> {
        bucket(&self.model_cost_days, self.granularity)
    }

    pub fn models(&self) -> &[ModelRow] {
        &self.models
    }

    pub fn repos(&self) -> &[RepoRow] {
        &self.repos
    }

    pub fn series(&self) -> &[String] {
        &self.series
    }

    /// One repository's tokens over time. Empty for a repository with no rows,
    /// which the chart renders as nothing rather than as a flat zero.
    pub fn project_buckets(&self, repo: &str) -> Vec<Bucket> {
        match self.project_days.get(repo) {
            Some(days) => bucket(days, self.granularity),
            None => Vec::new(),
        }
    }

    /// The project the selection is on, for the chart above the table.
    pub fn selected_project(&self) -> Option<&RepoRow> {
        self.repos.get(self.selected)
    }

    /// The sessions that ran in one repository, costliest first.
    ///
    /// Matched on the slug the ledger recorded against each session, which is
    /// the same slug the project rows are keyed by — so a session appears under
    /// exactly the project it was attributed to, including `(unattributed)`.
    pub fn sessions_in(&self, repo: &str) -> Vec<&SessionRow> {
        self.sessions.iter().filter(|s| s.repo == repo).collect()
    }

    pub fn total_tokens(&self) -> u64 {
        self.ledger()
            .totals_by_tool()
            .values()
            .map(|t| t.total())
            .sum()
    }

    pub fn total_messages(&self) -> u64 {
        self.ledger()
            .totals_by_tool()
            .values()
            .map(|t| t.messages)
            .sum()
    }

    pub fn total_usd(&self) -> f64 {
        self.models.iter().map(|m| m.cost.usd()).sum()
    }

    /// Models with no price. A total with these under it is a floor, not a figure.
    pub fn unpriced_models(&self) -> usize {
        self.models.iter().filter(|m| m.cost.is_unpriced()).count()
    }

    /// Spend per tool over the window, biggest first.
    ///
    /// This table used to be built inside [`App::subscriptions`] and thrown away:
    /// its `filter_map` keeps only tools with a configured `[cost.subscriptions]`
    /// entry, so on a default config the whole thing was discarded. It is the tool
    /// axis the Overview needs, and `subscriptions` now reads it from here.
    pub fn spend_by_tool(&self) -> Vec<(String, f64)> {
        let mut per_tool: BTreeMap<String, f64> = BTreeMap::new();
        for model in &self.models {
            *per_tool.entry(model.tool.clone()).or_default() += model.cost.usd();
        }
        let mut rows: Vec<(String, f64)> = per_tool.into_iter().collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        rows
    }

    /// Spend per model over the window, biggest first, carrying the number of
    /// unpriced `(tool, model)` pairs under each — a total above one of those is a
    /// floor, not a figure.
    ///
    /// Real model names, not the folded `model_names()` the charts use: a ranking
    /// is read for its names, so the panel shows the top few and sums the rest
    /// rather than lumping them under `other`.
    pub fn spend_by_model(&self) -> Vec<(String, f64, usize)> {
        let mut per_model: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for model in &self.models {
            let entry = per_model.entry(model.model.clone()).or_default();
            entry.0 += model.cost.usd();
            entry.1 += usize::from(model.cost.is_unpriced());
        }
        let mut rows: Vec<(String, f64, usize)> = per_model
            .into_iter()
            .map(|(name, (usd, unpriced))| (name, usd, unpriced))
            .collect();
        rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        rows
    }

    /// Distinct models, as opposed to the `(tool, model)` pairs [`App::models`]
    /// lists. One model run by two tools is one model.
    pub fn distinct_models(&self) -> usize {
        self.models
            .iter()
            .map(|m| m.model.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    /// Spend per day, at the window's own rate.
    ///
    /// Per *day* rather than projected to a month, which is what this first was:
    /// the default window is thirty days, so a monthly projection came out equal to
    /// the total and the card read `$1,627 … ≈ $1,627/mo`, which looks like a bug
    /// even though the arithmetic is right. A daily figure is never a restatement
    /// of the total and scales in the reader's head.
    ///
    /// `None` on a zero-day window — an empty scan reports exactly that, so this is
    /// a division by zero rather than a hypothetical.
    ///
    /// The rate divides by the *configured* window, not by the days that actually
    /// saw usage, so three days of history in a thirty-day window reads low. That
    /// is consistent with the card beside it saying `over 30 days`: it is the
    /// window's rate, not the busy days'.
    pub fn daily_rate(&self) -> Option<f64> {
        let days = self.scan.usage.window_days;
        if days == 0 {
            return None;
        }
        Some(self.total_usd() / days as f64)
    }

    /// The second half of the window against the first.
    ///
    /// Split on the **date** halfway between the first and last active day, never
    /// on the row index: days with no usage are absent from the series, so "the
    /// last N rows" would compare two unequal spans of time.
    ///
    /// `None` below [`TREND_MIN_DAYS`] active days, because a half-window delta
    /// over three days swings wildly enough to mislead, and `None` when the
    /// earlier half spent nothing — a ratio against zero is not a percentage.
    pub fn spend_trend(&self) -> Option<Trend> {
        use chrono::{Duration, NaiveDate};

        let days = &self.model_cost_days;
        if days.len() < TREND_MIN_DAYS {
            return None;
        }

        let parse = |row: &(String, BTreeMap<String, u64>, u64)| {
            NaiveDate::parse_from_str(&row.0, "%Y-%m-%d").ok()
        };
        let first = parse(days.first()?)?;
        let last = parse(days.last()?)?;
        let half = (last - first).num_days() / 2;
        if half < 1 {
            return None;
        }
        let split = first + Duration::days(half);

        let (mut earlier, mut later) = (0u128, 0u128);
        for row in days {
            let Some(day) = parse(row) else { continue };
            if day < split {
                earlier += u128::from(row.2);
            } else {
                later += u128::from(row.2);
            }
        }
        if earlier == 0 {
            return None;
        }

        Some(Trend {
            change: (later as f64 - earlier as f64) / earlier as f64,
            days: half as u64,
        })
    }

    /// Subscription versus what the same tokens cost at API rates.
    pub fn subscriptions(&self) -> Vec<SubscriptionRow> {
        let mut rows: Vec<SubscriptionRow> = self
            .spend_by_tool()
            .into_iter()
            .filter_map(|(tool, api_equivalent)| {
                // Plan is unknown to `surface` — it reads no account state — so
                // only a configured subscription produces a row.
                let (monthly, estimated) = self.cost_config.monthly(&tool, None)?;
                Some(SubscriptionRow {
                    tool,
                    monthly,
                    estimated,
                    api_equivalent,
                })
            })
            .collect();
        rows.sort_by(|a, b| b.saving().total_cmp(&a.saving()));
        rows
    }

    // ------------------------------------------------------------ navigation

    pub fn row_count(&self) -> usize {
        match self.tab {
            Tab::Overview => 0,
            Tab::Tools => self.tools.len(),
            Tab::Sites => self.site_count(),
            Tab::Usage => self.models.len(),
            // Nothing to select over when there are no prices; the footer should
            // not advertise a movement that does nothing.
            Tab::Cost if self.prices.is_empty() => 0,
            Tab::Cost => self.models.len(),
            Tab::Projects => self.repos.len(),
        }
    }

    #[cfg(feature = "sqlite")]
    fn site_count(&self) -> usize {
        self.scan.sites.sites.len()
    }

    #[cfg(not(feature = "sqlite"))]
    fn site_count(&self) -> usize {
        0
    }

    pub fn set_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            // Selection means something different per tab, so it does not carry.
            self.selected = 0;
            // Neither does the second pane's cursor, and focus least of all:
            // left on `Sessions`, the next view would take j/k for a pane it
            // does not draw.
            self.session_selected = 0;
            self.focus = Pane::Primary;
            // Each view charts something different, so the cursor means nothing
            // once the chart under it has changed.
            self.bucket_back = None;
        }
    }

    pub fn next_tab(&mut self) {
        let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.set_tab(Tab::ALL[(i + 1) % Tab::ALL.len()]);
    }

    pub fn prev_tab(&mut self) {
        let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.set_tab(Tab::ALL[(i + Tab::ALL.len() - 1) % Tab::ALL.len()]);
    }

    // ------------------------------------------------------ the second pane
    //
    // Projects draws two tables. `selected` is the project; `session_selected` is
    // the session, and it counts within *the selected project's* sessions — a
    // list that is rebuilt from `selected` on every frame. So moving the project
    // cursor replaces the second pane's contents entirely, and the session cursor
    // has to go back to 0 rather than be clamped: index 3 of the old repo's
    // sessions is a different session in the new repo's, and 0 is the meaningful
    // default because the list is sorted costliest-first.

    /// The pane j/k drives, ignoring focus that the current view cannot honour.
    ///
    /// Focus can be left on `Sessions` and then the project have none — from a
    /// rescan, or a project cursor that moved. Rather than police every path, the
    /// movers ask for the effective answer.
    pub fn focus(&self) -> Pane {
        if self.focus == Pane::Sessions && self.session_count() > 0 {
            Pane::Sessions
        } else {
            Pane::Primary
        }
    }

    /// How many sessions the second pane is showing.
    pub fn session_count(&self) -> usize {
        match self.tab {
            Tab::Projects => self
                .selected_project()
                .map_or(0, |project| self.sessions_in(&project.repo).len()),
            _ => 0,
        }
    }

    pub fn session_selected(&self) -> usize {
        self.session_selected
    }

    /// Move focus between the two panes. A no-op with nothing to move to, so the
    /// key never leaves j/k pointing at a pane the reader cannot see.
    pub fn toggle_focus(&mut self) {
        if self.session_count() == 0 {
            self.status_line = Some("no sessions to move into".to_string());
            return;
        }
        self.focus = match self.focus {
            Pane::Primary => Pane::Sessions,
            Pane::Sessions => Pane::Primary,
        };
        self.status_line = Some(
            match self.focus {
                Pane::Primary => "projects",
                Pane::Sessions => "sessions",
            }
            .to_string(),
        );
    }

    /// Select a session by index, for a click in the second pane.
    pub fn select_session_row(&mut self, index: usize) {
        if index < self.session_count() {
            self.session_selected = index;
            self.focus = Pane::Sessions;
        }
    }

    /// Move the project cursor, which is also what resets the session cursor.
    fn set_selected(&mut self, index: usize) {
        self.selected = index;
        self.session_selected = 0;
    }

    pub fn next_row(&mut self) {
        match self.focus() {
            Pane::Primary => {
                let n = self.row_count();
                if n > 0 {
                    self.set_selected((self.selected + 1) % n);
                }
            }
            Pane::Sessions => {
                let n = self.session_count();
                if n > 0 {
                    self.session_selected = (self.session_selected + 1) % n;
                }
            }
        }
    }

    pub fn prev_row(&mut self) {
        match self.focus() {
            Pane::Primary => {
                let n = self.row_count();
                if n > 0 {
                    self.set_selected((self.selected + n - 1) % n);
                }
            }
            Pane::Sessions => {
                let n = self.session_count();
                if n > 0 {
                    self.session_selected = (self.session_selected + n - 1) % n;
                }
            }
        }
    }

    /// Select a row by its absolute index, for a click that already knows which
    /// one it hit. A click on the empty space below a short table lands past the
    /// end and is ignored, rather than snapping to the last row.
    pub fn select_row(&mut self, index: usize) {
        if index < self.row_count() {
            self.set_selected(index);
            self.focus = Pane::Primary;
        }
    }

    pub fn first_row(&mut self) {
        match self.focus() {
            Pane::Primary => self.set_selected(0),
            Pane::Sessions => self.session_selected = 0,
        }
    }

    pub fn last_row(&mut self) {
        match self.focus() {
            Pane::Primary => {
                let last = self.row_count().saturating_sub(1);
                self.set_selected(last);
            }
            Pane::Sessions => self.session_selected = self.session_count().saturating_sub(1),
        }
    }

    pub fn scroll(&mut self, delta: i32) {
        self.scroll_pane(self.focus(), delta);
    }

    /// Scroll one pane by hand, for a wheel over a pane that is not focused.
    pub fn scroll_pane(&mut self, pane: Pane, delta: i32) {
        let (count, current) = match pane {
            Pane::Primary => (self.row_count(), self.selected),
            Pane::Sessions => (self.session_count(), self.session_selected),
        };
        if count == 0 {
            return;
        }
        let moved = (current as i32 + delta).clamp(0, count as i32 - 1) as usize;
        match pane {
            Pane::Primary => self.set_selected(moved),
            Pane::Sessions => self.session_selected = moved,
        }
    }

    pub fn toggle_unit(&mut self) {
        self.unit = match self.unit {
            Unit::Spend => Unit::Tokens,
            Unit::Tokens => Unit::Spend,
        };
        self.status_line = Some(
            match self.unit {
                Unit::Spend => "charting spend",
                Unit::Tokens => "charting tokens",
            }
            .to_string(),
        );
    }

    pub fn toggle_detail(&mut self) {
        self.detail = !self.detail;
        self.status_line = Some(
            if self.detail {
                "token detail on"
            } else {
                "token detail off"
            }
            .to_string(),
        );
    }

    pub fn cycle_granularity(&mut self) {
        self.granularity = self.granularity.next();
        // Regrouping rebuilds the buckets, so "three ago" now points at a
        // different span of days. Better to drop the cursor than to move it
        // somewhere the reader did not ask for.
        self.bucket_back = None;
        self.status_line = Some(format!("grouped by {}", self.granularity.label()));
    }

    // ------------------------------------------------------- the chart cursor

    pub fn bucket_back(&self) -> Option<usize> {
        self.bucket_back
    }

    /// How many buckets the chart on this view is drawing.
    ///
    /// Rebuckets on every call, which is what the draw path does per frame
    /// anyway; this runs once per keypress.
    fn bucket_count(&self) -> usize {
        match self.tab {
            // The same buckets the views actually draw. This used to name the
            // per-*tool* series, which happened to be the same length as the
            // per-model one — right answer, wrong reason, and wrong outright once
            // the Overview's unit could change under it.
            Tab::Overview => match self.unit {
                Unit::Spend => self.model_cost_buckets().len(),
                Unit::Tokens => self.model_token_buckets().len(),
            },
            Tab::Usage => self.model_token_buckets().len(),
            Tab::Cost => self.model_cost_buckets().len(),
            Tab::Projects => self
                .selected_project()
                .map_or(0, |project| self.project_buckets(&project.repo).len()),
            _ => 0,
        }
    }

    /// Move the chart cursor. Positive walks back in time.
    ///
    /// The first press lands on the newest bucket rather than jumping into the
    /// middle, so the cursor appears where the eye already is.
    pub fn move_bucket(&mut self, delta: i32) {
        let count = self.bucket_count();
        if count == 0 {
            return;
        }
        let back = match self.bucket_back {
            None => 0,
            Some(back) => (back as i32 + delta).clamp(0, count as i32 - 1) as usize,
        };
        self.bucket_back = Some(back);
    }

    /// Drop the cursor, so the chart titles go back to reporting the window.
    pub fn clear_bucket(&mut self) {
        if self.bucket_back.take().is_some() {
            self.status_line = Some("chart cursor cleared".to_string());
        }
    }
}

/// `BTreeMap<day, series>` -> sorted `(day, series, total)` rows.
fn flatten(by_day: BTreeMap<String, BTreeMap<String, u64>>) -> DailySeries {
    by_day
        .into_iter()
        .map(|(day, series)| {
            let total = series.values().sum();
            (day, series, total)
        })
        .collect()
}

/// Fold daily rows into chart buckets at the requested granularity.
fn bucket(days: &DailySeries, granularity: Granularity) -> Vec<Bucket> {
    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    for (day, series, _) in days {
        let key = granularity.bucket_of(day);
        let entry = buckets.entry(key.clone()).or_insert_with(|| Bucket {
            label: key,
            ..Bucket::default()
        });
        for (name, value) in series {
            *entry.series.entry(name.clone()).or_default() += value;
            entry.total += value;
        }
    }
    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_weeks_do_not_straddle_two_labels() {
        // 2026-07-27 is a Monday; the 28th is the Tuesday of the same ISO week.
        assert_eq!(iso_week("2026-07-27"), iso_week("2026-07-28"));
        assert_ne!(iso_week("2026-07-26"), iso_week("2026-07-27"));
    }

    #[test]
    fn an_unparseable_date_falls_back_to_itself_rather_than_vanishing() {
        assert_eq!(Granularity::Week.bucket_of("not-a-date"), "not-a-date");
        assert_eq!(Granularity::Month.bucket_of("2026-07-28"), "2026-07");
        assert_eq!(Granularity::Day.bucket_of("2026-07-28"), "2026-07-28");
    }

    #[test]
    fn granularity_cycles_back_to_day() {
        let g = Granularity::Day.next().next().next();
        assert_eq!(g, Granularity::Day);
    }

    #[test]
    fn bucketing_preserves_the_grand_total() {
        // The single most important property: regrouping must never change the
        // number the reader is looking at.
        let days = vec![
            (
                "2026-07-27".to_string(),
                BTreeMap::from([("claude_code".to_string(), 100u64)]),
                100u64,
            ),
            (
                "2026-07-28".to_string(),
                BTreeMap::from([("claude_code".to_string(), 50), ("codex".to_string(), 25)]),
                75,
            ),
        ];

        for granularity in [Granularity::Day, Granularity::Week, Granularity::Month] {
            let total: u64 = bucket(&days, granularity).iter().map(|b| b.total).sum();
            assert_eq!(total, 175, "{granularity:?} changed the total");
        }
    }

    #[test]
    fn weekly_bucketing_merges_days_of_one_week() {
        let days = vec![
            (
                "2026-07-27".to_string(),
                BTreeMap::from([("t".to_string(), 1u64)]),
                1u64,
            ),
            (
                "2026-07-28".to_string(),
                BTreeMap::from([("t".to_string(), 2)]),
                2,
            ),
        ];

        assert_eq!(bucket(&days, Granularity::Day).len(), 2);
        assert_eq!(bucket(&days, Granularity::Week).len(), 1);
        assert_eq!(bucket(&days, Granularity::Week)[0].total, 3);
    }

    #[test]
    fn a_saving_is_the_api_cost_minus_the_subscription() {
        let row = SubscriptionRow {
            tool: "claude_code".into(),
            monthly: 100.0,
            estimated: false,
            api_equivalent: 340.0,
        };
        assert_eq!(row.saving(), 240.0);

        // And negative when the subscription is the worse deal.
        let row = SubscriptionRow {
            api_equivalent: 12.0,
            ..row
        };
        assert_eq!(row.saving(), -88.0);
    }
}
