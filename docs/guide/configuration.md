# Configuration

Every setting is optional. surface works with nothing but the binary, and a
missing config file is the normal case rather than an error — so this page is
really a list of the four things worth changing.

## Where the file goes

`surface.toml`, in the config directory:

| | Path |
|---|---|
| **macOS** | `~/Library/Application Support/ai.holistic.surface/surface.toml` |
| **Linux** | `~/.config/surface/surface.toml` |
| **Windows** | `%APPDATA%\holistic\surface\config\surface.toml` |

`surface --check` prints the resolved directory on this machine, which beats
guessing:

```sh
surface --check
```

## Precedence

Later wins:

1. Built-in defaults
2. `surface.toml`
3. `SURFACE_*` environment variables
4. CLI flags (`--offline`)

## The whole file

Copy this and delete what you do not need — every value shown is the default.

```toml title="surface.toml"
--8<-- "surface.example.toml"
```

## `[web]`

Browser history, for known AI domains only.

| Key | Default | Does |
|---|---|---|
| `scan_history` | `true` | Read browser history at all |
| `history_lookback_days` | `30` | How recently a domain must have been visited to be listed |
| `extra_ai_domains` | `[]` | Extra domains to treat as AI services |

```toml
[web]
scan_history = true
history_lookback_days = 90
extra_ai_domains = ["models.corp.example", "llm-gateway.internal"]
```

`extra_ai_domains` is matched on the registrable host, so
`models.corp.example` also covers `www.models.corp.example` but never
`notmodels.corp.example`. An internal model gateway is the usual reason to set
it.

Setting `scan_history = false` makes the [Sites view](dashboard.md#sites) report
that it is switched off, rather than showing an empty list.

!!! note "The lookback filters domains, not visit counts"
    Visit counts are the browser's lifetime totals. `history_lookback_days` only
    decides which domains make the list.

## `[usage]`

Token accounting from the transcripts your tools already write.

| Key | Default | Does |
|---|---|---|
| `scan` | `true` | Read transcripts at all |
| `window_days` | `30` | How many days of daily totals to keep |

```toml
[usage]
scan = true
window_days = 90
```

`window_days` is also the retention policy: days that fall outside it are pruned
from the ledger, along with their deduplication keys. Widening it does not
recover days already pruned — those transcripts will be re-read from scratch,
which costs a cold scan once.

Both day counts are **clamped to 1–3650, not rejected**. A nonsensical window is
a typo, and refusing to run over a typo is worse than running over a sane value.
An unknown *key*, on the other hand, is an error at startup: a misspelled setting
that silently does nothing is the worse failure.

## `[usage.repo_aliases]`

The same project legitimately earns two rows in the Projects view: a checkout
with an `origin` remote reports `owner/name`, while a copy of the same code
with no remote — a scratch workspace, an agent's own working folder — reports
its directory basename. surface never guesses that two names are one project,
because folding someone's spend together on a string resemblance is
misattribution. Declare it instead:

```toml
[usage.repo_aliases]
"HAI Neo" = "holistic-ai/hai-neo"
```

Keys are rows exactly as the Projects view shows them; values are the row to
fold them into. The grouping is applied when the ledger is *read*, never to
what is stored — like prices — so an alias added today regroups the whole
window retroactively, and a wrong one is one edit away from undone. Aliases do
not chase: an alias pointing at another alias folds one hop only.

## `[cost]`

surface prices tokens at API list rates. If you pay a flat subscription instead,
tell it what you actually pay and the [Cost
view](dashboard.md#subscription-comparison) will compare the two.

```toml
[cost.subscriptions]
claude_code = 100.0
codex = 30.0
```

Keys are tool ids as they appear in the Usage view — `claude_code`, `codex`,
`opencode`, `gemini_cli`, … — and values are **monthly** USD.

A tool with no entry falls back to the plan its own transcripts name, if any —
Codex writes one beside its token counts — priced at that plan's published list
rate. surface reads no account state, so a tool that names no plan and has no
entry gets no row rather than a guess. A configured figure always wins and is
used as given; a list-price fallback is labelled `est` wherever it is shown.

## Environment variables

Handy for a one-off run, and what the test suite uses to stay out of a real
profile.

| Variable | Overrides |
|---|---|
| `SURFACE_STATE_DIR` | Where the ledger and price cache live |
| `SURFACE_CONFIG_DIR` | Where `surface.toml` is looked for |
| `SURFACE_SCAN_HISTORY` | `[web] scan_history` |
| `SURFACE_HISTORY_LOOKBACK_DAYS` | `[web] history_lookback_days` |
| `SURFACE_SCAN_USAGE` | `[usage] scan` |
| `SURFACE_USAGE_WINDOW_DAYS` | `[usage] window_days` |

Booleans accept `1`/`true`/`yes`/`on` and `0`/`false`/`no`/`off`; anything else is
ignored rather than treated as false.

=== "macOS / Linux"

    ```sh
    # Transcripts only — no browser history, nothing written to the real profile
    SURFACE_SCAN_HISTORY=false SURFACE_STATE_DIR=$(mktemp -d) surface --json
    ```

=== "Windows"

    ```powershell
    $env:SURFACE_SCAN_HISTORY = 'false'
    $env:SURFACE_STATE_DIR = (New-Item -ItemType Directory -Path "$env:TEMP\surface-scratch" -Force).FullName
    surface --json
    ```

    Those two assignments last for the current PowerShell session. Use
    `[Environment]::SetEnvironmentVariable('SURFACE_SCAN_HISTORY','false','User')`
    to make one stick, or `Remove-Item Env:SURFACE_SCAN_HISTORY` to undo it now.

## Recipes

??? example "Tools only — no history, no transcripts"

    ```toml
    [web]
    scan_history = false

    [usage]
    scan = false
    ```

    The scan then reports which AI tools are installed and nothing else. Tool
    detection has no off switch: it is the cheapest section and the one the tool
    exists for.

??? example "A quarter of history instead of a month"

    ```toml
    [web]
    history_lookback_days = 90

    [usage]
    window_days = 90
    ```

    Expect one slow scan while the extra transcripts are read, then the usual
    ~50 ms.

??? example "Never touch the network"

    Pass `--offline`, or make it permanent with a shell alias:

    ```sh
    alias surface='surface --offline'
    ```

    Prices then come from the cache, and from the built-in table when the cache
    is missing. There is no config key for this — a flag you can see beats a
    setting you have forgotten.
