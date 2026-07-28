# Installation

surface runs on **macOS, Linux and Windows**. It needs no runtime, no service and
no configuration; the binary is the whole install.

## Install

=== "macOS / Linux"

    ```sh
    curl -fsSL https://raw.githubusercontent.com/holistic-ai/surface/main/install.sh | sh
    ```

=== "Windows (PowerShell)"

    ```powershell
    irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex
    ```

    Works in both Windows PowerShell 5.1 and PowerShell 7.

=== "Windows (cmd)"

    ```bat
    powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex"
    ```

    `irm` and `iex` are PowerShell aliases and do not exist in `cmd.exe`, so the
    command above hands the job to PowerShell rather than running there.

=== "Cargo"

    ```sh
    cargo install surface-cli
    ```

    The crate is `surface-cli` but the binary it installs is `surface`.

    Compiles SQLite from C, so this one needs a C toolchain — see
    [without SQLite](#without-sqlite).

Then:

```sh
surface
```

## Or do it by hand

Prefer not to run a script? The archives are on the
[releases page](https://github.com/holistic-ai/surface/releases) — six targets,
each with a `SHA256SUMS` entry.

| Platform | Target | Archive |
|---|---|---|
| macOS, Apple silicon | `aarch64-apple-darwin` | `.tar.gz` |
| macOS, Intel | `x86_64-apple-darwin` | `.tar.gz` |
| Linux, x86-64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux, arm64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| Linux, x86-64, static | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Windows, x86-64 | `x86_64-pc-windows-msvc` | `.zip` |

```sh
tar -xzf surface-v0.1.0-aarch64-apple-darwin.tar.gz
shasum -a 256 -c SHA256SUMS --ignore-missing
install -m 755 surface-v0.1.0-aarch64-apple-darwin/surface ~/.local/bin/surface
```

The two `linux-gnu` archives are built on Ubuntu 22.04 and so need **glibc 2.35 or
newer**: Ubuntu 22.04, Debian 12 (2.36) and anything later. RHEL 9 and its
rebuilds sit just below at 2.34.

The `linux-musl` archive is statically linked and needs no libc at all, which
makes it the one to take on Alpine, on a distroless image, or on any distribution
below that floor — including RHEL 9. `install.sh` checks your glibc and picks it
for you, so on x86-64 this resolves itself. On arm64 there is no static build, so
an older arm64 distribution means building from source; the installer says so
rather than installing a binary that cannot start.

Two environment variables, if you need them:

```sh
SURFACE_VERSION=v0.1.0 sh install.sh     # a specific tag
SURFACE_INSTALL_DIR=~/bin sh install.sh  # somewhere else
```

On Windows the equivalents are `$env:SURFACE_VERSION` and
`$env:SURFACE_INSTALL_DIR`; the default there is `%LOCALAPPDATA%\Programs\surface`,
which the script adds to your user `PATH`. Set them before the install command in
the same session:

=== "PowerShell"

    ```powershell
    $env:SURFACE_INSTALL_DIR = 'C:\tools'
    irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex
    ```

=== "cmd"

    ```bat
    set SURFACE_INSTALL_DIR=C:\tools
    powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex"
    ```

Only the installer is PowerShell-only. Once it has run, `surface` itself is an
ordinary executable on your `PATH` and runs from cmd, PowerShell and Windows
Terminal alike — reopen the terminal first, so it picks up the new `PATH`.

## Verifying a download

Checking against `SHA256SUMS` proves the file arrived intact. It does not prove we
produced it, because the checksums are served from the same release as the
archives — anyone who could substitute one could substitute both.

Every archive therefore carries a [Sigstore](https://www.sigstore.dev) build
provenance attestation, which ties it to this repository, the workflow that built
it and the commit it was built from:

```sh
gh attestation verify surface-v0.1.0-aarch64-apple-darwin.tar.gz \
  --repo holistic-ai/surface
```

That needs the [GitHub CLI](https://cli.github.com) and, the first time, network
access to fetch the signing certificate. It is the check worth running if you are
putting surface into a build of your own; for an interactive install the
installer's checksum verification is the usual bar.

Or build it yourself:

```sh
git clone https://github.com/holistic-ai/surface
cd surface && cargo build --release
```

Details, including the minimum supported Rust version, are in [Building from
source](../guide/building.md).

!!! note "Platform notes"
    On musl (Alpine, distroless) take the `x86_64-unknown-linux-musl` archive; there
    is no static arm64 build, so arm64 Alpine still means building from source. The
    macOS binaries are not notarized yet, so an archive downloaded by a *browser*
    is quarantined — `xattr -d com.apple.quarantine surface` clears it. Windows on
    arm64 runs the x86-64 binary emulated; there is no native build yet. On
    Windows use Windows Terminal; the old `conhost` console does not draw the
    box-drawing characters the dashboard is made of.

No Homebrew tap yet.

## Development install

For working *on* surface rather than with it. There is no separate dev toolchain
to install: `cargo` is the whole thing.

```sh
git clone https://github.com/holistic-ai/surface
cd surface
cargo run                       # scan this machine, open the dashboard
```

`cargo run` builds a debug binary, which is slower on a cold transcript read but
otherwise identical. For realistic timings use `cargo run --release`.

### Requirements

| | |
|---|---|
| **Rust** | 1.88 or newer — the MSRV, built on exactly that version in CI |
| **C toolchain** | For the bundled SQLite. `xcode-select --install`, `build-essential`, or the MSVC C++ workload |
| **Python** | Only to build the docs site — see [Writing docs](../guide/writing-docs.md) |

No task runner, no code generation, no submodules, no `.env` to fill in.

### Before you open a pull request

The four commands CI runs. All four pass on a clean checkout:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test                              # 246 tests
cargo run -- --json --offline           # smoke test: must degrade, never fail
```

The last one matters more than it looks. A CI runner is a machine with no AI
tools, no browser profiles and no transcripts — exactly the case where a scan has
to produce empty output rather than an error. Empty is a pass; a non-zero exit is
not.

If you touched anything that changes what gets built:

```sh
cargo test --no-default-features        # 206 of the 246 tests; the rest need SQLite
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
uv run mkdocs build --strict            # docs, if you edited any
```

### Working against your own data safely

Point the state directory somewhere disposable and the real ledger is never
touched:

=== "macOS / Linux"

    ```sh
    SURFACE_STATE_DIR=$(mktemp -d) cargo run -- --offline
    ```

=== "Windows"

    ```powershell
    $env:SURFACE_STATE_DIR = "$env:TEMP\surface-dev"
    cargo run -- --offline
    ```

`SURFACE_CONFIG_DIR` does the same for configuration, so you can test a config
file without editing the one you use. `--offline` keeps a debug loop off the
network entirely.

### Serving the docs

```sh
uv venv
uv pip install -r docs/requirements.txt
uv run mkdocs serve                     # http://127.0.0.1:8000/surface/
```

Building the site runs `cargo doc` through an MkDocs hook, so cargo must be on
`PATH`. Details in [Writing docs](../guide/writing-docs.md).

More on the build itself — the release profile, what each CI job checks, the
three classes of test — is in [Building from source](../guide/building.md).

## Without SQLite

Browser history (Chromium and Firefox both store it in SQLite) and OpenCode's
token history are the two things that need SQLite, and it is the one dependency
that needs a C compiler. So it is a default feature rather than a mandatory one:

```sh
cargo install surface-cli --no-default-features   # pure Rust, no C compiler
```

| | Default build | `--no-default-features` |
|---|---|---|
| Size | ~2.06 MB | ~1.20 MB |
| [Sites view](../guide/dashboard.md#sites) | yes | not compiled in |
| OpenCode token counts | yes | reported as unreadable |
| Claude Code and Codex usage | yes | yes — plain JSONL, no SQLite involved |
| AI tool detection | yes | yes |

The reduced build says what it cannot see rather than showing empty views: the
Sites view reports that it was compiled out, and OpenCode appears as an
unreadable source rather than as zero usage. Released binaries are built with
the feature **on**, so installing one never compiles C.

## Verify the install

```sh
surface --version
surface --check       # resolved paths and settings, no scan
```

`--check` is the fastest way to confirm surface can find its own directories.
Typical output:

```
state dir   /Users/you/Library/Application Support/ai.holistic.surface
config dir  /Users/you/Library/Application Support/ai.holistic.surface
config      defaults (no file)
ledger      /Users/you/Library/Application Support/ai.holistic.surface/usage-ledger.json
prices      1284 models, cached 2h ago
sites       30 day lookback
usage       30 day window
```

## Where surface keeps its files

Two files, both in the state directory: the usage ledger
(`usage-ledger.json`, so the next scan reads only what was appended) and the
cached price table (`litellm-prices.json`). Nothing is written anywhere else,
ever.

| | State | Config |
|---|---|---|
| **macOS** | `~/Library/Application Support/ai.holistic.surface` | same as state |
| **Linux** | `~/.local/share/surface` | `~/.config/surface` |
| **Windows** | `%LOCALAPPDATA%\holistic\surface\data` | `%APPDATA%\holistic\surface\config` |

These are whatever the [`directories`](https://docs.rs/directories) crate reports
for this platform, which is why macOS gets a reverse-DNS directory and Linux does
not. `surface --check` prints the resolved pair, and that output is the
authority — never a guess from this table.

Both are overridable with `SURFACE_STATE_DIR` and `SURFACE_CONFIG_DIR` — handy
for a throwaway run that leaves the real profile untouched:

=== "macOS / Linux"

    ```sh
    SURFACE_STATE_DIR=$(mktemp -d) surface --json --offline
    ```

=== "Windows"

    ```powershell
    $env:SURFACE_STATE_DIR = "$env:TEMP\surface-scratch"
    surface --json --offline
    ```

## Uninstall

=== "macOS"

    ```sh
    sudo rm /usr/local/bin/surface                            # or: cargo uninstall surface-cli
    rm -rf ~/Library/Application\ Support/ai.holistic.surface # state and config
    ```

=== "Linux"

    ```sh
    rm ~/.local/bin/surface                                   # or: cargo uninstall surface-cli
    rm -rf ~/.local/share/surface ~/.config/surface            # state and config
    ```

=== "Windows"

    ```powershell
    Remove-Item -Recurse "$env:LOCALAPPDATA\Programs\surface"
    Remove-Item -Recurse "$env:LOCALAPPDATA\holistic\surface", "$env:APPDATA\holistic\surface"
    # then take the install directory back off your user PATH
    ```

There is nothing else: no service to stop, no launch agent, no registry keys,
and nothing on a server to delete because nothing was ever sent.

## Troubleshooting

??? failure "`cargo install surface-cli` fails compiling `libsqlite3-sys`"

    That is the bundled SQLite build, and it needs a C compiler:

    - **macOS** — `xcode-select --install`
    - **Debian/Ubuntu** — `sudo apt install build-essential`
    - **Windows** — the "Desktop development with C++" workload for MSVC

    Or skip it entirely with `cargo install surface-cli --no-default-features`, and
    see [without SQLite](#without-sqlite) for what that costs you.

??? failure "The Sites view says a browser is unreadable"

    Not a bug, and deliberately not hidden. Safari's history lives in a
    directory that needs **Full Disk Access**, so surface reports it as an
    unreadable browser rather than counting it as zero AI usage. Grant the
    permission in *System Settings → Privacy & Security → Full Disk Access*, to
    your terminal, or leave it — a known blind spot is more honest than a
    silent zero.

??? failure "The Sites view says `not compiled in`"

    The binary was built with `--no-default-features`. Install a released
    binary, or rebuild with the `sqlite` feature — see
    [without SQLite](#without-sqlite).

??? failure "The first run takes twenty seconds"

    Expected once. The first scan parses every transcript in the window; on a
    900 MB corpus that is around 19 seconds. Every run after it reads only the
    bytes that were appended since, typically under 50 ms. See
    [Core concepts](concepts.md#the-ledger).
