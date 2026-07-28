#!/bin/sh
# surface installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/holistic-ai/surface/main/install.sh | sh
#
# Downloads the release archive for this machine, verifies its checksum against
# the release's SHA256SUMS, and puts one binary on your PATH. Nothing else: no
# daemon, no launch agent, no shell profile edits.
#
# Overridable:
#   SURFACE_VERSION=v0.1.0   install a specific tag instead of the latest
#   SURFACE_INSTALL_DIR=~/bin   install somewhere other than the default
set -eu

REPO=holistic-ai/surface

fail() {
	echo "install: $1" >&2
	exit 1
}

# --------------------------------------------------------------- this machine

os=$(uname -s)
arch=$(uname -m)

# Patterns are quoted because bash 3.2 — still /bin/sh on macOS — rejects an
# unquoted space inside a case pattern.
case "$os $arch" in
"Darwin arm64" | "Darwin aarch64") target=aarch64-apple-darwin ;;
"Darwin x86_64") target=x86_64-apple-darwin ;;
"Linux x86_64" | "Linux amd64") target=x86_64-unknown-linux-gnu ;;
"Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-gnu ;;
*) fail "no prebuilt binary for $os $arch — build from source: cargo install surface-cli" ;;
esac

# The glibc builds are made on Ubuntu 22.04, so they need glibc 2.35 or newer.
# x86-64 has a statically linked musl build to fall back to, which needs nothing
# at all; arm64 does not, so there it is better to say so than to install a
# binary that cannot start.
if [ "$os" = Linux ]; then
	# getconf asks the question directly. `ldd --version` is the fallback, and
	# on a musl system it answers "musl" — which is itself the answer.
	libc=$(getconf GNU_LIBC_VERSION 2>/dev/null || ldd --version 2>&1 | head -1)
	minor=$(echo "$libc" | sed -n 's/.*[^0-9]2\.\([0-9][0-9]*\).*/\1/p' | head -1)
	case "$libc" in *musl* | *musl) minor= ;; esac

	if [ -z "$minor" ] || [ "$minor" -lt 35 ]; then
		case "$arch" in
		x86_64 | amd64) target=x86_64-unknown-linux-musl ;;
		*) fail "needs glibc 2.35 or newer (found ${libc:-none}) and there is no
  static build for $arch — build from source: cargo install surface-cli" ;;
		esac
	fi
fi

for tool in curl tar; do
	command -v "$tool" >/dev/null || fail "$tool is required"
done

# Both exist in the wild; neither is everywhere.
if command -v sha256sum >/dev/null; then
	sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null; then
	sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
	fail "need sha256sum or shasum to verify the download"
fi

# ------------------------------------------------------------------- version

tag=${SURFACE_VERSION:-}
# Tags carry the `v`; accept SURFACE_VERSION either way rather than failing on a
# download 404 several steps later.
case "$tag" in [0-9]*) tag=v$tag ;; esac
if [ -z "$tag" ]; then
	tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
		sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
fi
[ -n "$tag" ] || fail "no published release found — install with: cargo install surface-cli"

name="surface-$tag-$target"
base="https://github.com/$REPO/releases/download/$tag"

# --------------------------------------------------------- download + verify

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "surface $tag ($target)"
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz" ||
	fail "could not download $base/$name.tar.gz"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" ||
	fail "could not download the checksum file"

want=$(grep "  $name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
[ -n "$want" ] || fail "no checksum for $name.tar.gz in SHA256SUMS"
got=$(sha256 "$tmp/$name.tar.gz")
[ "$want" = "$got" ] || fail "checksum mismatch — refusing to install
  expected $want
  got      $got"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"

# --------------------------------------------------------------------- install

# Prefer a system directory when we can write to it, since it is already on
# everyone's PATH; fall back to the user's own rather than asking for sudo.
if [ -n "${SURFACE_INSTALL_DIR:-}" ]; then
	dir=$SURFACE_INSTALL_DIR
elif [ -w /usr/local/bin ]; then
	dir=/usr/local/bin
else
	dir=$HOME/.local/bin
fi

mkdir -p "$dir"
install -m 755 "$tmp/$name/surface" "$dir/surface" 2>/dev/null ||
	{ cp "$tmp/$name/surface" "$dir/surface" && chmod 755 "$dir/surface"; } ||
	fail "could not write to $dir — set SURFACE_INSTALL_DIR to somewhere writable"

echo "installed $dir/surface"

case ":$PATH:" in
*":$dir:"*) echo "run: surface" ;;
*) echo "$dir is not on your PATH — add it, or run $dir/surface" ;;
esac
