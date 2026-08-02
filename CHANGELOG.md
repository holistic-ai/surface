# Changelog

Notable changes to surface, newest first. The format follows [Keep a
Changelog](https://keepachangelog.com/en/1.1.0/) and the versioning follows
[Semantic Versioning](https://semver.org).

While the version is below `1.0`, minor releases may contain breaking changes.
Those are always listed under **Breaking changes** — see
[Stability](https://holistic-ai.github.io/surface/reference/stability/) for what
is covered and what is not.

The release workflow reads the section for the tag being built and uses it as the
GitHub Release body, so a tag with no section here fails the build rather than
publishing empty notes.

## [Unreleased]

### Fixed

- **Codex usage is priced again on current Codex versions.** Newer Codex no
  longer names the model in `session_meta`; it lives on each turn's
  `turn_context` record and can change mid-session. The scan now follows those
  records, attributing every usage record to the model that actually produced
  it, instead of filing entire sessions under `unknown`/`▲ unpriced`. The
  session-header read is also no longer cut off at a fixed 64 KiB, which
  truncated exactly the line that names the model.
### Changed

- **The SPEND card says what its dollars are.** Its last qualifier now reads
  `at API list rates` instead of `all models priced` — the figure is list-rate
  arithmetic, not a bill, and the old line said nothing the absence of `≥`
  did not already say. With unpriced models the `▲ N model(s) unpriced`
  caveat still takes the line.
  
### Added

- **A SPEND card that prices seats, and a TOKEN COST card that prices tokens.**
  The Overview's old SPEND figure — the window's tokens at API list rates — now
  sits under the name it deserved, **TOKEN COST**. The **SPEND** card answers
  the other question: what the seats behind that usage actually cost per month.
  Figures come from `[cost.subscriptions]`, or failing that from the plan a
  tool's own transcripts name (Codex writes `rate_limits.plan_type` beside its
  token counts), priced at that plan's list rate and marked `≈` an estimate. A
  tool with usage but no figure makes the total `≥` a floor; knowing nothing,
  the card shows `–` and says how to configure it rather than guessing. The
  Cost view's subscription table picks up detected plans the same way, still
  suffixed `est`. Detected plans persist in the ledger (now version 5, so the
  first scan after upgrading re-reads transcripts once).
- **The Projects view names the tools behind each repository.** Each project
  row ends in a `TOOLS` column listing the tools whose sessions ran there,
  behind the same series texture the usage chart keys that tool by — so two
  rows like `HAI Neo` and `owner/hai-neo` finally read as "the Codex checkout"
  and "the Claude Code one". It replaces the share bar, which repeated TOKENS
  as decoration.

## [0.1.0] - 2026-07-28

First release.

### Added

- **Tool detection.** Recognises 18 AI tools from `PATH`, config directories,
  editor extensions, running processes and installed applications, and marks
  which of them can execute code on this machine.
- **Site detection.** Visit counts for 30 known AI domains across 10 browsers in
  the Chromium and Firefox families. The domain filter runs inside SQLite, so no
  other browsing history is ever loaded.
- **Token usage.** Reads the transcripts Claude Code, Codex and OpenCode already
  write, attributing tokens per day, tool, model and git repository.
- **Cost.** Prices usage against LiteLLM's public table, with an optional
  comparison against what you actually pay via `[cost.subscriptions]`.
- **Dashboard.** A terminal UI with Overview, Tools, Sites, Usage, Cost and Repos
  views; mouse and keyboard navigation; and charts groupable by day, week or
  month.
- **`--json`** for the full scan as a versioned payload, **`--offline`** to skip
  the one network request, **`--check`** to print resolved paths and settings,
  and **`--demo`** to explore the dashboard on mock data.
- **Incremental scans.** The first run reads every transcript; later runs read
  only appended bytes.
- **One-line installers** for macOS, Linux and Windows, which verify the download
  against the release's `SHA256SUMS` before installing. Prebuilt archives are
  published for six targets, and every archive carries a Sigstore build
  provenance attestation verifiable with `gh attestation verify`.

### Known limits

- **Unpriced is not free.** A model missing from the price table shows as
  `▲ unpriced` rather than `$0.00`, and any total above an unpriced model is
  marked `≥` because it is a floor, not an estimate.
- **Cache-creation tokens are priced at input rates** where a provider does not
  publish a separate cache-write rate, so those totals are also floors.
- **Prices are API list rates.** surface reads no account state and does not
  guess your plan.
- **Browser history needs the `sqlite` feature**, which is on by default but
  requires a C toolchain when building from source. A
  `--no-default-features` build drops the Sites view and OpenCode token counts —
  and reports them as unreadable rather than as zero.
- **Safari history needs Full Disk Access** and is reported as an unreadable
  browser rather than counted as no usage.
- **Windows on arm64** gets the x86-64 binary, which runs emulated. There is no
  native arm64 Windows build yet.
- **Coverage tables go stale.** A tool, domain or browser missing from them is
  simply not reported; nothing is misattributed.

[unreleased]: https://github.com/holistic-ai/surface/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/holistic-ai/surface/releases/tag/v0.1.0
