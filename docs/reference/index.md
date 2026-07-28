# Reference

Precise interfaces, in the order you are likely to need them.

<div class="site-grid" markdown>

<div class="site-card" markdown>
### [CLI](cli.md)
Every flag, every exit code, every environment variable.
</div>

<div class="site-card" markdown>
### [Coverage](coverage.md)
The 18 tools, 30 domains, 10 browsers and 3 token sources surface knows about.
</div>

<div class="site-card" markdown>
### [Rust API](rust.md)
rustdoc for the crate, generated from the source on every docs build.
</div>

<div class="site-card" markdown>
### [Stability](stability.md)
What is promised, what is not, and how a break is announced.
</div>

</div>

## The one-screen version

```sh
surface              # scan, then open the dashboard
surface --json       # scan, print JSON, exit
surface --offline    # never touch the network
surface --check      # show resolved paths and settings, then exit
```

| | |
|---|---|
| **Config file** | `surface.toml` in the config directory; every setting optional |
| **State** | `usage-ledger.json` and `litellm-prices.json`, state directory only |
| **Env prefix** | `SURFACE_*` — see [CLI](cli.md#environment-variables) |
| **Exit code** | `0` on a completed scan, including one that found nothing |
| **Network** | One optional request, for model prices |

## Machine-readable interfaces

Three things a script can depend on, each with its own stability note:

| Surface | Documented in |
|---|---|
| The `--json` payload | [JSON output](../guide/json.md) |
| `surface.toml` keys | [Configuration](../guide/configuration.md) |
| `reason` strings (`insufficient_privileges`, `no_history_database`, `tool_unavailable`) | [JSON output](../guide/json.md#sites) |

!!! note "How to read the Rust API reference"
    surface is a binary crate, so its modules are private and rustdoc is run with
    `--document-private-items`. What you get is the internals: the module-level
    docs carry the design reasoning and the measurements behind each decision,
    which is usually what you came for. None of it is a public library API — see
    [Stability](stability.md#the-rust-api).
