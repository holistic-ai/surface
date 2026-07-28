# Privacy

The point of this tool is to show you your own data, so it is deliberately narrow
about what it touches. This page is the specific list, not a reassurance.

## Nothing is transmitted

There is no server, no account and no telemetry. surface has nowhere to send
anything. The only outbound request in the whole program fetches the model price
table, and `--offline` skips it — a table compiled into the binary means costs
still work without it.

```sh
surface --offline    # zero network activity, by construction
```

## What is read, and what is not

| | Read | Never read |
|---|---|---|
| **Transcripts** | Token counts, model ids, timestamps, working directory (resolved to a slug) | Prompts, replies, tool output — any message content, at any setting |
| **Browser history** | Domain, lifetime visit count, last-seen date, for known AI domains only | URLs, paths, query strings, page titles, and anything at all about non-AI browsing |
| **Tools** | Presence of executables, config directories, apps, extensions, process names | File contents, credentials, API keys, tokens |
| **Repositories** | `origin` remote host, owner and name from `.git/config` | Absolute paths, branch names, other remotes, credentials embedded in a remote URL |

Two of those deserve the detail.

### Message content is not read at all

Not as a default that can be flipped — there is no setting. The transcript
parsers extract token counts and model names and never touch the prose. The
ancestor project this came from had an opt-in for reading content; surface does
not, which makes "no prose is ever read" unconditional rather than a preference.

### The browser filter runs inside SQLite

The AI-domain filter is part of the SQL query, so non-AI history is never loaded
into the process in the first place — it is not read and then discarded, it is
never read. And for AI domains, the granularity stops at the host: a ChatGPT URL
carries a conversation id, and a tool that collected those would be a
surveillance feed rather than a usage meter.

There is a test that fails if a conversation id ever reaches a result.

`scan_history = false` turns the whole section off.

## No paths in the ledger

The ledger persists to disk, so what goes into it matters even though nothing
leaves the machine.

A working directory is resolved to its git remote slug — `owner/name` — during the
scan, and **the path itself is discarded**. Not stored: your home directory, your
username, the directory layout, branch names, raw session ids, or any remote other
than `origin`. Work outside a git repository becomes `(unattributed)` rather than
a path.

Two files are written, both in the state directory:

| File | Holds |
|---|---|
| `usage-ledger.json` | Daily token totals per tool, model and repo slug; dedup keys; byte offsets |
| `litellm-prices.json` | The cached price table |

Delete either at any time. The next scan rebuilds what it needs — a cold
transcript read once, then back to normal.

## Read-only and unprivileged

- surface **never elevates** and never prompts for a password.
- It **never writes outside its own state directory**.
- Browser databases are opened **read-only and immutable**, which is also how it
  reads a database Chrome has locked while running — no copy, no lock, no
  interference with the browser.
- Tool detection **executes nothing**. It is filesystem and process-list
  inspection only, so it cannot be tricked into running the very tool it is
  inventorying.

## Blind spots are reported, not hidden

What surface *cannot* see is shown rather than silently counted as zero. On macOS
that is usually Safari, whose history needs Full Disk Access:

```
┌ 12 AI domains · 6 profile(s) · 30 day lookback · ▲ unreadable: Safari ┐
```

The same rule runs through the whole program: "could not read" is never collapsed
into "nothing there". A browser you lack permission for must not look like a
browser with no AI use in it, a section that panicked is named in the footer, and
a build without SQLite says the Sites view was compiled out instead of showing an
empty list.

## Turning sections off

```toml
[web]
scan_history = false     # no browser history at all

[usage]
scan = false             # no transcripts at all
```

Tool detection has no off switch: it is the cheapest section, reads only whether
files exist, and is the thing surface exists to answer.

## Verifying any of this

The claims above are checkable, and reading the source is the point:

```sh
surface --json --offline | jq        # everything the scan collected, in full
cargo test                           # 246 tests, no fixture files and no network
```

The module-level documentation in the [Rust API reference](../reference/rust.md)
carries the design reasoning for each collector, including the measurements
behind these decisions.
