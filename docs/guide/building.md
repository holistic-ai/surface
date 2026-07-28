# Building from source

```sh
git clone https://github.com/holistic-ai/surface
cd surface
cargo build --release
cargo test
```

The binary lands at `target/release/surface`.

If you are setting up to *contribute* rather than to get a binary, start with
[Development install](../getting-started/installation.md#development-install) —
the clone-to-first-run steps and the four commands to get passing before a pull
request. This page is the detail underneath that.

## Requirements

| | |
|---|---|
| **Rust** | 1.88 or newer — set by ratatui's `instability` proc-macro, via darling 0.23 |
| **C toolchain** | Only for the default `sqlite` feature. See below |
| **Network** | For `cargo fetch`. The program itself needs none |

The MSRV is a promise, and CI builds on exactly 1.88 to keep it one.

## The one C dependency

SQLite, compiled from source by `rusqlite`'s `bundled` feature. It is the only
thing standing between this crate and trivial cross-compilation, so it is
opt-out rather than mandatory:

```sh
cargo build --no-default-features    # pure Rust, no C compiler needed
```

Two things need it — browser history (Chromium and Firefox both store it in
SQLite) and OpenCode's token history — and the reduced build says so rather than
showing empty views. The trade-off table is in
[Installation](../getting-started/installation.md#without-sqlite).

Released binaries are built with the feature on, so nobody installing a binary
compiles C.

## The release profile

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
```

Size over speed: the work is I/O-bound, so an optimiser tuned for a hot loop
buys nothing a scan can feel.

!!! warning "`panic = \"abort\"` is deliberately absent"
    Each section of the scan runs inside `catch_unwind`, so a parser that trips
    over an unfamiliar shape degrades that one section instead of taking the whole
    scan down. `abort` would turn that design into a crash — see [Core
    concepts](../getting-started/concepts.md#the-scan).

## Tests

```sh
cargo test                          # 246 tests
cargo test --no-default-features    # the pure-Rust path
```

No fixture files and no network. Tests that need a home directory or a state
directory build one in a temp directory and point `SURFACE_STATE_DIR` /
`SURFACE_CONFIG_DIR` at it, so a test run never touches your real profile and
never depends on what happens to be installed on the machine.

Three classes of test are worth knowing about before changing anything:

- **Privacy tests.** They fail if a conversation id, an absolute path or a
  branch name reaches a result. These are the tests that make the claims in
  [Privacy](privacy.md) checkable rather than aspirational.
- **Render tests.** Every view is drawn at every plausible terminal size,
  including a completely empty scan and a 40×10 terminal. ratatui panics on a
  layout that does not fit, so this catches the most likely runtime failure.
- **Arithmetic tests.** Regrouping a chart must never change the grand total,
  and an unpriced model must never render as `$0.00`.

## Lints

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI runs both with `RUSTFLAGS: -D warnings`, so a warning is a build failure
there.

## What CI checks

| Job | Does |
|---|---|
| **lint** | `cargo fmt --check` and `cargo clippy -D warnings`, the latter twice — once with default features and once without |
| **test** | `cargo test` on macOS, Windows and Ubuntu |
| **smoke** | `surface --json --offline` and `--check` on a runner with no AI tools at all |
| **no-sqlite** | `cargo test --no-default-features`, so that install path keeps working — tested rather than merely built, because the point of the flag is that the fallbacks behave |
| **cross** | Builds `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`, the two targets that compile bundled SQLite with a cross C toolchain |
| **msrv** | `cargo build --locked` on Rust 1.88 |
| **docs** | `mkdocs build --strict`, then publish to GitHub Pages |

The smoke test matters more than it looks: a CI runner is a machine with no AI
tools, no browser profiles and no transcripts, which is exactly the case where a
scan must degrade rather than fail. Empty output is a pass; a non-zero exit is
not.

**cross** exists for a narrower reason. Those two targets are the only ones whose
build can fail for reasons a native build never sees — a missing cross linker, or
a `CC_<target>` that sends the SQLite amalgamation to the host compiler. They are
also targets the release cannot do without, and the release builds all six in one
job that any single failure takes down. Proving them on every push is cheaper than
discovering it during a tag build.

## Building the docs

The site you are reading is Material for MkDocs, and the Rust API reference is
folded in by an MkDocs hook that runs `cargo doc`. See [Writing
docs](writing-docs.md).
