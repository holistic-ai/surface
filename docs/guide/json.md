# JSON output

`surface --json` runs the same scan as the dashboard, prints it as pretty JSON,
and exits. This is the form to pipe into `jq`, commit to a report, or feed to
whatever you already use.

```sh
surface --json --offline > scan.json
```

The payload is assembled for a reader rather than derived from the internal
types: the shape you want in `jq` is not the shape the views want to read.

## Top level

```json
{
  "surface": "0.1.0",
  "timings_ms": { "tools_ms": 6, "sites_ms": 331, "usage_ms": 52, "total_ms": 391 },
  "failed_sections": [],
  "tools": { "…": "…" },
  "sites": { "…": "…" },
  "usage": { "…": "…" },
  "prices": { "models": 1284, "age_secs": 7204 }
}
```

| Key | Meaning |
|---|---|
| `surface` | Version of the binary that produced this |
| `timings_ms` | Per-section and total scan duration |
| `failed_sections` | Sections that panicked and were skipped. Empty is normal |
| `prices.models` | Rows in the price table that was applied |
| `prices.age_secs` | Age of the cached table; `null` when the built-in table was used |

`failed_sections` is the one field worth asserting on in a script. A non-empty
array means a section produced no data for a reason that was not "nothing to
find".

## `tools`

```json
{
  "detected": 8,
  "autonomous": 5,
  "vendors": ["Anthropic", "OpenAI", "Ollama"],
  "list": [
    {
      "id": "claude_code",
      "name": "Claude Code",
      "vendor": "Anthropic",
      "kind": "coding_agent",
      "autonomous": true,
      "evidence": ["executable:/Users/you/.local/bin/claude", "config:~/.claude"]
    }
  ]
}
```

`kind` is one of `assistant`, `coding_agent`, `autonomous_agent`, `editor`,
`extension`, `local_runtime`. `autonomous` means the tool can execute code or
take actions on the machine on a model's behalf.

```sh
# Everything installed here that can act on the machine
surface --json | jq -r '.tools.list[] | select(.autonomous) | .name'
```

## `sites`

```json
{
  "sites": [
    {
      "domain": "claude.ai",
      "vendor": "Anthropic",
      "kind": "assistant",
      "visits": 317,
      "last_seen_unix": 1785100000,
      "last_seen": "2026-07-27T09:14:22Z"
    }
  ],
  "vendors": ["Anthropic", "OpenAI"],
  "domains_watched": 30,
  "browsers_scanned": ["chrome", "firefox"],
  "profiles_scanned": 6,
  "history_entries_scanned": 18422,
  "lookback_days": 30,
  "unreadable": [],
  "blind_spots": [
    { "browser": "safari", "name": "Safari", "reason": "insufficient_privileges" }
  ],
  "disabled": false
}
```

Three fields carry the caveats, and a script that ignores them will read a
permissions problem as an absence of AI usage:

| Field | Means |
|---|---|
| `disabled` | `scan_history = false`. Not "found nothing" |
| `unreadable` | A profile that was found but could not be read, and why |
| `blind_spots` | A browser installed but structurally unreadable — Safari without Full Disk Access |

`reason` values are stable strings meant to be matched on, not displayed:
`insufficient_privileges` (Safari without Full Disk Access),
`no_history_database`, `tool_unavailable`.

`visits` is the browser's **lifetime** count for the domain; `lookback_days` only
decides which domains appear. `last_seen` is `null` when the row carried no usable
timestamp, which is deliberately not the same as never visited.

Without the `sqlite` feature this becomes a marker instead of a result, and again
says so rather than looking empty:

```json
{ "compiled_in": false, "reason": "tool_unavailable", "detail": "built without the sqlite feature" }
```

## `usage`

```json
{
  "window_days": 30,
  "disabled": false,
  "tools_read": ["claude_code", "codex"],
  "sources_read": 214,
  "bytes_read": 12693,
  "unreadable": [],
  "duplicates_skipped": 9596,
  "totals_by_tool": {
    "claude_code": {
      "input": 1043221,
      "output": 8894120,
      "cache_read": 1785340221,
      "cache_creation": 42119883,
      "reasoning": 0,
      "messages": 8472,
      "total": 1837397445
    }
  },
  "days": [
    {
      "date": "2026-07-27",
      "tool": "claude_code",
      "model": "claude-opus-5",
      "input": 34221,
      "output": 291204,
      "cache_read": 58113402,
      "cache_creation": 1402113,
      "reasoning": 0,
      "messages": 284,
      "cost": { "state": "known", "usd": 41.88 }
    }
  ],
  "by_repo": [
    {
      "repo": "acme/platform",
      "tokens": 1204338211,
      "messages": 5218,
      "cost_usd": 612.4,
      "unpriced_models": 0
    },
    {
      "repo": "(unattributed)",
      "tokens": 88213004,
      "messages": 611,
      "cost_usd": 121.9,
      "unpriced_models": 2
    }
  ]
}
```

`days` is the long form — one row per `(date, tool, model)` — and the natural
thing to aggregate yourself. `bytes_read` is how much was read *this run*, which
is small precisely because the ledger tracks byte offsets;
`duplicates_skipped` is how many records were dropped as already-counted.

`by_repo` keys on the git remote slug, never a path. `unpriced_models` on a row
means `cost_usd` is a floor.

## Cost states

Every `cost` object is one of three states, and never a bare number:

```json
{ "state": "known",    "usd": 41.88 }
{ "state": "local",    "usd": 0.0 }
{ "state": "unpriced", "usd": null }
```

An unpriced model serialises as `null`, **never** `0.0` — see
[Costs](costs.md#unpriced-is-not-free). Any arithmetic you do downstream should
decide explicitly what to do with `null` rather than let it coerce to zero.

## Recipes

```sh
# Total spend, ignoring anything unpriced
surface --json | jq '[.usage.days[].cost.usd | select(. != null)] | add'
```

```sh
# Spend per repository, biggest first
surface --json | jq -r '.usage.by_repo[] | "\(.cost_usd)\t\(.repo)"' | sort -rn
```

```sh
# Models we cannot price — the list to check when a total shows ≥
surface --json | jq -r '[.usage.days[] | select(.cost.state == "unpriced") | .model] | unique[]'
```

```sh
# Which browsers could not be read
surface --json | jq -r '.sites.blind_spots[] | "\(.name): \(.reason)"'
```

```sh
# Fail a check if any section panicked
surface --json --offline | jq -e '.failed_sections | length == 0' > /dev/null
```

## Stability

The JSON shape is versioned along with the crate, and pre-1.0 it can change in a
minor release. The `surface` field in every payload tells you which version wrote
it. See [Stability](../reference/stability.md#the-json-payload).
