//! Configuration: an optional TOML file, overridden by `SURFACE_*` environment
//! variables, overridden by CLI flags.
//!
//! A missing config file is the normal case, not an error — the defaults are
//! what most people want, and `surface` should be useful with nothing but the
//! binary.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// How far back a domain must have been visited to be reported.
pub const DEFAULT_HISTORY_LOOKBACK_DAYS: u64 = 30;
/// Default token-accounting window, in days.
pub const DEFAULT_USAGE_WINDOW_DAYS: u64 = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub web: WebConfig,
    pub usage: UsageConfig,
    pub cost: CostConfig,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut config = if path.exists() {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            toml::from_str(&raw).with_context(|| format!("parsing config {}", path.display()))?
        } else {
            Config::default()
        };

        config.apply_env();
        config.validate();
        Ok(config)
    }

    fn apply_env(&mut self) {
        if let Some(v) = env_parse("SURFACE_HISTORY_LOOKBACK_DAYS") {
            self.web.history_lookback_days = v;
        }
        if let Some(v) = env_bool("SURFACE_SCAN_HISTORY") {
            self.web.scan_history = v;
        }
        if let Some(v) = env_parse("SURFACE_USAGE_WINDOW_DAYS") {
            self.usage.window_days = v;
        }
        if let Some(v) = env_bool("SURFACE_SCAN_USAGE") {
            self.usage.scan = v;
        }
    }

    /// Clamp rather than reject. A nonsensical window is a typo, and refusing to
    /// run over a typo is worse than running over a sane value.
    fn validate(&mut self) {
        self.web.history_lookback_days = self.web.history_lookback_days.clamp(1, 3_650);
        self.usage.window_days = self.usage.window_days.clamp(1, 3_650);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebConfig {
    /// Read browser history at all. Set `false` and the sites scan reports
    /// `disabled` rather than silently returning nothing.
    pub scan_history: bool,
    pub history_lookback_days: u64,
    /// Additional AI domains beyond the built-in table, e.g. an internal model
    /// gateway.
    pub extra_ai_domains: Vec<String>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            scan_history: true,
            history_lookback_days: DEFAULT_HISTORY_LOOKBACK_DAYS,
            extra_ai_domains: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageConfig {
    /// Read local AI transcripts to account for token usage. Set `false` and
    /// the usage scan reports `disabled`.
    pub scan: bool,
    /// How many days of daily totals to retain and report.
    pub window_days: u64,
    /// Project rows to fold into another, `shown name -> fold into`.
    ///
    /// The same project legitimately earns two names: a checkout with an
    /// `origin` remote reports `owner/name`, a copy of the same code with no
    /// remote reports its folder basename. surface never guesses that two
    /// names are one project — that would misattribute someone's spend on a
    /// string resemblance — so the operator declares it here instead.
    pub repo_aliases: BTreeMap<String, String>,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            scan: true,
            window_days: DEFAULT_USAGE_WINDOW_DAYS,
            repo_aliases: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CostConfig {
    /// Monthly subscription spend per tool, in USD.
    ///
    /// Overrides the built-in list-price defaults, which are estimates and go
    /// stale. Set what you actually pay and the comparison becomes real.
    pub subscriptions: BTreeMap<String, f64>,
}

impl CostConfig {
    /// Monthly cost for a tool on `plan`, and whether that figure is an
    /// estimate.
    ///
    /// Config wins. The fallbacks are published list prices at the time of
    /// writing and are labelled as estimates wherever they are shown — an
    /// unknown plan returns `None` rather than a guess.
    pub fn monthly(&self, tool: &str, plan: Option<&str>) -> Option<(f64, bool)> {
        if let Some(configured) = self.subscriptions.get(tool) {
            return Some((*configured, false));
        }
        let plan = plan?.to_lowercase();
        let listed = match plan.as_str() {
            "pro" | "plus" => 20.0,
            "max_5x" | "default_claude_max_5x" => 100.0,
            "max_20x" => 200.0,
            "team" | "team_tier_1" => 30.0,
            // Enterprise and API-key access have no list price to assume.
            _ => return None,
        };
        Some((listed, true))
    }
}

fn env_parse<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.trim().parse().ok()
}

fn env_bool(key: &str) -> Option<bool> {
    let raw = std::env::var(key).ok()?;
    match raw.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_defaults_not_an_error() {
        let config = Config::load(Path::new("/nonexistent/surface.toml")).unwrap();
        assert!(config.web.scan_history);
        assert!(config.usage.scan);
        assert_eq!(config.usage.window_days, DEFAULT_USAGE_WINDOW_DAYS);
        assert!(config.cost.subscriptions.is_empty());
    }

    #[test]
    fn partial_tables_keep_the_other_defaults() {
        let dir = std::env::temp_dir().join("surface-config-partial");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("surface.toml");
        std::fs::write(&path, "[usage]\nwindow_days = 7\n").unwrap();

        let config = Config::load(&path).unwrap();

        assert_eq!(config.usage.window_days, 7);
        assert!(config.usage.scan, "untouched field keeps its default");
        assert!(config.web.scan_history, "untouched table keeps its default");
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        // A typo that silently does nothing is worse than a startup error.
        let dir = std::env::temp_dir().join("surface-config-unknown");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("surface.toml");
        std::fs::write(&path, "[usage]\nwindo_days = 7\n").unwrap();

        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn a_nonsense_window_is_clamped_not_rejected() {
        let mut config = Config {
            usage: UsageConfig {
                scan: true,
                window_days: 0,
                ..Default::default()
            },
            ..Config::default()
        };
        config.validate();
        assert_eq!(config.usage.window_days, 1);
    }

    #[test]
    fn configured_subscription_beats_the_list_price_and_is_not_an_estimate() {
        let mut cost = CostConfig::default();
        cost.subscriptions.insert("claude_code".into(), 150.0);

        assert_eq!(
            cost.monthly("claude_code", Some("pro")),
            Some((150.0, false))
        );
    }

    #[test]
    fn a_known_plan_falls_back_to_a_flagged_list_price() {
        let cost = CostConfig::default();
        assert_eq!(
            cost.monthly("claude_code", Some("max_20x")),
            Some((200.0, true))
        );
    }

    #[test]
    fn an_unknown_plan_is_none_rather_than_a_guess() {
        let cost = CostConfig::default();
        assert_eq!(cost.monthly("claude_code", Some("enterprise")), None);
        assert_eq!(cost.monthly("claude_code", None), None);
    }
}
