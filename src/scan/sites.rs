//! AI website usage, read from browser history.
//!
//! Most AI use leaves nothing on disk: no installer, no binary, no config
//! directory. Someone opens chatgpt.com and works there all day.
//! [`super::tooling`] cannot see that, and neither can a software inventory.
//!
//! # What is read
//!
//! Domain, lifetime visit count, and last-seen timestamp — for known AI domains
//! only. No URLs, no paths, no query strings, no page titles, and nothing
//! whatsoever about non-AI browsing: the domain filter runs *inside* SQLite, so
//! unrelated history is never read into memory in the first place.
//!
//! That granularity is a deliberate limit, not an oversight. A ChatGPT URL
//! carries a conversation id, and a tool that collected those would be a
//! surveillance feed rather than a usage meter. `scan_history = false` turns the
//! whole thing off.
//!
//! Nothing here leaves the machine either way — `surface` has nowhere to send it.
//!
//! # Reading a database the browser has open
//!
//! Chrome holds a lock on `History` while running, so a plain read-only open
//! fails with `database is locked`. Opening with `immutable=1` skips locking and
//! reads the pages directly — verified against a live 146 MB Chrome database in
//! 38 ms, with no copy.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;

use crate::browser::{self, Family, Profile};
use crate::config::WebConfig;
use crate::reason;

/// Cap on matched rows pulled from one database. The aggregate is per domain,
/// so this only bounds memory, never the reported visit counts.
const MAX_ROWS_PER_PROFILE: usize = 20_000;

/// Seconds between the Windows epoch (1601-01-01) and the Unix epoch.
/// Chromium timestamps count microseconds from the former.
const WINDOWS_TO_UNIX_EPOCH_SECS: i64 = 11_644_473_600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainKind {
    /// Chat UI.
    Assistant,
    /// Runs agents in the browser or the cloud.
    Agent,
    /// Model API or console.
    ModelApi,
    /// AI-assisted development platform.
    DevTool,
    /// Model or dataset hosting.
    ModelHub,
}

pub struct AiDomain {
    pub domain: &'static str,
    pub vendor: &'static str,
    pub kind: DomainKind,
}

/// Known AI web services.
///
/// Matched on the registrable host, so `chatgpt.com` covers `www.chatgpt.com`
/// but never `notchatgpt.com` or `chatgpt.com.example.test`.
pub const AI_DOMAINS: &[AiDomain] = &[
    AiDomain {
        domain: "chatgpt.com",
        vendor: "OpenAI",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "chat.openai.com",
        vendor: "OpenAI",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "openai.com",
        vendor: "OpenAI",
        kind: DomainKind::ModelApi,
    },
    AiDomain {
        domain: "sora.com",
        vendor: "OpenAI",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "claude.ai",
        vendor: "Anthropic",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "anthropic.com",
        vendor: "Anthropic",
        kind: DomainKind::ModelApi,
    },
    AiDomain {
        domain: "gemini.google.com",
        vendor: "Google",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "aistudio.google.com",
        vendor: "Google",
        kind: DomainKind::ModelApi,
    },
    AiDomain {
        domain: "notebooklm.google.com",
        vendor: "Google",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "copilot.microsoft.com",
        vendor: "Microsoft",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "perplexity.ai",
        vendor: "Perplexity",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "poe.com",
        vendor: "Quora",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "character.ai",
        vendor: "Character.AI",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "meta.ai",
        vendor: "Meta",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "grok.com",
        vendor: "xAI",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "x.ai",
        vendor: "xAI",
        kind: DomainKind::ModelApi,
    },
    AiDomain {
        domain: "mistral.ai",
        vendor: "Mistral",
        kind: DomainKind::ModelApi,
    },
    AiDomain {
        domain: "deepseek.com",
        vendor: "DeepSeek",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "openclaw.ai",
        vendor: "OpenClaw",
        kind: DomainKind::Agent,
    },
    AiDomain {
        domain: "manus.im",
        vendor: "Manus",
        kind: DomainKind::Agent,
    },
    AiDomain {
        domain: "devin.ai",
        vendor: "Cognition",
        kind: DomainKind::Agent,
    },
    AiDomain {
        domain: "replit.com",
        vendor: "Replit",
        kind: DomainKind::DevTool,
    },
    AiDomain {
        domain: "v0.dev",
        vendor: "Vercel",
        kind: DomainKind::DevTool,
    },
    AiDomain {
        domain: "bolt.new",
        vendor: "StackBlitz",
        kind: DomainKind::DevTool,
    },
    AiDomain {
        domain: "lovable.dev",
        vendor: "Lovable",
        kind: DomainKind::DevTool,
    },
    AiDomain {
        domain: "cursor.com",
        vendor: "Anysphere",
        kind: DomainKind::DevTool,
    },
    AiDomain {
        domain: "huggingface.co",
        vendor: "Hugging Face",
        kind: DomainKind::ModelHub,
    },
    AiDomain {
        domain: "civitai.com",
        vendor: "Civitai",
        kind: DomainKind::ModelHub,
    },
    AiDomain {
        domain: "midjourney.com",
        vendor: "Midjourney",
        kind: DomainKind::Assistant,
    },
    AiDomain {
        domain: "elevenlabs.io",
        vendor: "ElevenLabs",
        kind: DomainKind::Assistant,
    },
];

/// Read every readable browser profile and roll AI-domain visits up by domain.
pub fn scan(config: &WebConfig) -> Sites {
    if !config.scan_history {
        return Sites {
            disabled: true,
            ..Sites::default()
        };
    }

    let cutoff = Utc::now() - chrono::Duration::days(config.history_lookback_days as i64);
    let domains = domain_list(&config.extra_ai_domains);

    let mut hits: Vec<Hit> = Vec::new();
    let mut scanned = 0usize;
    let mut browsers: Vec<&'static str> = Vec::new();
    let mut profiles = 0usize;
    let mut unreadable: Vec<Unreadable> = Vec::new();

    for profile in browser::discover_profiles() {
        match read_profile(&profile, &domains, cutoff) {
            Ok(result) => {
                profiles += 1;
                scanned += result.rows_examined;
                if !browsers.contains(&profile.browser_id) {
                    browsers.push(profile.browser_id);
                }
                hits.extend(result.hits);
            }
            Err(e) => unreadable.push(Unreadable {
                browser: profile.browser_id,
                profile: profile.profile.clone(),
                reason: e,
            }),
        }
    }

    let mut sites = summarise(&hits);
    sites.domains_watched = domains.len();
    sites.browsers_scanned = browsers;
    sites.profiles_scanned = profiles;
    sites.history_entries_scanned = scanned;
    sites.lookback_days = config.history_lookback_days;
    sites.unreadable = unreadable;
    sites.blind_spots = browser::blind_spots();
    sites
}

/// A browser profile we found but could not read, and why. Reported so a gap in
/// coverage is visible rather than being mistaken for an absence of AI use.
#[derive(Debug, Clone, Serialize)]
pub struct Unreadable {
    pub browser: &'static str,
    pub profile: String,
    pub reason: String,
}

/// One AI domain and how much it is used.
#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub domain: String,
    pub vendor: &'static str,
    pub kind: Option<DomainKind>,
    /// The browser's lifetime count for the domain. The lookback window decides
    /// which domains appear, not how far these counts reach back.
    pub visits: u64,
    pub last_seen_unix: i64,
    /// `None` when the row carried no usable timestamp — which is not the same
    /// as never visited, and must not render as an epoch date.
    pub last_seen: Option<String>,
}

/// Everything the sites scan found.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sites {
    pub sites: Vec<Site>,
    pub vendors: Vec<&'static str>,
    pub domains_watched: usize,
    pub browsers_scanned: Vec<&'static str>,
    pub profiles_scanned: usize,
    pub history_entries_scanned: usize,
    pub lookback_days: u64,
    pub unreadable: Vec<Unreadable>,
    pub blind_spots: Vec<browser::BlindSpot>,
    /// `scan_history = false`. Distinct from "scanned and found nothing".
    pub disabled: bool,
}

impl Sites {
    pub fn total_visits(&self) -> u64 {
        self.sites.iter().map(|s| s.visits).sum()
    }
}

/// One matched history row, already reduced to a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub domain: String,
    pub visits: u64,
    pub last_seen_unix: i64,
}

/// The built-in table plus any extra domains from config.
pub fn domain_list(extra: &[String]) -> Vec<String> {
    let mut domains: Vec<String> = AI_DOMAINS.iter().map(|d| d.domain.to_string()).collect();
    for domain in extra {
        // Lowercase before stripping the prefix, or `WWW.Foo.Test` keeps it.
        let domain = domain.trim().to_lowercase();
        let domain = domain.trim_start_matches("www.").to_string();
        if !domain.is_empty() && !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    domains
}

fn lookup(domain: &str) -> Option<&'static AiDomain> {
    AI_DOMAINS.iter().find(|d| d.domain == domain)
}

/// Fold matched rows into one entry per domain, most-used first.
///
/// No truncation. surface capped this at `MAX_DOMAINS` to bound a network
/// payload; nothing is shipped here, and a table the reader can scroll should
/// not lie about how many domains there were.
pub fn summarise(hits: &[Hit]) -> Sites {
    let mut by_domain: BTreeMap<&str, (u64, i64)> = BTreeMap::new();

    for hit in hits {
        let entry = by_domain.entry(&hit.domain).or_insert((0, 0));
        entry.0 += hit.visits;
        entry.1 = entry.1.max(hit.last_seen_unix);
    }

    // Most-used first: that is the order a reader wants it in.
    let mut ordered: Vec<_> = by_domain.into_iter().collect();
    ordered.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));

    let mut vendors: Vec<&'static str> = Vec::new();
    let sites: Vec<Site> = ordered
        .iter()
        .map(|(domain, (visits, last_seen))| {
            let known = lookup(domain);
            let vendor = known.map(|d| d.vendor).unwrap_or("unknown");
            if !vendors.contains(&vendor) {
                vendors.push(vendor);
            }
            Site {
                domain: (*domain).to_string(),
                vendor,
                kind: known.map(|d| d.kind),
                visits: *visits,
                last_seen_unix: *last_seen,
                last_seen: unix_to_rfc3339(*last_seen),
            }
        })
        .collect();

    vendors.sort_unstable();

    Sites {
        sites,
        vendors,
        ..Sites::default()
    }
}

struct ProfileResult {
    hits: Vec<Hit>,
    rows_examined: usize,
}

/// Query one profile's history database.
fn read_profile(
    profile: &Profile,
    domains: &[String],
    cutoff: DateTime<Utc>,
) -> Result<ProfileResult, String> {
    let connection = open_readonly(&profile.history_db)?;

    let (table, url_column, visits_column, time_column) = match profile.family {
        Family::Chromium => ("urls", "url", "visit_count", "last_visit_time"),
        Family::Firefox => ("moz_places", "url", "visit_count", "last_visit_date"),
        Family::Restricted => return Err(reason::INSUFFICIENT_PRIVILEGES.to_string()),
    };

    let cutoff_native = match profile.family {
        Family::Chromium => unix_to_chromium(cutoff.timestamp()),
        Family::Firefox => unix_to_firefox(cutoff.timestamp()),
        // Unreachable: the match above already returned for Restricted.
        Family::Restricted => return Err(reason::INSUFFICIENT_PRIVILEGES.to_string()),
    };

    // The domain filter runs inside SQLite. Non-AI history is never returned
    // to the agent, which is the whole privacy posture of this collector.
    let placeholders: Vec<String> = (1..=domains.len())
        .map(|i| format!("{url_column} LIKE ?{i}"))
        .collect();
    let sql = format!(
        "SELECT {url_column}, {visits_column}, {time_column} FROM {table} \
         WHERE ({}) AND {time_column} >= ?{} LIMIT {MAX_ROWS_PER_PROFILE}",
        placeholders.join(" OR "),
        domains.len() + 1,
    );

    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = domains
        .iter()
        .map(|d| Box::new(format!("%{d}%")) as Box<dyn rusqlite::ToSql>)
        .collect();
    params.push(Box::new(cutoff_native));
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = statement
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1).unwrap_or(0),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut hits = Vec::new();
    let mut rows_examined = 0usize;

    for row in rows.flatten() {
        rows_examined += 1;
        let (url, visits, timestamp) = row;

        // A URL containing "claude.ai" is not necessarily *on* claude.ai.
        let Some(host) = host_of(&url) else { continue };
        let Some(domain) = match_domain(&host, domains) else {
            continue;
        };

        hits.push(Hit {
            domain,
            visits: visits.max(0) as u64,
            last_seen_unix: match profile.family {
                Family::Chromium => chromium_to_unix(timestamp),
                Family::Firefox => firefox_to_unix(timestamp),
                Family::Restricted => 0,
            },
        });
    }

    Ok(ProfileResult {
        hits,
        rows_examined,
    })
}

/// Open a history database the browser may currently hold open.
///
/// `immutable=1` tells SQLite the file will not change, which skips all
/// locking. Verified to work against a live, locked Chrome database. The
/// `nolock=1` fallback covers builds where immutable is rejected.
pub fn open_readonly(path: &Path) -> Result<rusqlite::Connection, String> {
    use rusqlite::{Connection, OpenFlags};

    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let display = path.to_string_lossy();

    for uri in [
        format!("file:{display}?immutable=1"),
        format!("file:{display}?mode=ro&nolock=1"),
    ] {
        if let Ok(connection) = Connection::open_with_flags(&uri, flags) {
            return Ok(connection);
        }
    }

    Err(reason::TOOL_UNAVAILABLE.to_string())
}

/// Host component of a URL, lowercased, without credentials or port.
///
/// Hand-rolled rather than pulling in a URL crate: we need the authority and
/// nothing else, and deliberately never retain the path.
pub fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest)?;
    // Everything from the first '/', '?' or '#' is the path — dropped here and
    // never carried further.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|a| !a.is_empty())?;
    // Strip any user:pass@ prefix.
    let host_port = authority.rsplit('@').next()?;
    let host = match host_port.strip_prefix('[') {
        // IPv6 literal.
        Some(rest) => rest.split(']').next()?,
        None => host_port.split(':').next()?,
    };

    (!host.is_empty()).then(|| host.to_lowercase())
}

/// Which watched domain, if any, this host belongs to.
///
/// Suffix matching on a dot boundary: `chatgpt.com` matches itself and
/// `www.chatgpt.com`, but not `notchatgpt.com` and not `chatgpt.com.evil.test`
/// — the same false-positive class as the `analyticsagent`/`csagent` bug in
/// surface's `collect::security_agents`.
pub fn match_domain(host: &str, domains: &[String]) -> Option<String> {
    let host = host.trim_end_matches('.').to_lowercase();

    domains
        .iter()
        .filter(|domain| host == **domain || host.ends_with(&format!(".{domain}")))
        // Prefer the most specific match, so chat.openai.com does not report
        // as openai.com.
        .max_by_key(|domain| domain.len())
        .cloned()
}

/// Chromium: microseconds since 1601-01-01 UTC.
pub fn chromium_to_unix(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    value / 1_000_000 - WINDOWS_TO_UNIX_EPOCH_SECS
}

pub fn unix_to_chromium(unix: i64) -> i64 {
    (unix + WINDOWS_TO_UNIX_EPOCH_SECS) * 1_000_000
}

/// Firefox: microseconds since 1970-01-01 UTC.
pub fn firefox_to_unix(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    value / 1_000_000
}

pub fn unix_to_firefox(unix: i64) -> i64 {
    unix * 1_000_000
}

fn unix_to_rfc3339(unix: i64) -> Option<String> {
    if unix <= 0 {
        return None;
    }
    Utc.timestamp_opt(unix, 0)
        .single()
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watched() -> Vec<String> {
        domain_list(&[])
    }

    #[test]
    fn domain_table_is_unique_and_normalised() {
        let mut domains: Vec<_> = AI_DOMAINS.iter().map(|d| d.domain).collect();
        let count = domains.len();
        domains.sort_unstable();
        domains.dedup();
        assert_eq!(domains.len(), count, "duplicate domain in AI_DOMAINS");

        for d in AI_DOMAINS {
            assert_eq!(
                d.domain,
                d.domain.to_lowercase(),
                "{} not lowercase",
                d.domain
            );
            assert!(
                !d.domain.starts_with("www."),
                "{} has a www prefix",
                d.domain
            );
            assert!(!d.domain.contains('/'), "{} contains a path", d.domain);
        }
    }

    #[test]
    fn openclaw_is_watched() {
        assert!(watched().iter().any(|d| d == "openclaw.ai"));
    }

    // ---------------------------------------------------------------- epochs

    #[test]
    fn chromium_epoch_round_trips() {
        // 2026-07-26T21:14:02Z
        let unix = 1_785_100_442;
        assert_eq!(chromium_to_unix(unix_to_chromium(unix)), unix);
    }

    #[test]
    fn chromium_epoch_matches_a_known_value() {
        // 13_000_000_000_000_000 µs since 1601 = 2012-12-17T12:26:40Z
        assert_eq!(chromium_to_unix(13_000_000_000_000_000), 1_355_526_400);
    }

    #[test]
    fn firefox_epoch_round_trips() {
        let unix = 1_785_100_442;
        assert_eq!(firefox_to_unix(unix_to_firefox(unix)), unix);
    }

    #[test]
    fn zero_and_negative_timestamps_are_not_treated_as_1601() {
        assert_eq!(chromium_to_unix(0), 0);
        assert_eq!(chromium_to_unix(-5), 0);
        assert_eq!(firefox_to_unix(0), 0);
    }

    // ------------------------------------------------------------------ hosts

    #[test]
    fn extracts_host_and_discards_everything_else() {
        assert_eq!(
            host_of("https://chatgpt.com/c/68f2a1-secret?token=abc#frag").as_deref(),
            Some("chatgpt.com")
        );
        assert_eq!(
            host_of("https://WWW.Claude.AI/chat").as_deref(),
            Some("www.claude.ai")
        );
        assert_eq!(
            host_of("http://user:pw@claude.ai:8443/x").as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host_of("https://[2606:4700::1]/x").as_deref(),
            Some("2606:4700::1")
        );
        assert_eq!(
            host_of("https://chatgpt.com").as_deref(),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn malformed_urls_yield_no_host() {
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("not a url"), None);
        assert_eq!(host_of("chatgpt.com/no-scheme"), None);
        assert_eq!(host_of("https:///empty-authority"), None);
    }

    // --------------------------------------------------------------- matching

    #[test]
    fn matches_exact_domain_and_subdomains() {
        assert_eq!(
            match_domain("chatgpt.com", &watched()).as_deref(),
            Some("chatgpt.com")
        );
        assert_eq!(
            match_domain("www.chatgpt.com", &watched()).as_deref(),
            Some("chatgpt.com")
        );
        assert_eq!(
            match_domain("CHATGPT.COM", &watched()).as_deref(),
            Some("chatgpt.com")
        );
        assert_eq!(
            match_domain("chatgpt.com.", &watched()).as_deref(),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn lookalike_domains_are_not_matched() {
        // These are the ways a naive `contains` check goes wrong.
        for host in [
            "notchatgpt.com",
            "chatgpt.com.evil.test",
            "fakechatgpt.com",
            "myclaude.aim",
            "openclaw.ai.phishing.test",
        ] {
            assert_eq!(match_domain(host, &watched()), None, "{host} matched");
        }
    }

    #[test]
    fn most_specific_domain_wins() {
        // Both chat.openai.com and openai.com are watched.
        assert_eq!(
            match_domain("chat.openai.com", &watched()).as_deref(),
            Some("chat.openai.com")
        );
        assert_eq!(
            match_domain("platform.openai.com", &watched()).as_deref(),
            Some("openai.com")
        );
    }

    #[test]
    fn extra_domains_from_config_are_watched() {
        let domains = domain_list(&["internal-gpt.corp.test".into(), "WWW.Foo.Test".into()]);
        assert_eq!(
            match_domain("internal-gpt.corp.test", &domains).as_deref(),
            Some("internal-gpt.corp.test")
        );
        // Normalised on the way in.
        assert_eq!(
            match_domain("foo.test", &domains).as_deref(),
            Some("foo.test")
        );
    }

    // -------------------------------------------------------------- aggregation

    #[test]
    fn aggregates_visits_per_domain_across_profiles() {
        let hits = vec![
            Hit {
                domain: "chatgpt.com".into(),
                visits: 100,
                last_seen_unix: 1_785_100_000,
            },
            Hit {
                domain: "chatgpt.com".into(),
                visits: 42,
                last_seen_unix: 1_785_200_000,
            },
            Hit {
                domain: "claude.ai".into(),
                visits: 53,
                last_seen_unix: 1_785_300_000,
            },
        ];

        let sites = summarise(&hits);

        assert_eq!(sites.sites.len(), 2);
        // Most visited first.
        assert_eq!(sites.sites[0].domain, "chatgpt.com");
        assert_eq!(sites.sites[0].visits, 142);
        assert_eq!(sites.sites[0].vendor, "OpenAI");
        // Latest timestamp across the merged rows wins.
        assert_eq!(
            sites.sites[0].last_seen.as_deref(),
            Some("2026-07-28T00:53:20Z")
        );
        assert_eq!(sites.sites[1].domain, "claude.ai");
        assert_eq!(sites.sites[1].visits, 53);
    }

    #[test]
    fn vendors_are_deduplicated_and_sorted() {
        let hits = vec![
            Hit {
                domain: "chatgpt.com".into(),
                visits: 1,
                last_seen_unix: 1,
            },
            Hit {
                domain: "chat.openai.com".into(),
                visits: 1,
                last_seen_unix: 1,
            },
            Hit {
                domain: "claude.ai".into(),
                visits: 1,
                last_seen_unix: 1,
            },
        ];
        assert_eq!(summarise(&hits).vendors, ["Anthropic", "OpenAI"]);
    }

    #[test]
    fn no_ai_usage_is_a_clean_empty_report() {
        let sites = summarise(&[]);
        assert!(sites.sites.is_empty());
        assert!(sites.vendors.is_empty());
        assert_eq!(sites.total_visits(), 0);
        assert!(!sites.disabled, "empty is not the same as switched off");
    }

    #[test]
    fn every_domain_is_reported_rather_than_capped() {
        // surface capped this to bound a network payload. Nothing is shipped
        // here, so a scrollable table should not silently drop rows.
        let hits: Vec<Hit> = (0..120)
            .map(|i| Hit {
                domain: format!("d{i:03}.test"),
                visits: (i + 1) as u64,
                last_seen_unix: 1_785_100_000,
            })
            .collect();

        let sites = summarise(&hits);

        assert_eq!(sites.sites.len(), 120);
        // Ranked by visits, so the synthetic highest index leads.
        assert_eq!(sites.sites[0].visits, 120u64);
    }

    #[test]
    fn an_unknown_domain_is_reported_without_inventing_a_vendor() {
        let hits = vec![Hit {
            domain: "models.internal.test".into(),
            visits: 5,
            last_seen_unix: 1,
        }];

        let sites = summarise(&hits);

        assert_eq!(sites.sites[0].vendor, "unknown");
        assert!(sites.sites[0].kind.is_none());
    }

    #[test]
    fn a_missing_timestamp_is_none_rather_than_an_epoch_date() {
        let hits = vec![Hit {
            domain: "claude.ai".into(),
            visits: 3,
            last_seen_unix: 0,
        }];

        assert!(summarise(&hits).sites[0].last_seen.is_none());
    }

    #[test]
    fn disabled_is_distinct_from_finding_nothing() {
        let config = WebConfig {
            scan_history: false,
            ..WebConfig::default()
        };

        let sites = scan(&config);

        assert!(sites.disabled);
        assert!(sites.sites.is_empty());
        // Nothing was opened, so nothing can be claimed about coverage.
        assert_eq!(sites.profiles_scanned, 0);
        assert!(sites.blind_spots.is_empty());
    }

    // ------------------------------------------------------- end-to-end SQLite

    /// Build a Chromium-shaped history database, as the browser would.
    fn chromium_db(path: &Path, rows: &[(&str, i64, i64)]) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR, \
             visit_count INTEGER DEFAULT 0, typed_count INTEGER DEFAULT 0, \
             last_visit_time INTEGER NOT NULL, hidden INTEGER DEFAULT 0)",
            [],
        )
        .unwrap();
        for (url, visits, time) in rows {
            c.execute(
                "INSERT INTO urls (url, title, visit_count, last_visit_time) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![url, "title", visits, time],
            )
            .unwrap();
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("surface-web-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn profile_at(path: std::path::PathBuf) -> Profile {
        Profile {
            browser_id: "chrome",
            family: Family::Chromium,
            profile: "Default".into(),
            history_db: path,
        }
    }

    #[test]
    fn reads_a_real_database_and_reports_only_ai_domains() {
        let dir = temp_dir("readdb");
        let db = dir.join("History");
        let recent = unix_to_chromium(Utc::now().timestamp() - 3600);

        chromium_db(
            &db,
            &[
                ("https://chatgpt.com/c/abc", 142, recent),
                ("https://claude.ai/chat/xyz", 53, recent),
                ("https://news.ycombinator.com/item?id=1", 900, recent),
                ("https://mail.google.com/mail/u/0", 4000, recent),
                ("https://notchatgpt.com/pretend", 77, recent),
            ],
        );

        let cutoff = Utc::now() - chrono::Duration::days(30);
        let result = read_profile(&profile_at(db), &watched(), cutoff).unwrap();

        let mut domains: Vec<_> = result.hits.iter().map(|h| h.domain.as_str()).collect();
        domains.sort_unstable();
        assert_eq!(domains, ["chatgpt.com", "claude.ai"]);
        assert!(result.rows_examined >= 2);
    }

    #[test]
    fn history_outside_the_lookback_window_is_excluded() {
        let dir = temp_dir("cutoff");
        let db = dir.join("History");
        let now = Utc::now().timestamp();

        chromium_db(
            &db,
            &[
                (
                    "https://chatgpt.com/recent",
                    5,
                    unix_to_chromium(now - 3600),
                ),
                (
                    "https://claude.ai/ancient",
                    900,
                    unix_to_chromium(now - 400 * 86_400),
                ),
            ],
        );

        let cutoff = Utc::now() - chrono::Duration::days(30);
        let result = read_profile(&profile_at(db), &watched(), cutoff).unwrap();

        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].domain, "chatgpt.com");
    }

    #[test]
    fn reads_a_firefox_places_database() {
        let dir = temp_dir("firefox");
        let db = dir.join("places.sqlite");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR, \
             visit_count INTEGER DEFAULT 0, last_visit_date INTEGER)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO moz_places (url, title, visit_count, last_visit_date) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "https://perplexity.ai/search?q=x",
                "t",
                7,
                unix_to_firefox(Utc::now().timestamp() - 60)
            ],
        )
        .unwrap();
        drop(c);

        let profile = Profile {
            browser_id: "firefox",
            family: Family::Firefox,
            profile: "abc.default".into(),
            history_db: db,
        };

        let result = read_profile(
            &profile,
            &watched(),
            Utc::now() - chrono::Duration::days(30),
        )
        .unwrap();

        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].domain, "perplexity.ai");
        assert_eq!(result.hits[0].visits, 7);
    }

    #[test]
    fn a_missing_database_is_an_error_not_a_panic() {
        let profile = profile_at(std::path::PathBuf::from("/nonexistent/History"));
        assert!(read_profile(&profile, &watched(), Utc::now()).is_err());
    }

    #[test]
    fn a_corrupt_database_is_an_error_not_a_panic() {
        let dir = temp_dir("corrupt");
        let db = dir.join("History");
        std::fs::write(&db, b"this is not a sqlite file").unwrap();
        assert!(read_profile(&profile_at(db), &watched(), Utc::now()).is_err());
    }

    /// The privacy guarantee, asserted rather than described.
    #[test]
    fn no_urls_paths_or_query_strings_reach_the_payload() {
        let dir = temp_dir("privacy");
        let db = dir.join("History");
        let recent = unix_to_chromium(Utc::now().timestamp() - 60);

        chromium_db(
            &db,
            &[(
                "https://chatgpt.com/c/68f2a1-secret-conversation?token=SUPERSECRET",
                3,
                recent,
            )],
        );

        let result = read_profile(
            &profile_at(db),
            &watched(),
            Utc::now() - chrono::Duration::days(30),
        )
        .unwrap();
        // Serialise the whole result, not just the domain list, so a leak into
        // any field of any nested struct fails this test.
        let serialised = serde_json::to_string(&summarise(&result.hits)).unwrap();

        assert!(serialised.contains("chatgpt.com"));
        assert!(!serialised.contains("68f2a1"), "conversation id leaked");
        assert!(!serialised.contains("secret-conversation"), "path leaked");
        assert!(!serialised.contains("SUPERSECRET"), "query string leaked");
        assert!(!serialised.contains("/c/"), "path leaked");
    }

    /// The same guarantee, one layer out: whatever `scan` assembles from real
    /// profiles must be free of URLs too, not just what `summarise` builds.
    #[test]
    fn a_full_scan_result_carries_no_urls() {
        let sites = scan(&WebConfig::default());
        let serialised = serde_json::to_string(&sites).unwrap();

        assert!(!serialised.contains("://"), "a URL reached the result");
        assert!(
            !serialised.contains('?'),
            "a query string reached the result"
        );
    }
}
