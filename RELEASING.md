# Releasing

The release is tag-triggered: `git push --tags` builds six targets, publishes a
GitHub Release with checksums and provenance, then publishes to crates.io. What
follows is the order that keeps that from going wrong.

## One-time repository setup

- **Settings → Secrets and variables → Actions**: add `CARGO_REGISTRY_TOKEN`, a
  crates.io API token scoped to publish. Without it every step succeeds except
  the last, leaving a published GitHub Release and no crate.
- **Settings → Pages → Build and deployment → Source**: "GitHub Actions".
- **Settings → Actions → General → Workflow permissions**: read-only is enough.
  Each job requests what it needs.

## Cutting a release

1. **Update `CHANGELOG.md`.** Move `[Unreleased]` items under a new
   `## [x.y.z] - YYYY-MM-DD` heading and add the link reference at the foot. The
   workflow slices this section out and uses it verbatim as the Release body, and
   **a missing section fails the build** — deliberately, because the alternative
   is publishing empty notes.

2. **Bump `version` in `Cargo.toml`.**

3. **Refresh `Cargo.lock` in the same commit.** `cargo check` is enough to update
   the crate's own entry. Every release build runs `--locked`, so a lockfile that
   disagrees with the manifest fails all six of them at the first step.

4. **Get CI green on `main` before tagging.** The tag build shares almost nothing
   with a red CI run's causes, but it is six slow jobs — find out cheaply.

5. **Tag and push.**

   ```sh
   git tag -a v0.1.0 -m "surface 0.1.0"
   git push origin main --follow-tags
   ```

6. **Watch the run.** The `check` job fails fast if the tag, the manifest and the
   changelog disagree, or if the crate does not package. After it, six builds run
   in parallel; the fat-LTO release profile makes each of them slow, and the
   aarch64 and musl legs compile SQLite from C with a cross toolchain.

7. **Verify what shipped.** Six archives plus `SHA256SUMS` on the Release, then:

   ```sh
   shasum -a 256 -c SHA256SUMS --ignore-missing
   gh attestation verify surface-v0.1.0-aarch64-apple-darwin.tar.gz \
     --repo holistic-ai/surface
   ```

8. **Smoke-test the one-liners** on each platform, ideally on a machine that has
   never had surface on it.

## A dry run

`workflow_dispatch` on the release workflow builds every target and uploads the
archives as workflow artifacts, but publishes nothing — the `release` and
`crates-io` jobs are both guarded on a tag ref. Use it to prove a matrix change
before committing to a tag.

## If something fails

- **`check` failed.** Nothing was published. Fix, delete the tag locally and
  remotely (`git tag -d v0.1.0 && git push --delete origin v0.1.0`), re-tag.
- **A build leg failed.** No Release was created, because `release` needs all six.
  Same recovery.
- **`release` failed after some uploads.** Delete the draft/partial Release in the
  UI, then re-run the workflow from the Actions tab — the tag is unchanged.
- **`crates-io` failed.** The GitHub Release stands and is fine. Fix the cause and
  run `cargo publish --locked` locally from the tag; there is no need to re-tag.
- **A bad version reached crates.io.** It cannot be deleted, only yanked:
  `cargo yank --version 0.1.0 surface-cli`. Yanking leaves existing lockfiles
  working and stops new ones resolving to it. Then release a patch.

## Version numbers

Semantic versioning, with the pre-1.0 caveat that minor releases may break things
— see [Stability](https://holistic-ai.github.io/surface/reference/stability/).
Anything breaking goes under a **Breaking changes** heading in the changelog
entry, which is what makes it appear in the Release body.
