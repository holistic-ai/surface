# CLI

```
surface [OPTIONS]

What AI runs here, and what it costs.

Options:
      --json     Print the scan as JSON and exit, instead of opening the dashboard
      --offline  Never fetch model prices; use the cache, then the built-in table
      --check    Print resolved paths and settings, then exit without scanning
      --demo     Open the dashboard on mock data, without scanning this machine
  -h, --help     Print help (see more with '--help')
  -V, --version  Print version
```

No arguments, no subcommands, no positional paths. surface scans this machine and
nothing else.

## Flags

### `--json`

Runs the full scan, prints it as pretty JSON on stdout, exits. Nothing else is
written to stdout, so it pipes cleanly:

```sh
surface --json | jq '.usage.by_repo'
```

The payload is documented field by field in [JSON output](../guide/json.md).

### `--offline`

Suppresses the one network request in the program. Prices then come from the
cached table, and from the table compiled into the binary when there is no cache.
The Cost view labels the latter `built-in price table`, since those rates are as
old as the release.

Everything else is unaffected — the scan reads local files either way.

### `--check`

Prints what surface resolved and exits **without scanning**. The fastest way to
answer "where does it look for my config" and "did it read it":

```sh
$ surface --check
state dir   /Users/you/Library/Application Support/ai.holistic.surface
config dir  /Users/you/Library/Application Support/ai.holistic.surface
config      loaded
ledger      /Users/you/Library/Application Support/ai.holistic.surface/usage-ledger.json
prices      1284 models, cached 2h ago
sites       30 day lookback
usage       30 day window
```

| Line | Says |
|---|---|
| `state dir` | Where the ledger and price cache live |
| `config dir` | Where `surface.toml` is looked for |
| `config` | `loaded`, or `defaults (no file)` |
| `ledger` | Full path to the ledger file |
| `prices` | Model count, and the cache age — or `(built in; no cache, no network)` |
| `sites` | The lookback window, `disabled in config`, or `not compiled in (built without the sqlite feature)` |
| `usage` | The accounting window, or `disabled in config` |

`--check` honours `--offline`, so `surface --check --offline` never reaches the
network either.

### `--demo`

Opens the dashboard on mock data and **scans nothing** — no transcripts, no
browser history, no ledger read or written. For showing the tool on a machine
that has no AI tooling on it, for a screen recording, and for reproducing a
layout bug without having to arrange the data that caused it.

```sh
surface --demo            # the dashboard, populated
surface --demo --json     # the same mock scan as JSON, for fixtures
```

It is labelled everywhere it appears: the sidebar carries a `DEMO` badge for as
long as it is running, and the JSON payload reports `"demo": true` — a field that
is present and `false` on a real scan, so a consumer never has to know the flag
exists to be safe from it.

What the mock data is, precisely:

| | |
|---|---|
| **Tool and domain names** | Real. Taken from the same catalogues the detector uses, by id, so the demo cannot show a product that does not exist |
| **Counts** | Invented — 30 days of usage across four tools, 13 models and 16 repositories, each with its own lifecycle and rhythm |
| **Costs** | Computed, not invented. Fake tokens, real price table, so a demo dollar figure is what those tokens would genuinely have cost |
| **Warning states** | Deliberately present: one unpriced model, one local model, one unreadable browser profile, one unreadable transcript source |
| **Reproducibility** | Deterministic for a given day. Two runs produce the same dashboard, which is what makes it usable for screenshots |

The scan timings in the footer are fixed values, not measurements — nothing was
timed because nothing ran.

## Keys in the dashboard

| Key | Does |
|---|---|
| <kbd>tab</kbd>, <kbd>→</kbd>, <kbd>l</kbd> | Next view |
| <kbd>shift-tab</kbd>, <kbd>←</kbd>, <kbd>h</kbd> | Previous view |
| <kbd>1</kbd>–<kbd>6</kbd> | Jump to a view |
| <kbd>j</kbd>/<kbd>k</kbd>, <kbd>↓</kbd>/<kbd>↑</kbd> | Move the selection |
| <kbd>g</kbd>/<kbd>G</kbd>, <kbd>home</kbd>/<kbd>end</kbd> | First / last row |
| <kbd>page up</kbd>/<kbd>page down</kbd> | Move ten rows |
| Mouse wheel | Move three rows |
| Click a view in the sidebar | Jump to it |
| Click a table row | Select it, and give its pane the keyboard |
| <kbd>enter</kbd> | Switch pane, on a view that has two |
| <kbd>d</kbd> | Token detail line under each row, on and off |
| <kbd>u</kbd> | Switch the Overview chart between spend and tokens |
| <kbd>[</kbd>/<kbd>]</kbd> | Move the chart cursor one bucket |
| <kbd>backspace</kbd> | Drop the chart cursor |
| <kbd>w</kbd> | Regroup the charts by day, week or month |
| <kbd>?</kbd> | Help overlay (any key closes it) |
| <kbd>q</kbd>, <kbd>esc</kbd>, <kbd>ctrl-c</kbd> | Quit |

Full descriptions in [The dashboard](../guide/dashboard.md#keys).

## Environment variables

| Variable | Effect |
|---|---|
| `SURFACE_STATE_DIR` | Override the state directory (ledger, price cache) |
| `SURFACE_CONFIG_DIR` | Override where `surface.toml` is looked for |
| `SURFACE_SCAN_HISTORY` | `false` disables the browser-history section |
| `SURFACE_HISTORY_LOOKBACK_DAYS` | Domain lookback window |
| `SURFACE_SCAN_USAGE` | `false` disables the transcript section |
| `SURFACE_USAGE_WINDOW_DAYS` | Token accounting window |

Booleans accept `1`/`true`/`yes`/`on` and `0`/`false`/`no`/`off`; anything else is
ignored rather than read as false. Day counts are clamped to 1–3650.

Environment variables beat the config file; CLI flags beat both.

## Exit codes

| Code | Means |
|---|---|
| `0` | The scan completed. Includes a scan that found nothing, and one where a section failed and was reported |
| non-zero | surface could not start or could not restore the terminal — a bad config file, an unwritable state directory, a terminal that refused raw mode |

A section that panics does **not** change the exit code: it is named in the
footer, or in `failed_sections` under `--json`. If you want that to be an error,
assert on it:

```sh
surface --json --offline | jq -e '.failed_sections | length == 0' > /dev/null
```

An unknown key in `surface.toml` *is* a startup error. A misspelled setting that
silently does nothing is the worse outcome.

## Files

| Path | Written | Holds |
|---|---|---|
| `<state>/usage-ledger.json` | Every scan | Daily token totals, dedup keys, byte offsets |
| `<state>/litellm-prices.json` | When the price table is fetched | The cached rates |
| `<config>/surface.toml` | Never — read only | Your settings |

Platform defaults for `<state>` and `<config>` are in
[Installation](../getting-started/installation.md#where-surface-keeps-its-files).
surface writes nothing anywhere else.
