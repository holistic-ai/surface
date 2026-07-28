//! AI assistants and autonomous agents installed on this machine.
//!
//! A coding agent is not ordinary software: it holds shell access, reads the
//! whole working tree, and sends what it reads to a third-party model provider.
//! None of that shows up in a conventional software inventory as anything more
//! interesting than "a binary in ~/.local/bin".
//!
//! Detection is filesystem- and process-only. No commands are executed, so this
//! cannot be slow and cannot be tricked into running a tool it is trying to
//! inventory.
//!
//! It reports what is there, not a verdict. Whether a given tool is wanted is a
//! judgement `surface` has no basis to make; the `autonomous` flag is the fact
//! worth knowing, and it is left for the reader to weigh.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Chat UI with no direct access to the machine.
    Assistant,
    /// Command-line coding agent.
    CodingAgent,
    /// Long-running autonomous agent, often with messaging or scheduling.
    AutonomousAgent,
    /// Editor or IDE with agent capabilities built in.
    Editor,
    /// Editor extension.
    Extension,
    /// Runs models locally.
    LocalRuntime,
}

impl ToolKind {
    /// Short label for a table cell.
    pub fn label(&self) -> &'static str {
        match self {
            ToolKind::Assistant => "assistant",
            ToolKind::CodingAgent => "coding agent",
            ToolKind::AutonomousAgent => "autonomous agent",
            ToolKind::Editor => "editor",
            ToolKind::Extension => "extension",
            ToolKind::LocalRuntime => "local runtime",
        }
    }
}

/// One tool we know how to recognise.
///
/// `autonomous` means the tool can execute code or take actions on the device
/// on the model's behalf — the distinction that actually matters for risk.
/// A chat window that can only talk is not the same exposure as an agent with
/// a shell.
#[derive(Debug)]
pub struct AiTool {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub kind: ToolKind,
    pub autonomous: bool,
    /// Installed application names (macOS bundle stem / Windows DisplayName).
    pub apps: &'static [&'static str],
    /// Executable file names, matched exactly (Windows extensions stripped).
    pub executables: &'static [&'static str],
    /// Home-relative config files or directories whose existence proves use.
    pub config_paths: &'static [&'static str],
    /// Running process names.
    pub processes: &'static [&'static str],
    /// Editor extension id prefixes.
    pub extensions: &'static [&'static str],
}

/// Known AI tooling.
///
/// Best-effort and expected to go stale — this space moves fast. A tool absent
/// from this table is simply not reported; nothing else breaks.
pub const AI_TOOLS: &[AiTool] = &[
    AiTool {
        id: "claude_code",
        name: "Claude Code",
        vendor: "Anthropic",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &["Claude Code URL Handler"],
        executables: &["claude"],
        config_paths: &[".claude", ".claude.json"],
        processes: &["claude"],
        extensions: &["anthropic.claude-code"],
    },
    AiTool {
        id: "claude_desktop",
        name: "Claude Desktop",
        vendor: "Anthropic",
        kind: ToolKind::Assistant,
        autonomous: false,
        apps: &["Claude"],
        executables: &[],
        config_paths: &[],
        // Not bare "Claude": process matching is case-insensitive, and Claude
        // Code's binary is `claude`. The Electron helper name is unambiguous.
        processes: &["Claude Helper"],
        extensions: &[],
    },
    AiTool {
        id: "openai_codex",
        name: "Codex CLI",
        vendor: "OpenAI",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &[],
        executables: &["codex"],
        config_paths: &[".codex"],
        processes: &["codex"],
        extensions: &["openai.chatgpt", "openai.codex"],
    },
    AiTool {
        id: "chatgpt_desktop",
        name: "ChatGPT Desktop",
        vendor: "OpenAI",
        kind: ToolKind::Assistant,
        autonomous: false,
        apps: &["ChatGPT"],
        executables: &[],
        config_paths: &[],
        processes: &["ChatGPT"],
        extensions: &[],
    },
    AiTool {
        id: "opencode",
        name: "OpenCode",
        vendor: "SST",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &[],
        executables: &["opencode"],
        config_paths: &[".opencode", ".config/opencode"],
        processes: &["opencode"],
        extensions: &[],
    },
    AiTool {
        id: "openclaw",
        name: "OpenClaw",
        vendor: "OpenClaw",
        kind: ToolKind::AutonomousAgent,
        autonomous: true,
        apps: &["OpenClaw"],
        // Previously released as Clawdbot and Moltbot; older installs persist.
        executables: &["openclaw", "clawdbot", "moltbot"],
        config_paths: &[".openclaw", ".clawdbot", ".moltbot", ".config/openclaw"],
        processes: &["openclaw", "clawdbot", "moltbot"],
        extensions: &[],
    },
    AiTool {
        id: "cursor",
        name: "Cursor",
        vendor: "Anysphere",
        kind: ToolKind::Editor,
        autonomous: true,
        apps: &["Cursor"],
        executables: &["cursor"],
        config_paths: &[".cursor"],
        processes: &["Cursor"],
        extensions: &[],
    },
    AiTool {
        id: "windsurf",
        name: "Windsurf",
        vendor: "Codeium",
        kind: ToolKind::Editor,
        autonomous: true,
        apps: &["Windsurf"],
        executables: &["windsurf"],
        config_paths: &[".windsurf", ".codeium"],
        processes: &["Windsurf"],
        extensions: &["codeium.codeium"],
    },
    AiTool {
        id: "github_copilot",
        name: "GitHub Copilot",
        vendor: "GitHub",
        kind: ToolKind::Extension,
        autonomous: false,
        apps: &[],
        executables: &["copilot"],
        config_paths: &[".config/github-copilot"],
        processes: &[],
        extensions: &["github.copilot"],
    },
    AiTool {
        id: "aider",
        name: "Aider",
        vendor: "Aider",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &[],
        executables: &["aider"],
        config_paths: &[".aider.conf.yml"],
        processes: &["aider"],
        extensions: &[],
    },
    AiTool {
        id: "goose",
        name: "Goose",
        vendor: "Block",
        kind: ToolKind::AutonomousAgent,
        autonomous: true,
        apps: &["Goose"],
        executables: &["goose"],
        config_paths: &[".config/goose"],
        processes: &["goose"],
        extensions: &[],
    },
    AiTool {
        id: "hermes_agent",
        name: "Hermes Agent",
        vendor: "Nous Research",
        kind: ToolKind::AutonomousAgent,
        autonomous: true,
        apps: &[],
        // Deliberately no `executables` entry. Hermes Agent's command is
        // `hermes`, but so is React Native's Hermes JavaScript engine, and one
        // channel is enough to report a tool — so a `hermes` on `PATH` would
        // report an autonomous agent on the machine of anyone who has the JS
        // engine installed. `~/.hermes` is created when the agent initialises
        // and belongs to nothing else, so it carries the detection instead. The
        // cost is that an installed-but-never-run Hermes is not reported, which
        // is the right way round: a missed tool is a gap, a wrong one is a lie.
        executables: &[],
        config_paths: &[".hermes"],
        processes: &["hermes"],
        extensions: &[],
    },
    AiTool {
        id: "gemini_cli",
        name: "Gemini CLI",
        vendor: "Google",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &[],
        executables: &["gemini"],
        config_paths: &[".gemini"],
        processes: &["gemini"],
        extensions: &["google.gemini-code-assist"],
    },
    AiTool {
        id: "amp",
        name: "Amp",
        vendor: "Sourcegraph",
        kind: ToolKind::CodingAgent,
        autonomous: true,
        apps: &[],
        executables: &["amp"],
        config_paths: &[".config/amp"],
        processes: &[],
        extensions: &["sourcegraph.amp"],
    },
    AiTool {
        id: "cline",
        name: "Cline",
        vendor: "Cline",
        kind: ToolKind::Extension,
        autonomous: true,
        apps: &[],
        executables: &[],
        config_paths: &[],
        processes: &[],
        extensions: &["saoudrizwan.claude-dev", "rooveterinaryinc.roo-cline"],
    },
    AiTool {
        id: "continue",
        name: "Continue",
        vendor: "Continue",
        kind: ToolKind::Extension,
        autonomous: true,
        apps: &[],
        executables: &["cn"],
        config_paths: &[".continue"],
        processes: &[],
        extensions: &["continue.continue"],
    },
    AiTool {
        id: "ollama",
        name: "Ollama",
        vendor: "Ollama",
        kind: ToolKind::LocalRuntime,
        autonomous: false,
        apps: &["Ollama"],
        executables: &["ollama"],
        config_paths: &[".ollama"],
        processes: &["ollama"],
        extensions: &[],
    },
    AiTool {
        id: "lm_studio",
        name: "LM Studio",
        vendor: "LM Studio",
        kind: ToolKind::LocalRuntime,
        autonomous: false,
        apps: &["LM Studio"],
        executables: &["lms"],
        config_paths: &[".lmstudio", ".cache/lm-studio"],
        processes: &["LM Studio"],
        extensions: &[],
    },
];

/// Executable directories worth checking beyond `PATH` — agents installed by
/// their own bootstrap scripts frequently are not on the PATH of a background
/// service, but are still installed and runnable by the user.
const EXTRA_BIN_DIRS: &[&str] = &[
    ".local/bin",
    ".cargo/bin",
    ".bun/bin",
    ".deno/bin",
    ".volta/bin",
    ".npm-global/bin",
    ".opencode/bin",
    ".codeium/bin",
];

/// Editor extension directories, home-relative.
const EXTENSION_DIRS: &[&str] = &[
    ".vscode/extensions",
    ".vscode-insiders/extensions",
    ".cursor/extensions",
    ".windsurf/extensions",
    ".vscodium/extensions",
];

/// Probe this machine and match what is found against [`AI_TOOLS`].
///
/// `processes` is passed in rather than gathered here so the caller owns the one
/// expensive piece of system state and the rest stays pure.
pub fn scan(processes: Vec<String>) -> Vec<Detected> {
    let observed = Observed {
        apps: super::apps::enumerate(),
        executables: find_executables(),
        config_paths: find_config_paths(),
        processes,
        extensions: find_extensions(),
    };

    match_tools(&observed)
}

/// Everything the probes saw, before any matching. Keeping this separate is
/// what makes the matcher a pure function testable on any platform.
#[derive(Debug, Default)]
pub struct Observed {
    pub apps: Vec<String>,
    /// `(file name, full path)`.
    pub executables: Vec<(String, String)>,
    /// Home-relative paths that exist.
    pub config_paths: Vec<String>,
    pub processes: Vec<String>,
    /// Extension directory names, e.g. `anthropic.claude-code-2.1.218`.
    pub extensions: Vec<String>,
}

/// A tool that was found, and what gave it away.
#[derive(Debug)]
pub struct Detected {
    pub tool: &'static AiTool,
    pub evidence: Vec<String>,
}

/// Match observations against [`AI_TOOLS`].
pub fn match_tools(observed: &Observed) -> Vec<Detected> {
    let mut detected = Vec::new();

    for tool in AI_TOOLS {
        let mut evidence = Vec::new();

        for app in observed.apps.iter() {
            if tool.apps.iter().any(|pattern| matches_app(app, pattern)) {
                evidence.push(format!("app:{app}"));
            }
        }

        for (name, path) in observed.executables.iter() {
            if tool
                .executables
                .iter()
                .any(|exe| matches_executable(name, exe))
            {
                evidence.push(format!("executable:{path}"));
            }
        }

        for config in observed.config_paths.iter() {
            if tool.config_paths.iter().any(|p| p == config) {
                evidence.push(format!("config:~/{config}"));
            }
        }

        for process in observed.processes.iter() {
            if tool.processes.iter().any(|p| matches_process(process, p)) {
                evidence.push(format!("process:{process}"));
            }
        }

        for extension in observed.extensions.iter() {
            if tool
                .extensions
                .iter()
                .any(|prefix| extension.to_lowercase().starts_with(prefix))
            {
                evidence.push(format!("extension:{extension}"));
            }
        }

        if !evidence.is_empty() {
            evidence.sort();
            evidence.dedup();
            detected.push(Detected { tool, evidence });
        }
    }

    detected
}

/// Build the evidence payload.
/// Headline counts over a detection set.
pub fn summarise(detected: &[Detected]) -> Summary {
    Summary {
        detected: detected.len(),
        autonomous: detected.iter().filter(|d| d.tool.autonomous).count(),
        vendors: detected.iter().map(|d| d.tool.vendor).collect(),
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub detected: usize,
    pub autonomous: usize,
    pub vendors: BTreeSet<&'static str>,
}

/// Application names need an exact match: `Claude` must not claim
/// `Claude Code URL Handler`, which belongs to a different entry in the table.
/// Windows DisplayNames carry suffixes like `Cursor (User)`, so a parenthesised
/// or dashed qualifier is allowed.
fn matches_app(observed: &str, pattern: &str) -> bool {
    let observed = observed.trim();
    if observed.eq_ignore_ascii_case(pattern) {
        return true;
    }
    observed
        .get(..pattern.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(pattern))
        && matches!(&observed[pattern.len()..], rest if rest.starts_with(" (") || rest.starts_with(" - "))
}

/// Executables match exactly, after stripping a Windows extension.
fn matches_executable(observed: &str, pattern: &str) -> bool {
    let stem = observed
        .rsplit_once('.')
        .filter(|(_, ext)| matches!(*ext, "exe" | "cmd" | "bat" | "ps1"))
        .map(|(stem, _)| stem)
        .unwrap_or(observed);
    stem.eq_ignore_ascii_case(pattern)
}

/// Process names allow a separator suffix (`claude-code`, `ollama.exe`) but not
/// an alphanumeric one, so `codexplorer` never reads as `codex`.
///
/// A space is only a valid separator when the pattern is itself multi-word.
/// Otherwise `Claude Helper` — Claude Desktop's Electron child — would match
/// the single-token pattern `claude` and be attributed to Claude Code.
fn matches_process(observed: &str, pattern: &str) -> bool {
    let observed = observed.to_lowercase();
    let pattern = pattern.to_lowercase();
    if observed == pattern {
        return true;
    }

    let pattern_is_multiword = pattern.contains(' ');
    observed
        .strip_prefix(&pattern)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|next| !next.is_alphanumeric() && (pattern_is_multiword || next != ' '))
}

// ------------------------------------------------------------------ probes

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Every interesting executable name in the table, so directory scans compare
/// against a set rather than re-walking per tool.
fn interesting_executables() -> BTreeSet<&'static str> {
    AI_TOOLS
        .iter()
        .flat_map(|t| t.executables.iter().copied())
        .collect()
}

fn find_executables() -> Vec<(String, String)> {
    let wanted = interesting_executables();
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = home() {
        dirs.extend(EXTRA_BIN_DIRS.iter().map(|d| home.join(d)));
    }

    let mut seen_dirs = BTreeSet::new();
    let mut found = Vec::new();

    for dir in dirs {
        if !seen_dirs.insert(dir.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if wanted.iter().any(|w| matches_executable(&name, w)) {
                found.push((name, entry.path().to_string_lossy().to_string()));
            }
        }
    }

    found
}

fn find_config_paths() -> Vec<String> {
    let Some(home) = home() else {
        return Vec::new();
    };

    AI_TOOLS
        .iter()
        .flat_map(|t| t.config_paths.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|relative| home.join(relative).exists())
        .map(String::from)
        .collect()
}

fn find_extensions() -> Vec<String> {
    let Some(home) = home() else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for dir in EXTENSION_DIRS {
        let Ok(entries) = std::fs::read_dir(home.join(dir)) else {
            continue;
        };
        found.extend(
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| !name.starts_with('.') && name != "extensions.json"),
        );
    }

    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed_with_executables(list: &[&str]) -> Observed {
        Observed {
            executables: list
                .iter()
                .map(|n| (n.to_string(), format!("/usr/local/bin/{n}")))
                .collect(),
            ..Default::default()
        }
    }

    fn detected_ids(detected: &[Detected]) -> Vec<&'static str> {
        detected.iter().map(|d| d.tool.id).collect()
    }

    #[test]
    fn table_ids_are_unique() {
        let mut ids: Vec<_> = AI_TOOLS.iter().map(|t| t.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate tool id in AI_TOOLS");
    }

    #[test]
    fn a_react_native_hermes_is_not_reported_as_an_agent() {
        // `hermes` is both Nous Research's agent and React Native's JS engine,
        // and one channel is enough to report a tool. So Hermes Agent carries no
        // executable rule, and a bare `hermes` on `PATH` must detect nothing —
        // otherwise every React Native developer is told an autonomous agent is
        // installed. If this fails, someone added `hermes` to `executables`.
        let observed = observed_with_executables(&["hermes", "hermesc"]);
        assert_eq!(detected_ids(&match_tools(&observed)), Vec::<&str>::new());

        // The directory the agent actually creates is what finds it.
        let observed = Observed {
            config_paths: vec![".hermes".to_string()],
            ..Default::default()
        };
        assert_eq!(detected_ids(&match_tools(&observed)), ["hermes_agent"]);
    }

    #[test]
    fn every_tool_is_detectable_by_something() {
        for tool in AI_TOOLS {
            let detectable = !tool.apps.is_empty()
                || !tool.executables.is_empty()
                || !tool.config_paths.is_empty()
                || !tool.processes.is_empty()
                || !tool.extensions.is_empty();
            assert!(detectable, "{} has no detection rules", tool.id);
        }
    }

    #[test]
    fn detects_cli_agents_by_executable() {
        let detected = match_tools(&observed_with_executables(&[
            "claude", "codex", "opencode", "openclaw",
        ]));
        let ids = detected_ids(&detected);
        assert!(ids.contains(&"claude_code"));
        assert!(ids.contains(&"openai_codex"));
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"openclaw"));
    }

    #[test]
    fn detects_openclaw_under_its_former_names() {
        for legacy in ["clawdbot", "moltbot"] {
            let detected = match_tools(&observed_with_executables(&[legacy]));
            assert_eq!(detected_ids(&detected), vec!["openclaw"], "{legacy}");
        }
    }

    #[test]
    fn records_where_each_tool_was_found() {
        let observed = Observed {
            executables: vec![("claude".into(), "/Users/x/.local/bin/claude".into())],
            config_paths: vec![".claude".into()],
            processes: vec!["claude".into()],
            ..Default::default()
        };

        let detected = match_tools(&observed);
        assert_eq!(detected.len(), 1);
        assert_eq!(
            detected[0].evidence,
            vec![
                "config:~/.claude",
                "executable:/Users/x/.local/bin/claude",
                "process:claude",
            ]
        );
    }

    #[test]
    fn desktop_app_does_not_claim_a_different_tools_bundle() {
        // Both exist on a Mac running Claude Code and Claude Desktop, and each
        // must map to its own entry.
        let observed = Observed {
            apps: vec!["Claude".into(), "Claude Code URL Handler".into()],
            ..Default::default()
        };

        let detected = match_tools(&observed);
        let desktop = detected
            .iter()
            .find(|d| d.tool.id == "claude_desktop")
            .unwrap();
        let code = detected
            .iter()
            .find(|d| d.tool.id == "claude_code")
            .unwrap();

        assert_eq!(desktop.evidence, vec!["app:Claude"]);
        assert_eq!(code.evidence, vec!["app:Claude Code URL Handler"]);
    }

    #[test]
    fn claude_code_and_claude_desktop_stay_distinct() {
        // The CLI runs as `claude`, the desktop app as `Claude`. Process
        // matching is case-insensitive, so the two must be separated by using
        // rules that cannot collide.
        let cli_only = Observed {
            processes: vec!["claude".into()],
            config_paths: vec![".claude".into()],
            ..Default::default()
        };
        assert_eq!(detected_ids(&match_tools(&cli_only)), vec!["claude_code"]);

        let desktop_only = Observed {
            apps: vec!["Claude".into()],
            processes: vec!["Claude Helper (Renderer)".into()],
            ..Default::default()
        };
        assert_eq!(
            detected_ids(&match_tools(&desktop_only)),
            vec!["claude_desktop"]
        );
    }

    #[test]
    fn windows_display_name_suffixes_still_match() {
        let observed = Observed {
            apps: vec!["Cursor (User)".into()],
            ..Default::default()
        };
        assert_eq!(detected_ids(&match_tools(&observed)), vec!["cursor"]);
    }

    #[test]
    fn windows_executable_extensions_are_stripped() {
        let detected = match_tools(&observed_with_executables(&["ollama.exe", "codex.cmd"]));
        let ids = detected_ids(&detected);
        assert!(ids.contains(&"ollama"));
        assert!(ids.contains(&"openai_codex"));
    }

    #[test]
    fn unrelated_binaries_are_not_mistaken_for_agents() {
        // `codexplorer` and `ampersand` share prefixes with real tool names.
        let detected = match_tools(&observed_with_executables(&[
            "codexplorer",
            "ampersand",
            "gooseberry",
            "claudius",
            "ls",
            "git",
        ]));
        assert!(
            detected.is_empty(),
            "false positives: {:?}",
            detected_ids(&detected)
        );
    }

    #[test]
    fn similar_process_names_do_not_match() {
        let observed = Observed {
            processes: vec!["codexplorer".into(), "claudia-helper".into()],
            ..Default::default()
        };
        assert!(match_tools(&observed).is_empty());
    }

    #[test]
    fn process_names_with_separators_do_match() {
        let observed = Observed {
            processes: vec!["ollama.exe".into()],
            ..Default::default()
        };
        assert_eq!(detected_ids(&match_tools(&observed)), vec!["ollama"]);
    }

    #[test]
    fn detects_editor_extensions_regardless_of_version_suffix() {
        let observed = Observed {
            extensions: vec![
                "anthropic.claude-code-2.1.218-darwin-arm64".into(),
                "github.copilot-1.350.0".into(),
                "saoudrizwan.claude-dev-3.30.1".into(),
                "james-yu.latex-workshop-10.16.1".into(),
            ],
            ..Default::default()
        };

        let ids = detected_ids(&match_tools(&observed));
        assert!(ids.contains(&"claude_code"));
        assert!(ids.contains(&"github_copilot"));
        assert!(ids.contains(&"cline"));
        assert_eq!(ids.len(), 3, "latex-workshop must not be flagged");
    }

    #[test]
    fn summary_separates_autonomous_tools_from_chat_uis() {
        let observed = Observed {
            apps: vec!["Claude".into(), "Ollama".into()],
            executables: vec![("codex".into(), "/usr/local/bin/codex".into())],
            ..Default::default()
        };

        let summary = summarise(&match_tools(&observed));

        assert_eq!(summary.detected, 3);
        // Only Codex can act on the machine; the desktop app and the local
        // runtime cannot.
        assert_eq!(summary.autonomous, 1);
    }

    #[test]
    fn nothing_installed_is_a_clean_empty_report() {
        let summary = summarise(&[]);
        assert_eq!(summary.detected, 0);
        assert_eq!(summary.autonomous, 0);
        assert!(summary.vendors.is_empty());
    }

    #[test]
    fn vendors_are_deduplicated_across_tools() {
        // Claude Desktop and Claude Code are both Anthropic; the headline should
        // say one vendor, not two.
        let observed = Observed {
            apps: vec!["Claude".into()],
            executables: vec![("claude".into(), "/usr/local/bin/claude".into())],
            ..Default::default()
        };

        let summary = summarise(&match_tools(&observed));

        assert!(summary.detected >= 2);
        assert_eq!(summary.vendors.len(), 1);
        assert!(summary.vendors.contains("Anthropic"));
    }
}
