"""MkDocs hook that builds the rustdoc API reference into the site.

Registered as ``hooks:`` in ``mkdocs.yml``. On every build it runs
``cargo doc --no-deps --document-private-items`` for the crate at the repository
root (skipped when no crate source changed since the last run), stages the
output, and appends the files to the MkDocs build — so the Rust API reference is
part of the site in ``mkdocs serve``, ``mkdocs build``, and CI alike.

``--document-private-items`` is not optional here: ``surface`` is a binary
crate whose modules are private, and its module-level docs are the design
record. Without the flag rustdoc would emit an index with nothing under it.

Because the files join the build before pages render, links to them validate
under ``mkdocs build --strict`` like links to any other page.

Adapted from the docsforge template (https://github.com/kleyt0n/docsforge),
which keeps its crate in ``rust/``; surface *is* the crate, so the manifest is
at the root.
"""

from __future__ import annotations

import logging
import shutil
import subprocess
from pathlib import Path

from mkdocs.structure.files import File

log = logging.getLogger("mkdocs.hooks.rustdoc")

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "Cargo.toml"
TARGET = ROOT / "target"
DOC_OUT = TARGET / "doc"
STAMP = DOC_OUT / "surface" / "index.html"
STAGE = TARGET / "rustdoc-site"


def _sources_stale(stamp: Path) -> bool:
    """Return True if any crate source is newer than the rustdoc stamp file."""
    if not stamp.exists():
        return True
    stamp_mtime = stamp.stat().st_mtime
    sources = [*ROOT.glob("src/**/*.rs"), MANIFEST]
    return any(path.stat().st_mtime > stamp_mtime for path in sources)


def _cargo_doc() -> None:
    """Build the crate documentation with rustdoc."""
    cmd = [
        "cargo",
        "doc",
        "--no-deps",
        "--document-private-items",
        "--manifest-path",
        str(MANIFEST),
    ]
    try:
        subprocess.run(cmd, check=True, capture_output=True, text=True)
    except FileNotFoundError:
        msg = "cargo (Rust toolchain) not found — install Rust, or drop the hooks: block from mkdocs.yml"
        raise RuntimeError(msg) from None
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(exc.stderr) from None


def on_files(files, *, config):
    """Build rustdoc and append it to the files collection under ``rust/``."""
    if _sources_stale(STAMP):
        _cargo_doc()
        shutil.rmtree(STAGE, ignore_errors=True)
        shutil.copytree(DOC_OUT, STAGE / "rust")
    elif not (STAGE / "rust").is_dir():
        shutil.copytree(DOC_OUT, STAGE / "rust")

    count = 0
    for path in sorted(STAGE.rglob("*")):
        if not path.is_file():
            continue
        if path.suffix == ".md":
            # rustdoc ships font-license .md files that nothing links to. As
            # part of the build they would count as docs pages missing from
            # the nav, failing `omitted_files` validation — so leave them out.
            continue
        rel = path.relative_to(STAGE)
        files.append(File(str(rel), str(STAGE), config["site_dir"], config["use_directory_urls"]))
        count += 1
    log.info("rustdoc: staged %s files from %s", count, MANIFEST)
    return files
