# Quickstart

Nothing to configure, no account to make. Run it.

```sh
surface
```

That scans this machine and opens the dashboard. Press <kbd>q</kbd> to leave.

## What just happened

Four sections ran, in order, on local files only:

| Section | Reads | Typical time |
|---|---|---|
| **Tools** | `PATH`, config directories, installed apps, editor extensions, running processes | 6 ms |
| **Sites** | Browser history databases, for known AI domains only | 330 ms |
| **Usage** | The transcripts Claude Code, Codex and OpenCode already write | 50 ms warm |
| **Cost** | The cached model price table, applied to the tokens above | ~0 ms |

Each section runs inside its own `catch_unwind`, so a parser that trips over an
unfamiliar shape costs you that one section and not the scan. Anything that
failed is named in the footer.

!!! note "The first run is the slow one"
    A cold scan parses every transcript in the window — around 19 seconds on a
    900 MB corpus. Each later run reads only the bytes appended since, which is
    why the warm figure above is 50 ms. See [the
    ledger](concepts.md#the-ledger).

## Move around

<div class="site-grid" markdown>

<div class="site-card" markdown>
### Views
<kbd>tab</kbd> cycles, <kbd>1</kbd>–<kbd>6</kbd> jump straight to
Overview, Tools, Sites, Usage, Cost or Projects.
</div>

<div class="site-card" markdown>
### Rows
<kbd>j</kbd>/<kbd>k</kbd> move, <kbd>g</kbd>/<kbd>G</kbd> jump to the ends,
the mouse wheel scrolls.
</div>

<div class="site-card" markdown>
### Charts
<kbd>w</kbd> regroups by day, week or month. The grand total never changes —
only the buckets do.
</div>

</div>

<kbd>?</kbd> shows the full key list, <kbd>q</kbd> quits. Every key is listed in
[The dashboard](../guide/dashboard.md).

## Scan without the dashboard

`--json` prints the same scan and exits, which is the form to pipe somewhere:

```sh
surface --json | jq '.usage.totals_by_tool'
```

```json
{
  "claude_code": {
    "input": 1043221,
    "output": 8894120,
    "cache_read": 1785340221,
    "cache_creation": 42119883,
    "reasoning": 0,
    "messages": 8472,
    "total": 1837397445
  }
}
```

The full payload — tools, sites, per-day rows, per-repo spend and the three cost
states — is documented in [JSON output](../guide/json.md).

## Useful first commands

```sh
surface --check       # resolved paths and settings, then exit; no scan
surface --offline     # never touch the network; use the cache, then the built-in table
surface --json        # print the scan as JSON, then exit
```

Combine freely. A completely inert dry run, leaving the real profile untouched:

```sh
SURFACE_STATE_DIR=$(mktemp -d) surface --json --offline > scan.json
```

## Spend by repository

The Overview ranks spend three ways — by model, by tool and by repository — side
by side under its chart. The repository one is usually the first figure worth
acting on:

```
┌ by repository ───────────────────────────────┐
│acme/platform          ████████████████ $628.71│
│acme/webapp            ███████          $268.07│
│(unattributed)         ████             ≥$150.56│
│+ 11 more                               $289.46│
└──────────────────────────────────────────────┘
```

Three details in that picture matter:

- **`(unattributed)`** is work that happened outside a git repository with an
  `origin` remote. It is a real bucket, not a rounding error.
- **`≥`** marks a total with unpriced models under it. It is a floor, not a
  figure — see [Costs](../guide/costs.md#unpriced-is-not-free).
- **`+ N more`** carries the summed spend of everything below the top five, so the
  panel still accounts for the whole total. The
  [Projects](../guide/dashboard.md#projects) view lists every repository.

Attribution comes from the git remote, never the path: a working directory is
resolved to `owner/name` during the scan and the path is discarded. See
[Core concepts](concepts.md#repository-attribution).

## Next steps

- [Core concepts](concepts.md) — the five ideas that make the numbers legible.
- [The dashboard](../guide/dashboard.md) — every view, column and key.
- [Configuration](../guide/configuration.md) — narrow the window, add a domain,
  record what you actually pay.
- [Privacy](../guide/privacy.md) — exactly what is read, kept and never touched.
