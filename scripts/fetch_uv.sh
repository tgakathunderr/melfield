#!/usr/bin/env bash
# Download the pinned uv release for the current (or given) target triple and
# place it where Tauri expects the sidecar: app/src-tauri/binaries/uv-<triple>[.exe].
# Verifies the release's published sha256.
set -euo pipefail

UV_VERSION="${UV_VERSION:-0.13.7}"
TRIPLE="${1:-$(rustc --print host-tuple)}"
case "$TRIPLE" in
  *windows*) ARCHIVE="uv-${TRIPLE}.zip"; EXE=".exe" ;;
  *) ARCHIVE="uv-${TRIPLE}.tar.gz"; EXE="" ;;
esac
BASE="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/app/src-tauri/binaries"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$BASE/$ARCHIVE" -o "$TMP/$ARCHIVE"
curl -fsSL "$BASE/$ARCHIVE.sha256" -o "$TMP/$ARCHIVE.sha256"
EXPECTED="$(cut -d' ' -f1 < "$TMP/$ARCHIVE.sha256")"
ACTUAL="$( (sha256sum "$TMP/$ARCHIVE" 2>/dev/null || shasum -a 256 "$TMP/$ARCHIVE") | cut -d' ' -f1)"
if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "uv checksum mismatch: $ACTUAL != $EXPECTED" >&2
  exit 1
fi

mkdir -p "$TMP/x" "$DEST"
case "$ARCHIVE" in
  *.zip) unzip -q "$TMP/$ARCHIVE" -d "$TMP/x" ;;
  *) tar -xzf "$TMP/$ARCHIVE" -C "$TMP/x" ;;
esac
BIN="$(find "$TMP/x" -type f -name "uv${EXE}" | head -1)"
cp "$BIN" "$DEST/uv-${TRIPLE}${EXE}"
chmod +x "$DEST/uv-${TRIPLE}${EXE}"
# Universal macOS builds need both slices present.
echo "uv ${UV_VERSION} -> $DEST/uv-${TRIPLE}${EXE}"
