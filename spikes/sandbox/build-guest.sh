#!/usr/bin/env bash
# Builds guest/runner.wasm from guest/runner.js with the Javy CLI.
# Downloads Javy from the official GitHub release into <repo>/tools/ (gitignored)
# and verifies it against the release's published SHA-256 before use.
set -euo pipefail

VERSION=v9.1.0
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) ASSET=javy-arm-macos-$VERSION ;;
  Darwin-x86_64) ASSET=javy-x86_64-macos-$VERSION ;;
  Linux-aarch64) ASSET=javy-arm-linux-$VERSION ;;
  Linux-x86_64) ASSET=javy-x86_64-linux-$VERSION ;;
  *) echo "unsupported platform" >&2; exit 1 ;;
esac

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"
TOOLS="$ROOT/tools/javy-$VERSION"
BASE="https://github.com/bytecodealliance/javy/releases/download/$VERSION"
mkdir -p "$TOOLS"

if [ ! -x "$TOOLS/$ASSET" ]; then
  curl -fsSL -o "$TOOLS/$ASSET.gz" "$BASE/$ASSET.gz"
  curl -fsSL -o "$TOOLS/$ASSET.gz.sha256" "$BASE/$ASSET.gz.sha256"
  want="$(awk '{print $1}' "$TOOLS/$ASSET.gz.sha256")"
  got="$(shasum -a 256 "$TOOLS/$ASSET.gz" | awk '{print $1}')"
  if [ "$want" != "$got" ]; then
    echo "checksum mismatch for $ASSET.gz: want $want got $got" >&2
    rm -f "$TOOLS/$ASSET.gz"
    exit 1
  fi
  gunzip -kf "$TOOLS/$ASSET.gz"
  chmod +x "$TOOLS/$ASSET"
fi

# deterministic=y: fixed clocks and zero RNG during Wizer pre-initialisation.
"$TOOLS/$ASSET" build "$HERE/guest/runner.js" \
  -J javy-stream-io=y -J text-encoding=y -J event-loop=n \
  -C deterministic=y -C source=omitted \
  -o "$HERE/guest/runner.wasm"
shasum -a 256 "$HERE/guest/runner.wasm"
ls -l "$HERE/guest/runner.wasm"
