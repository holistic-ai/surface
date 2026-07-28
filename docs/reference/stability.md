# Stability

What surface promises not to break, and how it breaks it when it must.

## Versioning

surface follows [Semantic Versioning](https://semver.org). Every release is
described in
[`CHANGELOG.md`](https://github.com/holistic-ai/surface/blob/main/CHANGELOG.md),
which is the changelog. The release workflow reads the section for the tag it is
building and uses it as the GitHub release notes, so the two can never disagree —
and a tag with no section fails the build rather than publishing empty notes.

| Change | Version bump |
|---|---|
| Bug fix, no change to a documented interface | patch (`0.1.0 → 0.1.1`) |
| New view, flag, tool or domain; backward compatible | minor (`0.1.0 → 0.2.0`) |
| Breaking change to a documented interface | major (`1.0.0 → 2.0.0`) |

!!! warning "Pre-1.0"
    While the version is below `1.0`, minor releases may contain breaking
    changes. They will always be listed under **Breaking changes** in the release
    notes. Pin an exact version if you script against surface today.

## What counts as a public interface

**Public** — covered by the guarantees above:

- **CLI flags** and their meanings: `--json`, `--offline`, `--check`.
- **Exit codes**: `0` for a completed scan, non-zero only for a failure to run.
- **`surface.toml` keys** documented in [Configuration](../guide/configuration.md),
  and their defaults. Changing a default is a behaviour change.
- **`SURFACE_*` environment variables** documented in [CLI](cli.md#environment-variables).
- **The `--json` payload** — see below.
- **`reason` strings**: `insufficient_privileges`, `no_history_database`,
  `tool_unavailable`. They exist to be matched on.

**Not public** — may change in any release, without notice:

- Everything in the [Rust API reference](rust.md). It is a binary crate's
  internals.
- The on-disk format of `usage-ledger.json` and `litellm-prices.json`. Both are
  caches: a new version may read an old file, or discard it and rebuild.
- Dashboard layout, colours, glyphs, column widths and panel titles.
- Error message text (the exit codes are stable, the prose is not).
- Which tools, domains and browsers are in the tables — these are expected to
  grow, and a new row is a minor release rather than a break.

## The JSON payload

The stable part is the **shape**: existing keys keep their names, types and
meanings.

- **Adding a key** is a minor release. Consumers should ignore keys they do not
  know.
- **Renaming, removing or retyping a key** is breaking, and pre-1.0 it will
  appear under **Breaking changes** in the release notes.
- **New rows in an array** — a tool, a domain, a cost state — are not breaking.

Every payload carries the version that wrote it, which is the field to branch on
if you must:

```json
{ "surface": "0.1.0" }
```

!!! danger "One invariant that will not change"
    An unpriced cost serialises as `{"state": "unpriced", "usd": null}` and
    **never** as `0.0`. A total with unpriced models under it is a floor. This is
    the one thing surface will not trade away for a tidier schema, and there is a
    test that fails if it ever does.

## The Rust API

There is no `lib` target. The rustdoc reference exists so the design reasoning is
readable, not so another crate can depend on it — module paths, type names and
signatures change freely, in any release.

If you need surface's data programmatically, use `--json`. That is the supported
interface, and it is versioned.

## Numbers can change without the API changing

A change to a *computed figure* that is not a bug fix is treated as **breaking**
and documented with the reason:

- A different deduplication rule, or a change to how a source's records are
  accumulated.
- A change to which token kinds count toward a total, or to the rate a kind is
  priced at.
- A change to how a working directory is resolved to a repository slug.

Two things that are **not** breaking, because they are by design:

- **Prices moving.** The rate table is fetched, not pinned, and history re-prices
  itself when rates move. That is the point of pricing at read time.
- **A new tool, domain or browser appearing** in your results after an upgrade.
  A table gaining a row is coverage improving.

## Supported platforms

macOS, Linux and Windows, on x86-64 and arm64, all exercised in CI. Dropping one
would be a breaking change announced in the release notes.

The MSRV is **Rust 1.88**, and CI builds on exactly that version to keep the
promise honest. Raising it is a minor release, noted in the release notes.

## The `sqlite` feature

`sqlite` is a **default** feature. Building without it is supported, tested in CI,
and degrades explicitly — the Sites view says it was compiled out and OpenCode is
reported as unreadable, rather than either showing as empty. Neither the feature
nor that behaviour will be removed in a minor release.
