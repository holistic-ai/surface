# Core concepts

Five ideas. Together they explain every number the dashboard shows, and — more
usefully — every place a number is qualified rather than stated.

## The scan

One run, four sections, in order: **tools**, **sites**, **usage**, and the
**cost** layer applied over usage. Everything is synchronous; three sections that
take milliseconds each do not justify carrying an async runtime.

What the scan does keep from a fleet-oriented ancestor is **failure isolation**.
Each section runs inside `std::panic::catch_unwind`, so a parser that meets a
shape nobody anticipated costs that one section and not the whole scan. The
release profile deliberately does *not* set `panic = "abort"` for this reason,
and a section that died is named in the footer instead of being silently empty.

The same principle applies to anything unreadable. "Could not read" is never
collapsed into "nothing there": a browser you lack permission for must not look
like a browser with no AI use in it.

## Detection, not judgement

A tool is detected through five channels, all filesystem or process inspection —
**no commands are ever executed**, so detection cannot be slow and cannot be
tricked into running the very tool it is inventorying:

| Channel | Example evidence |
|---|---|
| Executables on `PATH` | `claude`, `codex`, `aider` |
| Home-relative config paths | `~/.claude`, `~/.codex` |
| Installed applications | `Claude.app`, `LM Studio` |
| Editor extensions | `anthropic.claude-code`, `openai.chatgpt` |
| Running processes | `ollama`, `Claude Helper` |

Every detection carries the evidence that produced it, shown in the **FOUND BY**
column, so you can tell a leftover config directory from a running agent.

surface reports what is there and does not grade it. The one fact it does
highlight is the `autonomous` flag — whether a tool can execute code or take
actions on the machine on a model's behalf. A chat window that can only talk is
not the same exposure as an agent holding a shell, and that distinction is worth
a column. Whether any given tool *should* be there is a judgement surface has no
basis to make.

## The ledger

Token counting is naively wrong in two ways, both measured on a real corpus
during design, and the ledger — one JSON file in the state directory — exists to
fix both.

**Duplicates.** 18,268 Claude Code usage records reduce to 8,672 distinct
`(requestId, message.id)` pairs. Count them all and every figure roughly
doubles. 89 of those keys appear in more than one transcript file, so the dedup
set has to survive *between* scans rather than living for one pass.

**Re-reading.** The same corpus is 717 MB and entirely inside a 30-day window.
Parsing it every run is pure waste, so each source file carries a byte offset and
only the appended tail is read. That is the difference between the ~19 s first
run and the ~50 ms ones after it.

Both concerns are per-day, which conveniently bounds the file: pruning a day
outside the window drops its totals and its dedup keys together.

### Cumulative versus per-message

Sources do not agree on what a usage record means, and getting this wrong is an
order-of-magnitude error rather than a rounding one:

| Source | Records | How surface reads them |
|---|---|---|
| Claude Code (`~/.claude/projects`) | per-message deltas | summed, deduplicated by `(requestId, message.id)` |
| Codex (`~/.codex/sessions`) | a **running total** per session | only the increase since the last scan |
| OpenCode (`opencode.db`) | per-message deltas, disjoint parts | summed, deduplicated by message id |

Summing Codex's running totals gave 34,935,584 tokens against a true 2,274,321 —
a 15× overcount. Each source therefore declares its accumulation mode and the
runner respects it.

### Token kinds

Six counters are kept per day, tool and model: `input`, `output`, `cache_read`,
`cache_creation`, `reasoning` and `messages`. They are billed differently —
cache reads at cache rates, reasoning at output rates — which is why they are
never merged before pricing. The headline "tokens" figure is everything billable
added together.

## Prices at read time

The scan records token counts and model ids, and stops there. **Pricing happens
when the numbers are read**, not when they are counted, because a rate baked in
at count time is wrong the moment prices move and there is no way to correct it
afterwards. A newer price table re-prices your entire history for free.

Three sources, in order:

1. The **cached table** in the state directory, if less than 24 hours old.
2. The **built-in table** compiled into the binary, so a first run with no
   network still costs correctly instead of showing an empty Cost view.
3. The **network** — [LiteLLM's public price
   table](https://github.com/BerriAI/litellm) — when the cache is stale and
   `--offline` was not passed.

A stale cache beats no prices, and the built-in table beats both when neither is
available.

### Three cost states, not one number

A local Ollama model genuinely costs nothing. A model missing from the price
table costs an *unknown* amount. Both would compute to `$0.00`, so they are
tracked separately and reported differently:

| State | Shown as | Meaning |
|---|---|---|
| `known` | `$12.40` | Priced from the table |
| `local` | `local` | Ran locally; genuinely free |
| `unpriced` | `▲ unpriced` | Not in the table; cost unknown |

A total with unpriced models under it is prefixed `≥`, because it is a floor
rather than a figure. In `--json`, an unpriced cost serialises as `null` and
never as `0.0`.

## Repository attribution

Agents record where they ran as an absolute path. A path is the wrong identity to
keep: it carries your username and private directory layout, and it does not
join across machines — the same repository checked out to two different places
looks like two unrelated projects.

So during ingest a working directory is resolved to its git remote slug,
`owner/name`, read from `.git/config` — and **the path itself is discarded**.
This matters because the ledger is written to disk. Never stored: absolute
paths, your home directory or username, branch names, remote URL paths beyond
`owner/name`, credentials embedded in a remote URL, and any remote other than
`origin`.

Work that happened outside a repository with an `origin` remote lands in
`(unattributed)`. That is a real bucket that gets its own row, not a silent
omission — spend you cannot attribute is exactly the spend worth noticing.

## Next steps

- [The dashboard](../guide/dashboard.md) — the six views these ideas render into.
- [Costs](../guide/costs.md) — the four ways the money figures can mislead you.
- [Privacy](../guide/privacy.md) — the full list of what is and is not read.
- [Rust API](../reference/rust.md) — the module docs, generated from the source.
