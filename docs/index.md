---
# No table of contents on the landing page: it lists four headings nobody
# navigates by, and dropping it hands ~12rem back to the column — which is
# what gives the recording below room to be legible.
hide:
  - toc
---

<!-- =================================================================
     HOME PAGE
     The hero below is plain HTML + the `.site-*` classes defined in
     docs/stylesheets/extra.css.

     Deliberately spare: mark, name, one sentence, two links. Badges live
     in the README, where they answer a stranger's first questions; here
     they would only compete with the tagline.
     ================================================================= -->

<div class="site-hero" markdown>

<h1 class="site-hero__lockup"><span>surface</span></h1>

<p class="site-hero__tagline">
What AI runs here, and what it costs.
</p>

<p class="site-hero__lede">
One binary, one local scan, no account. It reads the transcripts, history and
config already on your disk, then tells you what runs here and what it cost.
</p>

=== ":simple-apple: macOS · :simple-linux: Linux"

    ```sh
    curl -fsSL https://raw.githubusercontent.com/holistic-ai/surface/main/install.sh | sh
    ```

=== ":fontawesome-brands-windows: Windows · PowerShell"

    ```powershell
    irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex
    ```

=== ":fontawesome-brands-windows: Windows · cmd"

    ```bat
    powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/holistic-ai/surface/main/install.ps1 | iex"
    ```

    `irm` and `iex` are PowerShell only, so cmd hands the job over.

=== ":simple-rust: Cargo"

    ```sh
    cargo install surface-cli
    ```

Every release ships `SHA256SUMS`, and the installer checks the download against it
before it moves anything. [All the ways in](getting-started/installation.md).
{ .site-hero__aside }

</div>

<video class="site-demo" controls autoplay muted loop playsinline
       preload="metadata" poster="assets/demo-poster.webp"
       aria-label="A screen recording of the surface dashboard: the Overview with a
       spend-per-day chart, then the Tools, Usage and Projects views walked through
       with the keyboard.">
  <source src="assets/demo-surface.mp4" type="video/mp4">
  <a href="assets/demo-surface.mp4">Download the recording</a> — your browser will not play it inline.
</video>

## Why surface

AI tooling arrives one install at a time — an agent in `~/.local/bin`, an
assistant in `/Applications`, an extension in a `.vscode` directory, a browser
tab doing the rest. None of it is written down. Neither is the bill: the tokens
are spent here, but the invoice arrives monthly, aggregated, detached from the
work that caused it.

**surface answers both from local files only.** It reads the transcripts your
tools already write and the history databases already on disk, then prices them
against [LiteLLM's public rate table](https://github.com/BerriAI/litellm). No
daemon, no account, no telemetry — one optional request, for prices.

It is narrow on purpose: **no message content, no URLs, no paths**. The
AI-domain filter runs inside SQLite, so the rest of your browsing never reaches
memory, and working directories become `owner/name` before anything is stored.
[Privacy](guide/privacy.md) has the full list.

## What is inside

<div class="site-grid" markdown>

<div class="site-card" markdown>
### [Installation](getting-started/installation.md)
`cargo install`, a prebuilt binary, or a build with no C toolchain at all.
</div>

<div class="site-card" markdown>
### [Quickstart](getting-started/quickstart.md)
First scan, first dashboard, and what the first run costs you in time.
</div>

<div class="site-card" markdown>
### [Core concepts](getting-started/concepts.md)
The scan, the ledger, the price table and the repo attribution — five ideas.
</div>

<div class="site-card" markdown>
### [The dashboard](guide/dashboard.md)
Five views, every key, and what each column actually measures.
</div>

<div class="site-card" markdown>
### [Costs](guide/costs.md)
Where the money figures come from, and the four ways they can be wrong.
</div>

<div class="site-card" markdown>
### [Privacy](guide/privacy.md)
What is read, what is stored, what is never touched, and what is unreadable.
</div>

</div>

## Quickstart

```sh
cargo install surface-cli   # crate name; the binary is `surface`
surface
```

That is the whole setup — no config file, no login. To pipe the same scan into
something else:

```sh
surface --json --offline | jq '.usage.totals_by_tool'
```

## At a glance

| | |
|---|---|
| **Binary** | ~2 MB, static, no runtime dependencies |
| **Warm scan** | ~400 ms — 6 ms tools, 330 ms browser history, 50 ms transcripts |
| **Cold scan** | ~19 s once, reading 900 MB of transcripts; ~50 ms thereafter |
| **Coverage** | [18 AI tools · 30 AI domains · 10 browsers · 3 token sources](reference/coverage.md) |
| **Platforms** | macOS, Linux, Windows — x86-64 and arm64 |
| **Privilege** | Runs unprivileged. Never elevates, never prompts |
| **Network** | One optional request, for model prices. `--offline` skips it |

!!! tip "New here?"
    Start with [Installation](getting-started/installation.md), then the
    [Quickstart](getting-started/quickstart.md). If you want to know why a
    number says `≥` or `unpriced` before you trust it, read
    [Core concepts](getting-started/concepts.md) and then
    [Costs](guide/costs.md).
