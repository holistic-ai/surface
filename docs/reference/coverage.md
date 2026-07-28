# Coverage

What surface knows how to recognise: **18 AI tools**, **30 AI domains**, **10
browsers** and **3 token sources**.

Every table here is best-effort and expected to go stale — this space moves fast.
A tool, domain or browser absent from a table is simply not reported; nothing
else breaks, and nothing is silently misattributed.

## AI tools

`CAN ACT` means the tool can execute code or take actions on this machine on a
model's behalf. It is the fact worth knowing, and the only judgement surface
makes.

| | Tool | Vendor | Kind | Can act | Id |
|:-:|---|---|---|:-:|---|
| ![](../assets/tools/claude_code.png){ width="22" height="22" } | Claude Code | Anthropic | coding agent | ✅ | `claude_code` |
| ![](../assets/tools/claude_desktop.png){ width="22" height="22" } | Claude Desktop | Anthropic | assistant | | `claude_desktop` |
| ![](../assets/tools/openai_codex.png){ width="22" height="22" } | Codex CLI | OpenAI | coding agent | ✅ | `openai_codex` |
| ![](../assets/tools/chatgpt_desktop.png){ width="22" height="22" } | ChatGPT Desktop | OpenAI | assistant | | `chatgpt_desktop` |
| ![](../assets/tools/opencode.png){ width="22" height="22" } | OpenCode | SST | coding agent | ✅ | `opencode` |
| ![](../assets/tools/openclaw.png){ width="22" height="22" } | OpenClaw | OpenClaw | autonomous agent | ✅ | `openclaw` |
| ![](../assets/tools/cursor.png){ width="22" height="22" } | Cursor | Anysphere | editor | ✅ | `cursor` |
| ![](../assets/tools/windsurf.png){ width="22" height="22" } | Windsurf | Codeium | editor | ✅ | `windsurf` |
| ![](../assets/tools/github_copilot.png){ width="22" height="22" } | GitHub Copilot | GitHub | extension | | `github_copilot` |
| ![](../assets/tools/aider.png){ width="22" height="22" } | Aider | Aider | coding agent | ✅ | `aider` |
| ![](../assets/tools/goose.png){ width="22" height="22" } | Goose | Block | autonomous agent | ✅ | `goose` |
| ![](../assets/tools/hermes_agent.png){ width="22" height="22" } | Hermes Agent | Nous Research | autonomous agent | ✅ | `hermes_agent` |
| ![](../assets/tools/gemini_cli.png){ width="22" height="22" } | Gemini CLI | Google | coding agent | ✅ | `gemini_cli` |
| ![](../assets/tools/amp.png){ width="22" height="22" } | Amp | Sourcegraph | coding agent | ✅ | `amp` |
| ![](../assets/tools/cline.png){ width="22" height="22" } | Cline | Cline | extension | ✅ | `cline` |
| ![](../assets/tools/continue.png){ width="22" height="22" } | Continue | Continue | extension | ✅ | `continue` |
| ![](../assets/tools/ollama.png){ width="22" height="22" } | Ollama | Ollama | local runtime | | `ollama` |
| ![](../assets/tools/lm_studio.png){ width="22" height="22" } | LM Studio | LM Studio | local runtime | | `lm_studio` |

### How each is detected

Five channels, all filesystem or process inspection. **No commands are ever
executed** — detection cannot be slow, and cannot be tricked into running the
tool it is inventorying. Every detection carries its evidence into the
`FOUND BY` column and the `evidence` array in `--json`.

| Tool | Executable | Config path | App | Process |
|---|---|---|---|---|
| Claude Code | `claude` | `~/.claude`, `~/.claude.json` | Claude Code URL Handler | `claude` |
| Claude Desktop | | | Claude | `Claude Helper` |
| Codex CLI | `codex` | `~/.codex` | | `codex` |
| ChatGPT Desktop | | | ChatGPT | `ChatGPT` |
| OpenCode | `opencode` | `~/.opencode`, `~/.config/opencode` | | `opencode` |
| OpenClaw | `openclaw`, `clawdbot`, `moltbot` | `~/.openclaw`, `~/.clawdbot`, `~/.moltbot`, `~/.config/openclaw` | OpenClaw | `openclaw`, `clawdbot`, `moltbot` |
| Cursor | `cursor` | `~/.cursor` | Cursor | `Cursor` |
| Windsurf | `windsurf` | `~/.windsurf`, `~/.codeium` | Windsurf | `Windsurf` |
| GitHub Copilot | `copilot` | `~/.config/github-copilot` | | |
| Aider | `aider` | `~/.aider.conf.yml` | | `aider` |
| Goose | `goose` | `~/.config/goose` | Goose | `goose` |
| Hermes Agent | | `~/.hermes` | | `hermes` |
| Gemini CLI | `gemini` | `~/.gemini` | | `gemini` |
| Amp | `amp` | `~/.config/amp` | | |
| Cline | | | | |
| Continue | `cn` | `~/.continue` | | |
| Ollama | `ollama` | `~/.ollama` | Ollama | `ollama` |
| LM Studio | `lms` | `~/.lmstudio`, `~/.cache/lm-studio` | LM Studio | `LM Studio` |

A fifth channel, editor extensions, is matched on id prefixes —
`anthropic.claude-code`, `openai.chatgpt`, `github.copilot`,
`saoudrizwan.claude-dev`, `continue.continue` — which is how Cline and Copilot are
found even with nothing on `PATH`.

**Hermes Agent has no executable listed on purpose.** Its command is `hermes`,
and so is React Native's Hermes JavaScript engine. One channel is enough to
report a tool, so matching that name would announce an autonomous agent on the
machine of anyone who has the JS engine on `PATH`. `~/.hermes` is created when the
agent initialises and belongs to nothing else, so it carries the detection
instead. The cost is that an installed-but-never-run Hermes goes unreported — the
right way round, since a missed tool is a gap and a wrong one is a lie.

Two details that are deliberate rather than accidental:

- Executables are matched **exactly** (Windows extensions stripped), and app
  names too: `Claude` must not claim `Claude Code URL Handler`, which belongs to a
  different row.
- The process match for Claude Desktop is `Claude Helper`, not `Claude` — process
  matching is case-insensitive and Claude Code's binary is `claude`.

## AI domains

Matched on the registrable host, so `chatgpt.com` covers `www.chatgpt.com` but
never `notchatgpt.com` or `chatgpt.com.example.test`. Add your own — an internal
model gateway, say — with
[`extra_ai_domains`](../guide/configuration.md#web).

| Domain | Vendor | Kind |
|---|---|---|
| chatgpt.com | OpenAI | assistant |
| chat.openai.com | OpenAI | assistant |
| openai.com | OpenAI | model API |
| sora.com | OpenAI | assistant |
| claude.ai | Anthropic | assistant |
| anthropic.com | Anthropic | model API |
| gemini.google.com | Google | assistant |
| aistudio.google.com | Google | model API |
| notebooklm.google.com | Google | assistant |
| copilot.microsoft.com | Microsoft | assistant |
| perplexity.ai | Perplexity | assistant |
| poe.com | Quora | assistant |
| character.ai | Character.AI | assistant |
| meta.ai | Meta | assistant |
| grok.com | xAI | assistant |
| x.ai | xAI | model API |
| mistral.ai | Mistral | model API |
| deepseek.com | DeepSeek | assistant |
| openclaw.ai | OpenClaw | agent |
| manus.im | Manus | agent |
| devin.ai | Cognition | agent |
| replit.com | Replit | dev tool |
| v0.dev | Vercel | dev tool |
| bolt.new | StackBlitz | dev tool |
| lovable.dev | Lovable | dev tool |
| cursor.com | Anysphere | dev tool |
| huggingface.co | Hugging Face | model hub |
| civitai.com | Civitai | model hub |
| midjourney.com | Midjourney | assistant |
| elevenlabs.io | ElevenLabs | assistant |

Only the domain, a lifetime visit count and a last-seen date are read, and the
filter runs inside SQLite so nothing else is loaded at all — see
[Privacy](../guide/privacy.md#the-browser-filter-runs-inside-sqlite).

## Browsers

| Browser | Id | Family | History |
|---|---|---|---|
| Google Chrome | `chrome` | Chromium | readable |
| Microsoft Edge | `edge` | Chromium | readable |
| Brave | `brave` | Chromium | readable |
| Arc | `arc` | Chromium | readable |
| Vivaldi | `vivaldi` | Chromium | readable |
| Opera | `opera` | Chromium | readable |
| Chromium | `chromium` | Chromium | readable |
| Firefox | `firefox` | Firefox | readable |
| Zen Browser | `zen` | Firefox | readable |
| Safari | `safari` | Restricted | needs Full Disk Access |

Every profile of every installed browser is discovered and read; the count is in
the Sites panel title and in `sites.profiles_scanned`. Databases are opened
**read-only and immutable**, which is also how a database Chrome holds a lock on
gets read — no copy, no lock, no interference.

Safari is listed as a **blind spot** rather than as zero usage when its history
cannot be read. That distinction is the whole point: a browser you lack
permission for must not look like a browser with no AI use in it.

## Token sources

| Source | Location | Format | Records |
|---|---|---|---|
| Claude Code | `~/.claude/projects/**/*.jsonl` | JSONL | per-message deltas |
| Codex | `~/.codex/sessions/**/*.jsonl` | JSONL | a running total per session |
| OpenCode | `~/.local/share/opencode/opencode.db`, `~/Library/Application Support/opencode/opencode.db` | SQLite | per-message deltas |

Deltas are summed and deduplicated; the running total is differenced. Getting
that wrong is a 15× error, not a rounding one — see [Cumulative versus
per-message](../getting-started/concepts.md#cumulative-versus-per-message).

OpenCode needs the `sqlite` feature. Without it, a present-but-unread store is
reported as `unreadable` with reason `tool_unavailable` rather than as zero usage.

Tools with no token source are still **detected** — they just contribute no usage
rows, because they write no transcript `surface` can read.

## Model prices

[LiteLLM's public price table](https://github.com/BerriAI/litellm), cached for 24
hours, with a copy compiled into the binary as a fallback. A model missing from
the table is `unpriced`, never free. See [Costs](../guide/costs.md).

## Adding to a table

All four tables are `const` arrays in the source, and adding a row is a small,
self-contained change:

| Table | File |
|---|---|
| `AI_TOOLS` | `src/scan/tooling.rs` |
| `AI_DOMAINS` | `src/scan/sites.rs` |
| `BROWSERS` | `src/browser.rs` |
| Token sources | `src/scan/usage.rs` |

A new token source also has to declare its accumulation mode, which is the part
worth reviewing carefully.
