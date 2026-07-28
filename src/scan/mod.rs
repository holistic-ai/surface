//! The scan: four sections, run in order, on this machine only.
//!
//! Everything is synchronous. surface ran its collectors concurrently under a
//! tokio semaphore with per-collector timeouts, which bought real throughput
//! across 21 collectors on a fleet. Three sections that take milliseconds each
//! (bar a cold transcript read) do not justify carrying a runtime, so this is a
//! plain loop.
//!
//! What is kept from that design is failure isolation. Each section runs inside
//! [`std::panic::catch_unwind`], so a parser that trips over a shape nobody
//! anticipated costs that one section and not the whole scan. This is why the
//! release profile deliberately does not set `panic = "abort"`.

pub mod apps;
#[cfg(feature = "sqlite")]
pub mod sites;
pub mod tooling;
pub mod usage;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::config::Config;

/// Everything one scan found.
pub struct Scan {
    pub tools: Vec<tooling::Detected>,
    pub tools_summary: tooling::Summary,
    #[cfg(feature = "sqlite")]
    pub sites: sites::Sites,
    pub usage: usage::Usage,
    /// Sections that panicked, by name. Empty is the normal case.
    pub failed: Vec<&'static str>,
    /// Built by [`crate::demo`] rather than read off this machine. Never true
    /// for a real scan, and every surface that shows the data says so.
    pub demo: bool,
}

/// How long each section took, for the footer and `--json`.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Timings {
    pub tools_ms: u128,
    pub sites_ms: u128,
    pub usage_ms: u128,
    pub total_ms: u128,
}

impl Scan {
    /// Whether the browser scan is compiled in at all.
    pub const fn sites_compiled_in() -> bool {
        cfg!(feature = "sqlite")
    }
}

/// Run every section. Never returns an error: a section that cannot answer
/// reports that it could not, and the rest of the scan still runs.
pub fn run(config: &Config, state_dir: &Path) -> (Scan, Timings) {
    let started = Instant::now();
    let mut failed = Vec::new();
    let mut timings = Timings::default();

    // The one piece of shared system state, gathered once.
    let processes = process_names();

    let mark = Instant::now();
    let tools = section("tools", &mut failed, || tooling::scan(processes)).unwrap_or_default();
    timings.tools_ms = mark.elapsed().as_millis();
    let tools_summary = tooling::summarise(&tools);

    #[cfg(feature = "sqlite")]
    let sites = {
        let mark = Instant::now();
        let found = section("sites", &mut failed, || sites::scan(&config.web)).unwrap_or_default();
        timings.sites_ms = mark.elapsed().as_millis();
        found
    };

    let mark = Instant::now();
    let usage = section("usage", &mut failed, || {
        usage::scan(&config.usage, state_dir)
    })
    .unwrap_or_default();
    timings.usage_ms = mark.elapsed().as_millis();

    timings.total_ms = started.elapsed().as_millis();

    (
        Scan {
            tools,
            tools_summary,
            #[cfg(feature = "sqlite")]
            sites,
            usage,
            failed,
            demo: false,
        },
        timings,
    )
}

/// Run one section, converting a panic into a recorded failure.
///
/// The payload is deliberately not printed: it would land in the middle of
/// terminal setup and is of no use to someone who just wants their token counts.
/// The section is named in the footer instead.
fn section<T>(
    name: &'static str,
    failed: &mut Vec<&'static str>,
    body: impl FnOnce() -> T,
) -> Option<T> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(body));
    std::panic::set_hook(hook);

    match result {
        Ok(value) => Some(value),
        Err(_) => {
            failed.push(name);
            None
        }
    }
}

/// Running process names, for the one detection channel that needs them.
fn process_names() -> Vec<String> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};

    // Names only. Refreshing CPU or memory per process costs hundreds of
    // milliseconds on a busy machine and nothing here reads them.
    let system =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    system
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_section_is_recorded_and_does_not_abort_the_scan() {
        let mut failed = Vec::new();

        let ok = section("first", &mut failed, || 41 + 1);
        let boom = section::<u32>("second", &mut failed, || panic!("parser tripped"));
        let after = section("third", &mut failed, || "still running");

        assert_eq!(ok, Some(42));
        assert_eq!(boom, None);
        assert_eq!(after, Some("still running"), "the scan continued");
        assert_eq!(failed, ["second"]);
    }

    #[test]
    fn the_panic_hook_is_restored_afterwards() {
        // Leaving the silencing hook installed would swallow every later panic,
        // including the ones a test run needs to see.
        let mut failed = Vec::new();
        section::<()>("boom", &mut failed, || panic!("x"));

        let survived = catch_unwind(AssertUnwindSafe(|| {
            let mut inner: Vec<&'static str> = Vec::new();
            section::<()>("again", &mut inner, || panic!("y"));
            inner
        }));
        assert_eq!(survived.unwrap(), ["again"]);
    }

    #[test]
    fn a_scan_with_everything_disabled_still_returns() {
        let config = Config {
            web: crate::config::WebConfig {
                scan_history: false,
                ..Default::default()
            },
            usage: crate::config::UsageConfig {
                scan: false,
                ..Default::default()
            },
            ..Config::default()
        };

        let (scan, timings) = run(&config, &std::env::temp_dir().join("surface-scan-none"));

        assert!(scan.failed.is_empty());
        assert!(scan.usage.disabled);
        #[cfg(feature = "sqlite")]
        assert!(scan.sites.disabled);
        // Tooling has no off switch — it is the cheapest section and the one the
        // tool exists for.
        assert!(timings.total_ms < 60_000);
    }

    #[test]
    fn process_names_are_never_blank() {
        // Not asserting a count: a container can legitimately have very few.
        assert!(process_names().iter().all(|n| !n.is_empty()));
    }
}
