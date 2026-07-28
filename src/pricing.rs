//! Model prices, from LiteLLM's public table.
//!
//! # Why pricing happens here and not during the scan
//!
//! The scan records token counts and model ids. A price baked in at count time
//! is wrong the moment rates move, and there is no way to correct it after the
//! fact. Applying prices at read time means a newer table re-prices the whole
//! history for free.
//!
//! # Three sources, in order
//!
//! 1. A cached table in the state directory, if it is less than [`CACHE_TTL`] old.
//! 2. The built-in table (`prices_fallback.json`), so a first run with no network
//!    still costs correctly rather than showing an empty Cost view.
//! 3. The network, when the cache is stale and `--offline` was not passed.
//!
//! A stale cache beats no prices, and the built-in table beats both when neither
//! is available.
//!
//! # Unpriced is not free
//!
//! A local Ollama model genuinely costs nothing; a model missing from the table
//! costs an unknown amount. Both would compute to `$0.00`, so they are counted
//! separately and reported — a total that silently omits a third of the usage is
//! worse than no total.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub const SOURCE_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// How long a cached table is considered current.
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Guard against a redirect to something enormous.
const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024;

/// USD per single token, by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PerToken {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

/// What a model's usage cost, and whether we could price it at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cost {
    /// Priced from the table.
    Known(f64),
    /// Runs locally, so there is no per-token charge.
    Local,
    /// Not in the table — the cost is unknown, which is not the same as zero.
    Unpriced,
}

impl Cost {
    pub fn usd(&self) -> f64 {
        match self {
            Cost::Known(v) => *v,
            Cost::Local | Cost::Unpriced => 0.0,
        }
    }

    pub fn is_unpriced(&self) -> bool {
        matches!(self, Cost::Unpriced)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Prices {
    models: BTreeMap<String, PerToken>,
    /// When the underlying cache file was written.
    pub fetched_at: Option<SystemTime>,
    /// These came from the compiled-in table, so they are as old as the release.
    builtin: bool,
}

impl Prices {
    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Price a model.
    ///
    /// Exact match first, then a suffix match on the last path segment, which
    /// is how a provider-qualified key like `cloudflare/@cf/zai-org/glm-5.2`
    /// gets found from the bare name a tool records.
    pub fn lookup(&self, model: &str) -> Option<PerToken> {
        if let Some(found) = self.models.get(model) {
            return Some(*found);
        }
        let wanted = normalise(model);
        self.models
            .iter()
            .find(|(key, _)| normalise(key.rsplit('/').next().unwrap_or(key)) == wanted)
            .map(|(_, price)| *price)
    }

    /// Cost of one model's tokens.
    pub fn cost(&self, model: &str, tokens: &crate::ledger::Tokens) -> Cost {
        if is_local_model(model) {
            return Cost::Local;
        }
        let Some(price) = self.lookup(model) else {
            return Cost::Unpriced;
        };

        Cost::Known(
            tokens.input as f64 * price.input
                + tokens.output as f64 * price.output
                + tokens.cache_read as f64 * price.cache_read
                + tokens.cache_creation as f64 * price.cache_creation
                // Reasoning tokens are billed as output everywhere that
                // distinguishes them.
                + tokens.reasoning as f64 * price.output,
        )
    }

    /// Parse LiteLLM's table.
    pub fn parse(json: &str) -> Option<Self> {
        let raw: BTreeMap<String, serde_json::Value> = serde_json::from_str(json).ok()?;

        let mut models = BTreeMap::new();
        for (name, entry) in raw {
            // A metadata header shares the namespace with real models.
            if name == "sample_spec" {
                continue;
            }
            let num = |key: &str| entry.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let price = PerToken {
                input: num("input_cost_per_token"),
                output: num("output_cost_per_token"),
                cache_read: num("cache_read_input_token_cost"),
                cache_creation: num("cache_creation_input_token_cost"),
            };
            // Entries with no cost at all are embeddings, moderation and the
            // like; keeping them would let a suffix match find a zero price.
            if price.input == 0.0 && price.output == 0.0 {
                continue;
            }
            models.insert(name, price);
        }

        (!models.is_empty()).then_some(Self {
            models,
            fetched_at: None,
            builtin: false,
        })
    }

    /// A trimmed LiteLLM table compiled into the binary.
    ///
    /// The families `surface` can detect, and only the four fields it reads —
    /// about 46 KB. It exists so a first run with no network still shows real
    /// costs instead of a Cost view full of "unpriced", which reads like a bug.
    ///
    /// It goes stale, which is exactly why it is the last resort rather than the
    /// first: a fresh cache always wins.
    pub fn builtin() -> Self {
        let mut prices = Prices::parse(include_str!("prices_fallback.json")).unwrap_or_default();
        prices.builtin = true;
        prices
    }

    /// Load prices: fresh cache, else built-in, else refreshed from the network.
    ///
    /// A stale cache beats no prices, so a failed refresh falls back to whatever
    /// is on disk rather than erroring.
    pub fn load(cache_dir: &Path, allow_network: bool) -> Self {
        let path = cache_path(cache_dir);
        let age = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok());

        let fresh = age.is_some_and(|age| age < CACHE_TTL);

        if !fresh && allow_network {
            if let Some(body) = download() {
                if Prices::parse(&body).is_some() {
                    let _ = std::fs::create_dir_all(cache_dir);
                    let _ = std::fs::write(&path, &body);
                }
            }
        }

        match std::fs::read_to_string(&path).ok().and_then(|body| {
            let mut prices = Prices::parse(&body)?;
            prices.fetched_at = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            Some(prices)
        }) {
            Some(prices) => prices,
            // No cache and no network. The built-in table is smaller than the
            // real one, so some models will still be unpriced — which is
            // reported, not hidden.
            None => Prices::builtin(),
        }
    }

    /// Age of the loaded table. `None` for the built-in one, which has no date
    /// worth quoting.
    pub fn age(&self) -> Option<Duration> {
        self.fetched_at.and_then(|t| t.elapsed().ok())
    }

    /// Whether these prices came from the compiled-in table rather than a fetch.
    pub fn is_builtin(&self) -> bool {
        self.builtin
    }
}

pub fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("litellm-prices.json")
}

/// Fetch with `curl`, which ships on macOS, Windows 10+ and every Linux we
/// target — cheaper than linking a TLS stack into a dev tool.
fn download() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--fail",
            "--location",
            "--max-time",
            "45",
            "--max-filesize",
            &MAX_DOWNLOAD_BYTES.to_string(),
            SOURCE_URL,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Models that run on this machine, where there is no per-token charge.
pub fn is_local_model(model: &str) -> bool {
    const LOCAL: &[&str] = &[
        "gemma",
        "llama",
        "mistral-7b",
        "qwen",
        "phi",
        "deepseek-r1:",
    ];
    let lower = model.to_lowercase();
    // An Ollama tag (`gemma4:latest`) is a strong local signal on its own.
    lower.contains(':') && !lower.starts_with("http") || LOCAL.iter().any(|l| lower.starts_with(l))
}

/// Compare model names ignoring punctuation that providers vary.
fn normalise(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Format USD for a dense table.
pub fn format_usd(value: f64) -> String {
    if value >= 1000.0 {
        // Grouped, because four and five figure sums are the ones a reader
        // most needs to tell apart at a glance and `$8757` does not scan.
        format!("${}", crate::format::thousands(value.round() as u64))
    } else if value >= 1.0 {
        format!("${value:.2}")
    } else if value > 0.0 {
        format!("${value:.4}")
    } else {
        "$0".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Tokens;

    /// A cut-down table in LiteLLM's real shape.
    fn table() -> Prices {
        Prices::parse(
            r#"{
              "sample_spec": {"input_cost_per_token": 0.0, "notes": "not a model"},
              "claude-opus-4-8": {
                "input_cost_per_token": 5e-06,
                "output_cost_per_token": 2.5e-05,
                "cache_read_input_token_cost": 5e-07,
                "cache_creation_input_token_cost": 6.25e-06,
                "litellm_provider": "anthropic"
              },
              "cloudflare/@cf/zai-org/glm-5.2": {
                "input_cost_per_token": 1e-06,
                "output_cost_per_token": 2e-06
              },
              "text-embedding-3-small": {
                "input_cost_per_token": 0.0,
                "output_cost_per_token": 0.0
              }
            }"#,
        )
        .unwrap()
    }

    fn tokens(input: u64, output: u64, cache_read: u64) -> Tokens {
        Tokens {
            input,
            output,
            cache_read,
            ..Default::default()
        }
    }

    #[test]
    fn parses_the_real_field_names() {
        let prices = table();
        let opus = prices.lookup("claude-opus-4-8").unwrap();
        assert_eq!(opus.input, 5e-06);
        assert_eq!(opus.output, 2.5e-05);
        assert_eq!(opus.cache_read, 5e-07);
        assert_eq!(opus.cache_creation, 6.25e-06);
    }

    #[test]
    fn the_spec_header_and_zero_cost_entries_are_not_models() {
        let prices = table();
        assert!(prices.lookup("sample_spec").is_none());
        // Otherwise a suffix match could find a free price for a real model.
        assert!(prices.lookup("text-embedding-3-small").is_none());
        assert_eq!(prices.len(), 2);
    }

    #[test]
    fn a_bare_name_finds_a_provider_qualified_entry() {
        // Tools record `glm-5.2`; LiteLLM keys it under a cloudflare path.
        assert!(table().lookup("glm-5.2").is_some());
    }

    #[test]
    fn cost_sums_every_token_kind() {
        let prices = table();
        // 1M input, 1M output, 1M cache read.
        let cost = prices.cost("claude-opus-4-8", &tokens(1_000_000, 1_000_000, 1_000_000));
        // 5 + 25 + 0.5
        assert_eq!(cost, Cost::Known(30.5));
    }

    #[test]
    fn reasoning_tokens_are_billed_as_output() {
        let prices = table();
        let with_reasoning = Tokens {
            reasoning: 1_000_000,
            ..Default::default()
        };
        assert_eq!(
            prices.cost("claude-opus-4-8", &with_reasoning),
            Cost::Known(25.0)
        );
    }

    #[test]
    fn a_local_model_is_free_not_unpriced() {
        // The distinction matters: one is genuinely zero, the other is unknown.
        let prices = table();
        assert_eq!(
            prices.cost("gemma4:latest", &tokens(1_000_000, 0, 0)),
            Cost::Local
        );
        assert_eq!(prices.cost("llama3", &tokens(1_000_000, 0, 0)), Cost::Local);
    }

    #[test]
    fn an_unknown_remote_model_is_unpriced_not_free() {
        let prices = table();
        let cost = prices.cost("kimi-k3", &tokens(1_000_000, 0, 0));
        assert_eq!(cost, Cost::Unpriced);
        assert!(cost.is_unpriced(), "must not be mistaken for zero cost");
        assert_eq!(cost.usd(), 0.0);
    }

    #[test]
    fn local_model_detection_does_not_swallow_hosted_models() {
        assert!(is_local_model("gemma4:latest"));
        assert!(is_local_model("qwen2.5-coder:7b"));
        assert!(!is_local_model("claude-opus-4-8"));
        assert!(!is_local_model("gpt-5.6-sol"));
        assert!(!is_local_model("kimi-k3"));
    }

    #[test]
    fn malformed_or_empty_tables_yield_nothing() {
        assert!(Prices::parse("{not json").is_none());
        assert!(Prices::parse("{}").is_none());
        assert!(Prices::default().is_empty());
        // An empty table prices nothing rather than pricing everything at zero.
        assert_eq!(
            Prices::default().cost("claude-opus-4-8", &tokens(1, 1, 1)),
            Cost::Unpriced
        );
    }

    #[test]
    fn money_formats_readably_across_magnitudes() {
        assert_eq!(format_usd(0.0), "$0");
        assert_eq!(format_usd(0.0031), "$0.0031");
        assert_eq!(format_usd(4.5), "$4.50");
        assert_eq!(format_usd(999.5), "$999.50");
        // Grouped past a thousand: `$8757` does not scan, and four- and
        // five-figure sums are the ones a reader most needs to tell apart.
        // Rounding is half-away-from-zero rather than the half-to-even the
        // old `{:.0}` gave, which is why 1234.5 now reads 1,235.
        assert_eq!(format_usd(1000.0), "$1,000");
        assert_eq!(format_usd(1234.5), "$1,235");
        assert_eq!(format_usd(8757.4), "$8,757");
    }

    #[test]
    fn a_missing_cache_falls_back_to_the_builtin_table() {
        let dir = std::env::temp_dir().join("surface-pricing-missing");
        let _ = std::fs::remove_dir_all(&dir);

        let prices = Prices::load(&dir, false);

        assert!(
            !prices.is_empty(),
            "a missing cache must not mean no prices"
        );
        assert!(prices.is_builtin());
    }

    #[test]
    fn a_cached_table_is_read_without_the_network() {
        let dir = std::env::temp_dir().join("surface-pricing-cache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            cache_path(&dir),
            r#"{"claude-opus-4-8":{"input_cost_per_token":5e-06,"output_cost_per_token":2.5e-05}}"#,
        )
        .unwrap();

        let prices = Prices::load(&dir, false);
        assert_eq!(prices.len(), 1);
        assert!(prices.fetched_at.is_some());
    }

    #[test]
    fn a_corrupt_cache_does_not_poison_the_view() {
        let dir = std::env::temp_dir().join("surface-pricing-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(cache_path(&dir), b"{garbage").unwrap();

        // Unparseable is treated as absent, so it falls through to the built-in
        // table rather than leaving the Cost view empty.
        let prices = Prices::load(&dir, false);
        assert!(prices.is_builtin());
        assert!(!prices.is_empty());
    }

    #[test]
    fn the_builtin_table_parses_and_prices_the_common_models() {
        let prices = Prices::builtin();

        assert!(prices.is_builtin());
        assert!(prices.len() > 100, "only {} models", prices.len());
        assert!(
            prices.age().is_none(),
            "the built-in table has no fetch date"
        );

        // The families surface detects must actually be priceable, or the
        // fallback is decoration.
        for model in [
            "claude-opus-4-8",
            "claude-haiku-4-5-20251001",
            "gpt-5.6-sol",
        ] {
            assert!(
                prices.lookup(model).is_some(),
                "{model} is not in the built-in table"
            );
        }
    }

    #[test]
    fn the_builtin_table_never_prices_anything_at_zero() {
        // A zero price would read as free. Entries with no cost are dropped at
        // parse time; this asserts the trimmed table has none to begin with.
        let prices = Prices::builtin();
        for (name, price) in &prices.models {
            assert!(
                price.input > 0.0 || price.output > 0.0,
                "{name} has no non-zero price"
            );
        }
    }

    #[test]
    fn no_cache_and_no_network_still_prices() {
        // The first run on a machine with no connectivity: Cost must show real
        // numbers, not a column of "unpriced".
        let dir = std::env::temp_dir().join("surface-pricing-nocache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let prices = Prices::load(&dir, false);

        assert!(!prices.is_empty());
        assert!(prices.is_builtin());
    }
}
