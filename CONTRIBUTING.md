# Contributing

Thanks for looking. surface is a single binary with no build system beyond cargo —
`git clone`, `cargo run`, and you are working.

## Get CI passing locally

These are exactly the checks CI runs. Getting them green before you open a pull
request saves a round trip:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 246 tests, no fixtures and no network
cargo run -- --json --offline           # the scan must degrade, never fail
```

Two more that are easy to forget, because they cover the build configuration for
people without a C toolchain:

```sh
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo test --workspace --no-default-features
```

`--no-default-features` drops bundled SQLite, and with it browser history and
OpenCode token counts. Both modules disappear behind `#[cfg]`, so it is genuinely
possible to break that build without noticing — CI checks it, and so should you if
you touched `src/scan/` or `src/ui/`.

Needs Rust 1.88 or newer. That floor comes from ratatui's proc-macro chain rather
than from anything in `src/`, and CI pins it.

## What makes a change easy to accept

- **Tests live beside the code they exercise**, in a `mod tests` at the foot of
  the file. They point `SURFACE_STATE_DIR` at a temp directory, use no fixture
  files and touch no network. Keep it that way.
- **A comment should say why, not what.** The existing ones explain tradeoffs and
  the reasons behind non-obvious choices; match that.
- **The scan must degrade rather than fail.** Every section runs inside
  `catch_unwind`, and something unreadable is reported as unreadable — never
  silently counted as zero. If you add a source, add its failure path too.
- **Nothing is transmitted.** The only outbound request is the price table. If a
  change would add another, it needs discussion first.
- **No message content, URLs, paths or page titles** may reach the `--json`
  payload, the ledger or the screen. There are tests that fail if they do.

## Adding a tool, domain or browser

These are the most common contributions and the easiest to get right: the coverage
tables in `src/scan/tooling.rs`, `src/scan/sites.rs` and `src/browser.rs` are
plain data. Add the row, add the detection path, and update the corresponding
table in `docs/reference/coverage.md` and the README. A new row is a minor
release, not a breaking change.

## Docs

Material for MkDocs, with the rustdoc reference folded in by
`scripts/rustdoc_hook.py`:

```sh
uv venv && uv pip install -r docs/requirements.txt
uv run mkdocs serve                     # http://127.0.0.1:8000/surface/
uv run mkdocs build --strict            # what CI runs; fails on a broken link
```

Conventions for prose are in
[writing docs](https://holistic-ai.github.io/surface/guide/writing-docs/).

## Reporting things

Bugs and feature requests go in
[issues](https://github.com/holistic-ai/surface/issues). Security problems do
not — see [SECURITY.md](SECURITY.md).

For a bug, `surface --check` prints the resolved paths and settings, and
`surface --json --offline` gives the full scan. Both are safe to attach: neither
contains message content, URLs or paths.
