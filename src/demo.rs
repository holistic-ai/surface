//! Mock data for `--demo`.
//!
//! Builds the same [`Scan`] the real thing produces, without touching the disk:
//! nothing here reads a transcript, a history database or a config file. It
//! exists so the dashboard can be shown — in a talk, a README recording, a bug
//! report about layout — on a machine that has no AI tooling on it, and so the
//! empty-state and warning paths can be looked at without having to break
//! something first.
//!
//! Three rules keep it from becoming a lie:
//!
//! 1. **It is labelled.** [`Scan::demo`] is set, the dashboard carries a `DEMO`
//!    marker in the sidebar, and `--json` reports `"demo": true`.
//! 2. **The names are real, the numbers are not.** Tools come from
//!    [`AI_TOOLS`](tooling::AI_TOOLS) and domains from
//!    [`AI_DOMAINS`](sites::AI_DOMAINS) by id, so the demo cannot drift into
//!    advertising a tool that does not exist. Only counts are invented.
//! 3. **Costs are not invented at all.** The token counts are fake, but they are
//!    priced by the real table, so a demo dollar figure is what those tokens
//!    would actually have cost. The model list deliberately includes one
//!    unpriced and one local model, because `unpriced` and `local` are states a
//!    viewer needs to see.
//!
//! Deterministic by construction — the generator is a seeded LCG and every date
//! is derived from today — so two runs on the same day produce the same
//! dashboard. That matters for screenshots and for reproducing a layout bug.

use chrono::{Datelike, Duration, Utc, Weekday};

use crate::ledger::{Ledger, Tokens};
use crate::scan::meter;
#[cfg(feature = "sqlite")]
use crate::scan::sites;
use crate::scan::{tooling, usage, Scan, Timings};

/// Days of history the demo covers. Matches the default usage window.
const WINDOW_DAYS: u64 = 30;

/// Fixed seed. Changing it reshapes every chart in the demo, so don't, unless
/// the shape is what you are unhappy with.
const SEED: u64 = 0x5EED_5CA1_AB1E_D00D;

/// A scan that was never run, and the timings it would plausibly have had.
pub fn scan() -> (Scan, Timings) {
    let tools = tools();
    let tools_summary = tooling::summarise(&tools);

    (
        Scan {
            tools,
            tools_summary,
            #[cfg(feature = "sqlite")]
            sites: sites(),
            usage: usage(),
            metering: metering(),
            failed: Vec::new(),
            demo: true,
        },
        Timings {
            tools_ms: 6,
            sites_ms: 331,
            usage_ms: 52,
            total_ms: 395,
        },
    )
}

/// A linear congruential generator — the numbers only have to look unplanned,
/// and this keeps the demo reproducible without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // Numerical Recipes' constants.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    /// Uniform in `low..=high`.
    fn range(&mut self, low: u64, high: u64) -> u64 {
        if high <= low {
            return low;
        }
        low + self.next() % (high - low + 1)
    }

    /// True with probability `1/n`.
    fn one_in(&mut self, n: u64) -> bool {
        self.range(1, n) == 1
    }
}

// ------------------------------------------------------------------- tools

/// Tools by catalogue id, with the evidence shape the real detector emits.
///
/// Ids that are not in the catalogue are skipped rather than faked, so this
/// list going stale costs a row and never invents a product.
fn tools() -> Vec<tooling::Detected> {
    const FOUND: &[(&str, &[&str])] = &[
        (
            "claude_code",
            &[
                "executable:/opt/homebrew/bin/claude",
                "config:~/.claude",
                "process:claude",
            ],
        ),
        ("claude_desktop", &["app:Claude"]),
        (
            "openai_codex",
            &["executable:/opt/homebrew/bin/codex", "config:~/.codex"],
        ),
        (
            "opencode",
            &[
                "executable:/Users/demo/.local/bin/opencode",
                "config:~/.local/share/opencode",
            ],
        ),
        ("cursor", &["app:Cursor", "config:~/.cursor"]),
        (
            "github_copilot",
            &["extension:github.copilot", "extension:github.copilot-chat"],
        ),
        ("gemini_cli", &["executable:/opt/homebrew/bin/gemini"]),
        (
            "ollama",
            &[
                "app:Ollama",
                "executable:/usr/local/bin/ollama",
                "process:ollama",
            ],
        ),
        ("aider", &["executable:/Users/demo/.local/bin/aider"]),
    ];

    FOUND
        .iter()
        .filter_map(|(id, evidence)| {
            let tool = tooling::AI_TOOLS.iter().find(|t| t.id == *id)?;
            Some(tooling::Detected {
                tool,
                evidence: evidence.iter().map(|e| e.to_string()).collect(),
            })
        })
        .collect()
}

// ------------------------------------------------------------------- sites

#[cfg(feature = "sqlite")]
fn sites() -> sites::Sites {
    // Visit counts only; the domains themselves come from the real table.
    const VISITED: &[(&str, u64, i64)] = &[
        ("chatgpt.com", 284, 1),
        ("claude.ai", 197, 1),
        ("gemini.google.com", 68, 2),
        ("huggingface.co", 41, 3),
        ("perplexity.ai", 33, 2),
        ("aistudio.google.com", 24, 5),
        ("v0.dev", 18, 8),
        ("cursor.com", 14, 4),
        ("grok.com", 9, 11),
        ("bolt.new", 6, 16),
        ("replit.com", 4, 21),
        ("elevenlabs.io", 1, 27),
    ];

    let now = Utc::now();
    let mut found = Vec::new();
    let mut vendors = Vec::new();

    for (domain, visits, days_ago) in VISITED {
        let Some(known) = sites::AI_DOMAINS.iter().find(|d| d.domain == *domain) else {
            continue;
        };
        let last_seen = now - Duration::days(*days_ago);
        found.push(sites::Site {
            domain: known.domain.to_string(),
            vendor: known.vendor,
            kind: Some(known.kind),
            visits: *visits,
            last_seen_unix: last_seen.timestamp(),
            last_seen: Some(last_seen.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        });
        if !vendors.contains(&known.vendor) {
            vendors.push(known.vendor);
        }
    }

    sites::Sites {
        sites: found,
        vendors,
        domains_watched: sites::AI_DOMAINS.len(),
        browsers_scanned: vec!["Chrome", "Firefox", "Arc"],
        profiles_scanned: 4,
        history_entries_scanned: 41_882,
        lookback_days: WINDOW_DAYS,
        // A coverage gap is part of an honest picture, so the demo has one.
        unreadable: vec![sites::Unreadable {
            browser: "Safari",
            profile: "default".to_string(),
            reason: "needs Full Disk Access".to_string(),
        }],
        blind_spots: Vec::new(),
        disabled: false,
    }
}

// ----------------------------------------------------------------- metering

/// Fourteen days of 5-hour windows: mostly shallow, two hot days, one brush
/// with the cap — the states a viewer needs to see, none of them invented
/// past what a real fortnight looks like.
fn metering() -> meter::Metering {
    let mut rng = Rng(SEED ^ 0x11E7E4);
    let today = Utc::now().date_naive();
    let mut windows = Vec::new();
    for days_ago in (0..14).rev() {
        let day = (today - Duration::days(days_ago)).format("%Y-%m-%d");
        for _ in 0..rng.range(1, 3) {
            let peak = match rng.range(1, 6) {
                1 => rng.range(45, 92), // a hot window
                _ => rng.range(2, 35),  // an ordinary one
            };
            windows.push(meter::MeterWindow {
                day: day.to_string(),
                peak,
            });
        }
    }
    meter::Metering {
        from: windows.first().map(|w| w.day.clone()),
        to: windows.last().map(|w| w.day.clone()),
        windows,
        found: true,
    }
}

// ------------------------------------------------------------------- usage

/// One tool's models, and the weight each carries in that tool's traffic.
///
/// The weights are relative, not percentages — they are normalised per day.
const TRAFFIC: &[(&str, &[(&str, u64)])] = &[
    (
        "claude_code",
        &[
            ("claude-sonnet-4-5", 58),
            ("claude-opus-4-5", 19),
            ("claude-haiku-4-5", 23),
        ],
    ),
    (
        "codex",
        &[
            ("gpt-5.1-codex", 61),
            ("gpt-5.1-codex-mini", 22),
            ("gpt-5-mini", 17),
        ],
    ),
    (
        "opencode",
        &[
            ("claude-sonnet-4-5", 41),
            ("gemini-2.5-flash", 24),
            ("gemini-2.5-pro", 13),
            ("deepseek-chat", 9),
            // Not in any price table: shows the `unpriced` state, and the "this
            // total is a floor" handling that hangs off it.
            ("acme-internal-eval-v3", 13),
        ],
    ),
    // Ollama tags price as `local`, which is a third state again: free, but
    // genuinely free rather than unknown.
    (
        "ollama",
        &[
            ("llama3.1:8b", 47),
            ("qwen2.5-coder:14b", 33),
            ("gemma3:12b", 20),
        ],
    ),
];

/// How a project's work is spread over the window.
enum Shape {
    /// Worked on most weekdays.
    Steady,
    /// Quiet, then a heavy day — a migration, a deploy week, an incident.
    Bursty,
    /// Evenings and weekends only. Somebody's own project.
    Spare,
    /// Touched every so often and briefly. A one-off fix, a scratch checkout.
    Occasional,
}

/// One repository, and the shape of the work that happened in it.
///
/// The variety here is the point: a real machine has a project that started
/// mid-window, one that stopped, one that is only ever touched at the weekend,
/// and a long tail of repositories with almost nothing in them. A demo where
/// every project is a scaled copy of every other shows none of the things the
/// Projects view is for.
struct Project {
    slug: &'static str,
    /// Relative share of a day's tokens while active.
    weight: u64,
    /// Days before today the work starts. `WINDOW_DAYS` means "from the start".
    starts: i64,
    /// Days before today the work stops. `0` means "still going".
    stops: i64,
    shape: Shape,
    /// Which tools are used in this repository. Different work, different tools.
    tools: &'static [&'static str],
}

const PROJECTS: &[Project] = &[
    // The main line of work: every tool, all window, heaviest share.
    Project {
        slug: "acme/platform",
        weight: 100,
        starts: 30,
        stops: 0,
        shape: Shape::Steady,
        tools: &["claude_code", "codex", "opencode"],
    },
    Project {
        slug: "acme/webapp",
        weight: 46,
        starts: 30,
        stops: 0,
        shape: Shape::Steady,
        tools: &["claude_code", "opencode"],
    },
    // Infrastructure work arrives in bursts, not in a trickle.
    Project {
        slug: "acme/infra",
        weight: 38,
        starts: 30,
        stops: 0,
        shape: Shape::Bursty,
        tools: &["claude_code", "codex"],
    },
    // Started mid-window: the Projects chart should show a project that ramps.
    Project {
        slug: "acme/billing-api",
        weight: 52,
        starts: 13,
        stops: 0,
        shape: Shape::Steady,
        tools: &["claude_code", "codex"],
    },
    // Stopped mid-window: shipped, or shelved. Its `last used` is not today.
    Project {
        slug: "acme/mobile-app",
        weight: 34,
        starts: 30,
        stops: 9,
        shape: Shape::Steady,
        tools: &["claude_code", "opencode"],
    },
    Project {
        slug: "acme/data-pipeline",
        weight: 29,
        starts: 30,
        stops: 0,
        shape: Shape::Bursty,
        tools: &["codex", "opencode"],
    },
    Project {
        slug: "acme/design-system",
        weight: 14,
        starts: 30,
        stops: 0,
        shape: Shape::Occasional,
        tools: &["claude_code"],
    },
    Project {
        slug: "acme/docs-site",
        weight: 11,
        starts: 30,
        stops: 0,
        shape: Shape::Occasional,
        tools: &["opencode"],
    },
    // Where the unpriced model runs — see `CONFINED`. Its total is a floor.
    Project {
        slug: "acme/ml-eval",
        weight: 22,
        starts: 22,
        stops: 0,
        shape: Shape::Bursty,
        tools: &["opencode"],
    },
    Project {
        slug: "acme/terraform",
        weight: 9,
        starts: 30,
        stops: 4,
        shape: Shape::Occasional,
        tools: &["codex"],
    },
    // Somebody's own work, at the weekend.
    Project {
        slug: "personal/notes",
        weight: 18,
        starts: 30,
        stops: 0,
        shape: Shape::Spare,
        tools: &["claude_code"],
    },
    Project {
        slug: "personal/dotfiles",
        weight: 6,
        starts: 30,
        stops: 0,
        shape: Shape::Spare,
        tools: &["claude_code", "ollama"],
    },
    // Local models only: real tokens, and a cost of exactly nothing.
    Project {
        slug: "personal/homelab",
        weight: 12,
        starts: 30,
        stops: 0,
        shape: Shape::Spare,
        tools: &["ollama"],
    },
    Project {
        slug: "oss/ratatui-fork",
        weight: 8,
        starts: 18,
        stops: 2,
        shape: Shape::Occasional,
        tools: &["claude_code"],
    },
    Project {
        slug: "scratch",
        weight: 5,
        starts: 30,
        stops: 0,
        shape: Shape::Occasional,
        tools: &["claude_code", "ollama"],
    },
    // Sessions whose working directory was not a git repository. Always present
    // in real data, so present here.
    Project {
        slug: crate::ledger::UNATTRIBUTED,
        weight: 21,
        starts: 30,
        stops: 0,
        shape: Shape::Steady,
        tools: &["claude_code", "codex", "opencode"],
    },
];

fn usage() -> usage::Usage {
    let mut rng = Rng(SEED);
    // The demo invents its titles, so it can afford to have them; a real scan
    // only collects them when the config says to.
    let mut ledger = Ledger {
        titles_enabled: true,
        ..Default::default()
    };
    let today = Utc::now().date_naive();

    for back in (0..WINDOW_DAYS as i64).rev() {
        let date = today - Duration::days(back);
        let day = date.format("%Y-%m-%d").to_string();

        // Weekends are quiet, and one day in nine is a push. Without this the
        // chart is a flat band and demonstrates nothing about the chart.
        //
        // The scale is a heavy user, or a machine several people share: output
        // tokens in the low millions per day, which the other counts multiply up
        // to the billions over the window. Chosen so the demo lands in the same
        // range as the dashboard in the README, and so the axis exercises the
        // `B` suffix rather than sitting in `M` where a real corpus would not.
        let weekday = !matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
        let base = match (weekday, rng.one_in(9)) {
            (true, true) => rng.range(6_600_000, 8_400_000),
            (true, false) => rng.range(1_670_000, 4_220_000),
            (false, _) => rng.range(88_000, 616_000),
        };

        // Project first, then tool, then model — the order the work actually
        // happens in. Generating tool-first and sprinkling projects underneath
        // gives every project the same tool mix and the same daily rhythm,
        // which is the one thing this view should not show.
        let weight_total: u64 = PROJECTS.iter().map(|p| p.weight).sum();

        for project in PROJECTS {
            if back > project.starts || back < project.stops {
                continue;
            }

            let factor = match project.shape {
                Shape::Steady if !weekday => 15,
                Shape::Steady => rng.range(75, 125),
                // Mostly nothing, occasionally everything.
                Shape::Bursty if rng.one_in(5) => rng.range(260, 420),
                Shape::Bursty => rng.range(4, 22),
                // Inverted: the weekend is when this one gets touched.
                Shape::Spare if weekday => rng.range(0, 12),
                Shape::Spare => rng.range(90, 190),
                Shape::Occasional if rng.one_in(4) => rng.range(40, 110),
                Shape::Occasional => 0,
            };
            if factor == 0 {
                continue;
            }

            let for_project = base * project.weight * factor / weight_total / 100;
            if for_project == 0 {
                continue;
            }

            for tool in project.tools {
                let Some((_, models)) = TRAFFIC.iter().find(|(t, _)| t == tool) else {
                    continue;
                };
                // Not every tool is reached for every day, even in a project
                // that uses it. The gap is what shows the chart handles gaps.
                if rng.one_in(4) {
                    continue;
                }

                let for_tool = for_project / project.tools.len() as u64;
                let model_total: u64 = models.iter().map(|(_, w)| w).sum();

                for (model, weight) in *models {
                    // A project only runs a model the tool would actually reach
                    // for there; `CONFINED` keeps the internal one in one place.
                    if !runs_here(model, project.slug) {
                        continue;
                    }
                    let output = for_tool * weight / model_total;
                    if output == 0 {
                        continue;
                    }
                    let tokens = tokens_for(&mut rng, output);
                    // Three halves of the same fact: the tool totals the Usage
                    // view reads, the repository attribution the Projects view
                    // reads, and the session breakdown under it. Same tokens, so
                    // all three views agree.
                    ledger.add(&day, tool, model, &tokens);
                    ledger.add_project(&day, project.slug, model, &tokens);

                    // One session per repository, tool and day — a day's work in
                    // one checkout, which is the shape a real transcript has.
                    let seed = format!("{}/{}/{}", project.slug, tool, day);
                    let session = crate::ledger::session_key(&seed);
                    ledger.add_session(&day, &session, model, &tokens);
                    ledger.observe_session(
                        &session,
                        tool,
                        project.slug,
                        Some(session_title(&seed)),
                    );
                }
            }
        }
    }

    // The plans the tools' own transcripts would name, so the SPEND card has
    // something to price the seats with.
    ledger.observe_plan("claude_code", "max_20x");
    ledger.observe_plan("codex", "team");

    usage::Usage {
        ledger,
        tools_read: vec!["claude_code", "codex", "opencode", "ollama"],
        sources_read: 148,
        bytes_read: 12_684,
        window_days: WINDOW_DAYS,
        unreadable: vec![usage::Unreadable {
            tool: "opencode",
            reason: "database is locked".to_string(),
        }],
        ledger_write_failed: false,
        disabled: false,
    }
}

/// A stable, invented title per session.
///
/// Derived from the seed rather than drawn from [`Rng`], because the demo is
/// deterministic by contract — see `the_same_day_produces_the_same_demo` — and an
/// extra draw here would shift every figure generated after it.
fn session_title(seed: &str) -> &'static str {
    const TITLES: [&str; 8] = [
        "flaky checkout test",
        "rate limiter rewrite",
        "migrate the auth middleware",
        "invoice rounding bug",
        "split the deploy pipeline",
        "cache keys collide under load",
        "port the CLI to the new config",
        "tidy the query planner",
    ];
    let sum: usize = seed.bytes().map(usize::from).sum();
    TITLES[sum % TITLES.len()]
}

/// Turn an output-token figure into a full [`Tokens`] with the proportions real
/// transcripts show — cache reads dominate, and input dwarfs output.
fn tokens_for(rng: &mut Rng, output: u64) -> Tokens {
    Tokens {
        input: output * rng.range(7, 12),
        output,
        // Prompt caching is where most of the token volume lives, and it is
        // priced differently, so a demo that omits it misprices itself.
        cache_read: output * rng.range(20, 40),
        cache_creation: output * rng.range(2, 5),
        reasoning: output * rng.range(0, 2),
        // A "message" is a whole assistant turn, tool calls included, so the
        // ratio of output tokens to messages is high — a few thousand to one.
        messages: (output / rng.range(3_000, 6_000)).max(1),
    }
}

/// Models that only ever run in one place.
///
/// Without this the unpriced model scatters across every project, so almost
/// every row in the Projects view renders as a `≥` floor and the distinction
/// between "this is the cost" and "this is a lower bound" stops being visible —
/// which is the one thing that view most needs to teach.
const CONFINED: &[(&str, &str)] = &[("acme-internal-eval-v3", "acme/ml-eval")];

/// Whether a model runs in a given repository.
///
/// Only [`CONFINED`] models are restricted; everything else runs wherever its
/// tool does.
fn runs_here(model: &str, project: &str) -> bool {
    match CONFINED.iter().find(|(m, _)| *m == model) {
        Some((_, only)) => *only == project,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_scan_fills_every_view() {
        let (scan, timings) = scan();

        assert!(scan.demo, "the dashboard needs this to label itself");
        assert!(scan.failed.is_empty());
        assert!(timings.total_ms > 0);

        assert!(scan.tools.len() >= 8, "Tools view: {}", scan.tools.len());
        assert!(scan.tools_summary.autonomous > 0, "the ▲ autonomous count");
        assert!(scan.tools_summary.vendors.len() > 3);

        let rows = scan.usage.ledger.rows();
        assert!(rows.len() > 200, "Usage view: {} rows", rows.len());
        assert!(!scan.usage.ledger.by_project().is_empty(), "Projects view");

        // The Projects view draws exact costs and floors differently, so the
        // demo has to contain both kinds of row.
        // `is_unpriced` and not `lookup(..).is_none()`: a local model is also
        // absent from the table, and the view counts it as free-and-known
        // rather than as a floor. Using the same predicate the view uses is the
        // difference between this test asserting the behaviour and asserting a
        // lookalike.
        let prices = crate::pricing::Prices::builtin();
        let (floors, exact): (Vec<_>, Vec<_>) = scan
            .usage
            .ledger
            .by_project()
            .into_iter()
            .partition(|(_, models)| {
                models
                    .iter()
                    .any(|(model, tokens)| prices.cost(model, tokens).is_unpriced())
            });
        assert!(!floors.is_empty(), "no project shows a ≥ floor");
        assert!(!exact.is_empty(), "every project is a floor: {floors:?}");
        assert!(!scan.usage.unreadable.is_empty(), "the ▲ unreadable state");

        #[cfg(feature = "sqlite")]
        {
            assert!(scan.sites.sites.len() >= 10, "Sites view");
            assert!(!scan.sites.unreadable.is_empty(), "the ▲ browser state");
        }
    }

    #[test]
    fn every_tool_and_domain_named_is_one_that_exists() {
        // The demo must not become a place where a tool nobody ships appears to
        // be detected.
        let (scan, _) = scan();
        for detected in &scan.tools {
            assert!(
                tooling::AI_TOOLS.iter().any(|t| t.id == detected.tool.id),
                "{} is not in the catalogue",
                detected.tool.id
            );
        }
        #[cfg(feature = "sqlite")]
        for site in &scan.sites.sites {
            assert!(
                sites::AI_DOMAINS.iter().any(|d| d.domain == site.domain),
                "{} is not in the domain table",
                site.domain
            );
        }
    }

    #[test]
    fn the_same_day_produces_the_same_demo() {
        // Screenshots and layout bug reports both depend on this.
        let (first, _) = scan();
        let (second, _) = scan();
        assert_eq!(first.usage.ledger.rows(), second.usage.ledger.rows());
    }

    #[test]
    fn the_window_ends_today_and_reaches_back_thirty_days() {
        let (scan, _) = scan();
        let rows = scan.usage.ledger.rows();

        let days: std::collections::BTreeSet<_> =
            rows.iter().map(|(day, ..)| day.clone()).collect();
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        let oldest = (Utc::now().date_naive() - Duration::days(WINDOW_DAYS as i64 - 1))
            .format("%Y-%m-%d")
            .to_string();

        assert!(days.contains(&today), "the chart should reach today");
        assert_eq!(days.first().map(String::as_str), Some(oldest.as_str()));
        assert!(days.len() >= 28, "{} days covered", days.len());
    }

    #[test]
    fn the_model_list_covers_priced_unpriced_and_local() {
        // Each of the three cost states is a distinct code path in the views;
        // the demo exists partly to show all three at once.
        let models: Vec<&str> = TRAFFIC
            .iter()
            .flat_map(|(_, models)| models.iter().map(|(m, _)| *m))
            .collect();

        let prices = crate::pricing::Prices::builtin();
        assert!(
            models.iter().any(|m| prices.lookup(m).is_some()),
            "no priced model"
        );
        assert!(
            models
                .iter()
                .any(|m| prices.lookup(m).is_none() && !crate::pricing::is_local_model(m)),
            "no unpriced model"
        );
        assert!(
            models.iter().any(|m| crate::pricing::is_local_model(m)),
            "no local model"
        );
    }
}
