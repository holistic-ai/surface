//! Claude's 5-hour metering windows, from the usage history Claude Desktop
//! already keeps beside its config.
//!
//! Plans are metered in two units: how *deep* each 5-hour window runs, and how
//! *many* windows get started. The Usage view's token table answers neither —
//! tokens say nothing about where the plan's own meter stands. Claude Desktop
//! samples that meter every few minutes into `plan-usage-history.json`
//! (`{t, org, u: {fh, sd}}`: epoch millis, org id, and the 5-hour / 7-day
//! window utilisation as percentages), which is enough to reconstruct each
//! window and the peak it reached.
//!
//! # What is read, and what is not
//!
//! Timestamps and two percentages. The org id in each sample is not kept,
//! shown, or written anywhere — the file is parsed for `t` and `u.fh` only.
//! Absent file (no Claude Desktop, or another platform layout) scans to an
//! empty result and the view says so, rather than rendering a zero that would
//! read as "no usage".
//!
//! # Windows are reconstructed, not reported
//!
//! The file stores samples, not windows. A window is inferred to start when
//! utilisation rises from zero, resumes after a gap of at least the window
//! length, or falls sharply (a reset while the app kept sampling). Samples
//! only exist while Claude Desktop runs, so the count is a floor — windows
//! started while it was closed are invisible — which is the honest direction
//! for a "how close to the cap am I" signal to err.

use std::path::{Path, PathBuf};

/// The 5-hour window length, for gap detection, in milliseconds.
const WINDOW_MS: i64 = 5 * 60 * 60 * 1000;

/// A drop this many percentage points (while sampling stayed continuous) is a
/// meter reset, not noise: utilisation only decays with time, and a decay
/// this steep inside one sampling gap means a new window began.
const RESET_DROP: u64 = 30;

/// One reconstructed 5-hour window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterWindow {
    /// `YYYY-MM-DD` (UTC) the window started.
    pub day: String,
    /// Highest utilisation the window reached, in percent.
    pub peak: u64,
}

/// Every window the history file can testify to.
#[derive(Debug, Clone, Default)]
pub struct Metering {
    pub windows: Vec<MeterWindow>,
    /// First and last sample days, so the view can say what span the count
    /// covers rather than implying it covers the usage window.
    pub from: Option<String>,
    pub to: Option<String>,
    /// The history file was found and parsed. `false` renders as an honest
    /// absence, never as zero windows.
    pub found: bool,
}

impl Metering {
    /// Peak utilisation across every window, in percent.
    pub fn hottest(&self) -> u64 {
        self.windows.iter().map(|w| w.peak).max().unwrap_or(0)
    }

    /// Mean of the per-window peaks, in percent.
    pub fn average_peak(&self) -> u64 {
        if self.windows.is_empty() {
            return 0;
        }
        let sum: u64 = self.windows.iter().map(|w| w.peak).sum();
        (sum as f64 / self.windows.len() as f64).round() as u64
    }
}

/// Read the metering history this machine has, if any.
pub fn scan() -> Metering {
    let Some(home) = crate::paths::home() else {
        return Metering::default();
    };
    candidates(&home)
        .iter()
        .find(|p| p.is_file())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| parse(&raw))
        .unwrap_or_default()
}

/// Where Claude Desktop keeps the file, per platform. Checked in order.
fn candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("AppData/Roaming/Claude/plan-usage-history.json"),
        home.join("Library/Application Support/Claude/plan-usage-history.json"),
        home.join(".config/Claude/plan-usage-history.json"),
    ]
}

/// One sample: when, and how used the 5-hour window was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    /// Epoch milliseconds.
    t: i64,
    /// 5-hour window utilisation, percent.
    fh: u64,
}

/// Parse the history file into windows.
pub fn parse(raw: &str) -> Option<Metering> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let samples: Vec<Sample> = value
        .get("samples")?
        .as_array()?
        .iter()
        .filter_map(|s| {
            Some(Sample {
                t: s.get("t")?.as_i64()?,
                fh: s.get("u")?.get("fh")?.as_u64()?,
            })
        })
        .collect();
    if samples.is_empty() {
        return None;
    }

    Some(Metering {
        windows: windows_of(&samples),
        from: Some(day_of(samples.first()?.t)),
        to: Some(day_of(samples.last()?.t)),
        found: true,
    })
}

/// Reconstruct windows from the sample stream. See the module doc for the
/// three start conditions; the peak is simply the highest sample inside.
fn windows_of(samples: &[Sample]) -> Vec<MeterWindow> {
    let mut windows: Vec<MeterWindow> = Vec::new();
    let mut current: Option<MeterWindow> = None;
    let mut prev: Option<Sample> = None;

    for &sample in samples {
        let starts = match prev {
            None => sample.fh > 0,
            Some(prev) => {
                sample.fh > 0
                    && (prev.fh == 0
                        || sample.t - prev.t >= WINDOW_MS
                        || sample.fh + RESET_DROP < prev.fh)
            }
        };
        if starts {
            windows.extend(current.take());
            current = Some(MeterWindow {
                day: day_of(sample.t),
                peak: sample.fh,
            });
        } else if let Some(window) = &mut current {
            window.peak = window.peak.max(sample.fh);
        }
        prev = Some(sample);
    }
    windows.extend(current);
    windows
}

/// `YYYY-MM-DD` in UTC from epoch milliseconds.
fn day_of(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Minutes to epoch millis, from an arbitrary but fixed origin.
    fn at(minutes: i64) -> i64 {
        1_753_920_000_000 + minutes * 60_000
    }

    fn history(samples: &[(i64, u64)]) -> String {
        json!({
            "version": 2,
            "samples": samples.iter().map(|(t, fh)| json!({
                "t": t, "org": "not-kept", "u": {"fh": fh, "sd": 1}
            })).collect::<Vec<_>>()
        })
        .to_string()
    }

    #[test]
    fn a_rise_from_zero_starts_a_window_and_its_peak_is_kept() {
        let raw = history(&[
            (at(0), 0),
            (at(5), 12),
            (at(10), 40),
            (at(15), 33), // decay inside the same window
        ]);
        let m = parse(&raw).unwrap();
        assert!(m.found);
        assert_eq!(m.windows.len(), 1);
        assert_eq!(m.windows[0].peak, 40);
    }

    #[test]
    fn a_sharp_drop_is_a_reset_and_a_shallow_one_is_decay() {
        let raw = history(&[
            (at(0), 50),
            (at(5), 55),
            (at(10), 15), // 40-point drop: a new window
            (at(15), 35), // 20-point rise: same window
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.windows.len(), 2);
        assert_eq!(m.windows[0].peak, 55);
        assert_eq!(m.windows[1].peak, 35);
    }

    #[test]
    fn a_gap_of_a_window_length_starts_a_new_window() {
        let raw = history(&[
            (at(0), 20),
            (at(5), 25),
            (at(5 + 5 * 60), 10), // five hours later
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.windows.len(), 2);
    }

    #[test]
    fn idle_samples_start_nothing() {
        let raw = history(&[(at(0), 0), (at(5), 0), (at(10), 0)]);
        let m = parse(&raw).unwrap();
        assert!(m.windows.is_empty());
        assert_eq!(m.hottest(), 0);
    }

    #[test]
    fn summary_figures_average_the_peaks_not_the_samples() {
        let raw = history(&[
            (at(0), 10),
            (at(5), 60), // window 1 peaks at 60
            (at(10), 2), // a 58-point drop: reset, window 2
            (at(15), 40),
        ]);
        let m = parse(&raw).unwrap();
        assert_eq!(m.windows.len(), 2);
        assert_eq!(m.average_peak(), 50, "(60 + 40) / 2");
        assert_eq!(m.hottest(), 60);
    }

    #[test]
    fn an_absent_or_malformed_file_is_not_found_rather_than_zero() {
        assert!(parse("{not json").is_none());
        assert!(parse(r#"{"version": 2, "samples": []}"#).is_none());
        let m = Metering::default();
        assert!(!m.found, "absence must be distinguishable from idleness");
    }

    #[test]
    fn the_org_id_is_never_kept() {
        let raw = history(&[(at(0), 10)]);
        let m = parse(&raw).unwrap();
        let rendered = format!("{m:?}");
        assert!(
            !rendered.contains("not-kept"),
            "org id leaked into the result"
        );
    }
}
