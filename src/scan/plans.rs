//! The subscription plan an AI tool's own account file names.
//!
//! Two tools cache, beside their other local state, which plan they are
//! signed into: Claude Code writes an `oauthAccount` block into
//! `~/.claude.json`, and Codex keeps an OpenID token in `~/.codex/auth.json`
//! whose payload carries a `chatgpt_plan_type` claim. Both are files already
//! on this disk — nothing is fetched, and this is still not reading account
//! *state*, only the name of the plan the tool itself wrote down.
//!
//! # What leaves this module
//!
//! The plan's name, per tool. Nothing else. The files parsed here also hold
//! access tokens; those are read into memory only as far as finding the one
//! claim requires, and are never stored, logged, shown or compared. The token
//! signature is not verified — this is not authentication, it is reading a
//! label off a file the tool already trusts.
//!
//! # Why bother, when transcripts also name a plan
//!
//! A transcript names the plan that was active when usage was written; the
//! account file names the plan the tool is signed into *now*, and exists even
//! for a tool that has not run in the window. Where both speak, the account
//! file wins — see the merge in [`crate::scan::run`].

use std::collections::BTreeMap;
use std::path::Path;

/// Where a detected plan came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSource {
    /// The tool's account file: what it is signed into right now.
    Account,
    /// A transcript record: what it was on when it last wrote usage.
    Transcript,
}

/// One tool's detected plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedPlan {
    pub plan: String,
    pub source: PlanSource,
}

/// Plans named by account files, keyed by *usage* tool id.
pub fn scan() -> BTreeMap<String, DetectedPlan> {
    let Some(home) = crate::paths::home() else {
        return BTreeMap::new();
    };
    let mut plans = BTreeMap::new();
    if let Some(plan) = claude_seat(&home.join(".claude.json")) {
        plans.insert(
            "claude_code".to_string(),
            DetectedPlan {
                plan,
                source: PlanSource::Account,
            },
        );
    }
    if let Some(plan) = codex_plan(&home.join(".codex/auth.json")) {
        plans.insert(
            "codex".to_string(),
            DetectedPlan {
                plan,
                source: PlanSource::Account,
            },
        );
    }
    plans
}

/// Fold transcript-named plans in behind account-named ones.
///
/// The account file is what the tool is signed into *now*; a transcript names
/// the plan that was active when usage was written. Where both speak, now
/// wins.
pub fn merge_transcripts(
    plans: &mut BTreeMap<String, DetectedPlan>,
    transcripts: &BTreeMap<String, String>,
) {
    for (tool, plan) in transcripts {
        plans.entry(tool.clone()).or_insert_with(|| DetectedPlan {
            plan: plan.clone(),
            source: PlanSource::Transcript,
        });
    }
}

/// The usage-ledger tool id for a detection id, where the two disagree.
///
/// Usage sources predate the tool table and named Codex `codex`; the
/// detection table calls it `openai_codex`. Plans are keyed the usage way
/// because pricing is, so the Tools view translates through this.
pub fn usage_tool_id(detection_id: &str) -> &str {
    match detection_id {
        "openai_codex" => "codex",
        other => other,
    }
}

/// The seat named by Claude Code's `~/.claude.json`.
///
/// Two fields describe it, and the *capacity* one prices better. A Team
/// premium seat reads `seatTier: team_tier_1` — the same slug as a standard
/// seat — while its `userRateLimitTier: default_claude_max_5x` names the
/// Max-class capacity actually being paid for; pricing by the seat slug
/// under-reported a real premium seat by 70%. So: the rate-limit tier when
/// it carries a known list price, the seat tier otherwise (a standard seat's
/// rate tier is unpriceable, a personal plan names no seat at all). Only
/// `oauthAccount` is looked at — the rest of the file is never walked.
fn claude_seat(path: &Path) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let account = value.get("oauthAccount")?;
    let field = |key: &str| {
        account
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let rate = field("userRateLimitTier");
    match rate {
        Some(rate) if crate::config::list_price(&rate).is_some() => Some(rate),
        rate => field("seatTier").or(rate),
    }
}

/// The plan named by Codex's `~/.codex/auth.json`.
///
/// The file holds an OpenID `id_token` whose payload is a base64url JSON
/// object carrying `chatgpt_plan_type` under the `https://api.openai.com/auth`
/// claim. The payload is decoded for that one field; the token is neither
/// verified nor kept.
fn codex_plan(path: &Path) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let token = value.get("tokens")?.get("id_token")?.as_str()?;
    plan_in_id_token(token)
}

/// `chatgpt_plan_type` out of a JWT, without trusting anything else in it.
fn plan_in_id_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let claims: serde_json::Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    claims
        .get("https://api.openai.com/auth")?
        .get("chatgpt_plan_type")?
        .as_str()
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}

/// Base64url without padding, as JWT segments are written. Hand-rolled:
/// fifteen lines against a new dependency for one segment of one file.
fn base64url(s: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.as_bytes().chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        let bytes = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&bytes[..chunk.len() - 1]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("surface-plans");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A JWT in shape only: unsigned, unverified, exactly like the parser
    /// treats the real one.
    fn fake_jwt(claims: serde_json::Value) -> String {
        fn encode(bytes: &[u8]) -> String {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let mut acc = 0u32;
                for (i, &b) in chunk.iter().enumerate() {
                    acc |= u32::from(b) << (16 - 8 * i);
                }
                for i in 0..=chunk.len() {
                    out.push(ALPHABET[((acc >> (18 - 6 * i)) & 63) as usize] as char);
                }
            }
            out
        }
        format!(
            "{}.{}.x",
            encode(b"{}"),
            encode(claims.to_string().as_bytes())
        )
    }

    #[test]
    fn a_premium_seat_is_priced_by_its_capacity_not_its_seat_slug() {
        // seatTier reads `team_tier_1` for standard and premium seats alike;
        // the rate tier is what tells them apart, so it wins when priceable.
        let path = temp_file(
            "claude.json",
            &json!({"oauthAccount": {
                "seatTier": "team_tier_1",
                "userRateLimitTier": "default_claude_max_5x",
                "emailAddress": "someone@example.com"
            }})
            .to_string(),
        );
        assert_eq!(claude_seat(&path).as_deref(), Some("default_claude_max_5x"));
    }

    #[test]
    fn a_standard_seat_keeps_its_seat_slug_and_gaps_stay_honest() {
        // A standard seat's rate tier names no priced capacity.
        let path = temp_file(
            "claude-standard.json",
            &json!({"oauthAccount": {
                "seatTier": "team_tier_1",
                "userRateLimitTier": "default_claude"
            }})
            .to_string(),
        );
        assert_eq!(claude_seat(&path).as_deref(), Some("team_tier_1"));

        // A personal plan names no seat at all.
        let path = temp_file(
            "claude-personal.json",
            &json!({"oauthAccount": {"userRateLimitTier": "default_claude_max_5x"}}).to_string(),
        );
        assert_eq!(claude_seat(&path).as_deref(), Some("default_claude_max_5x"));

        let path = temp_file("claude-empty.json", &json!({"projects": {}}).to_string());
        assert_eq!(claude_seat(&path), None, "signed out is None, not a guess");
    }

    #[test]
    fn the_codex_plan_is_read_from_the_token_payload() {
        let jwt = fake_jwt(json!({
            "https://api.openai.com/auth": {"chatgpt_plan_type": "team"},
            "email": "someone@example.com"
        }));
        let path = temp_file(
            "auth.json",
            &json!({"tokens": {"id_token": jwt}}).to_string(),
        );
        assert_eq!(codex_plan(&path).as_deref(), Some("team"));
    }

    #[test]
    fn a_token_without_the_claim_is_none_rather_than_an_error() {
        assert_eq!(plan_in_id_token(&fake_jwt(json!({"sub": "u-1"}))), None);
        assert_eq!(plan_in_id_token("not-a-jwt"), None);
        assert_eq!(plan_in_id_token(""), None);
    }

    #[test]
    fn missing_files_scan_to_nothing() {
        assert_eq!(claude_seat(Path::new("/nonexistent/claude.json")), None);
        assert_eq!(codex_plan(Path::new("/nonexistent/auth.json")), None);
    }

    #[test]
    fn the_detection_id_translates_to_the_usage_id() {
        assert_eq!(usage_tool_id("openai_codex"), "codex");
        assert_eq!(usage_tool_id("claude_code"), "claude_code");
    }

    #[test]
    fn an_account_plan_beats_a_transcript_plan_and_gaps_are_filled() {
        let mut plans = BTreeMap::from([(
            "codex".to_string(),
            DetectedPlan {
                plan: "team".to_string(),
                source: PlanSource::Account,
            },
        )]);
        let transcripts = BTreeMap::from([
            ("codex".to_string(), "pro".to_string()),
            ("claude_code".to_string(), "max_5x".to_string()),
        ]);

        merge_transcripts(&mut plans, &transcripts);

        assert_eq!(plans["codex"].plan, "team", "now beats then");
        assert_eq!(plans["codex"].source, PlanSource::Account);
        assert_eq!(plans["claude_code"].plan, "max_5x", "gap filled");
        assert_eq!(plans["claude_code"].source, PlanSource::Transcript);
    }
}
