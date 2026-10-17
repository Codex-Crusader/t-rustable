#!/usr/bin/env bash
# Downloads the pinned non-V8 PDFium build (no JavaScript, no XFA) and checks its SHA-256.
# Works in Git Bash on Windows and in bash on Linux.
# Usage: scripts/fetch-pdfium.sh [output-dir]   (default: .pdfium in the repo root)
# Output: <output-dir>/<platform>/ with bin/ or lib/, include/, licenses/, and args.gn.
set -euo pipefail

RELEASE="chromium/8100"
case "$(uname -s)" in
  Linux*)  ASSET="pdfium-linux-x64.tgz"; SHA="1c1894a12d4d96ee01400c3874444126254180c5c7d2ab628576b67f8120320a"; DIR="linux-x64" ;;
  MINGW*|MSYS*|CYGWIN*) ASSET="pdfium-win-x64.tgz"; SHA="51621bcc1d9be9ea448f4b84b0516862b928e3968d3fca899af1a6d6e2848d6d"; DIR="win-x64" ;;
  *) echo "Unsupported platform: $(uname -s)" >&2; exit 1 ;;
esac

REPO="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="${1:-$REPO/.pdfium}/$DIR"
if [ -f "$ROOT/.ok" ]; then echo "PDFium already in $ROOT"; exit 0; fi
mkdir -p "$ROOT"
TMP="$ROOT/$ASSET"
curl -sSfL --retry 3 -o "$TMP" "https://github.com/bblanchon/pdfium-binaries/releases/download/$RELEASE/$ASSET"
echo "$SHA  $TMP" | sha256sum -c -
tar -xzf "$TMP" -C "$ROOT"
rm "$TMP"

# The engine refuses a build with JavaScript or XFA.
grep -q "pdf_enable_v8 = false" "$ROOT/args.gn"
grep -q "pdf_enable_xfa = false" "$ROOT/args.gn"

touch "$ROOT/.ok"
echo "PDFium $RELEASE in $ROOT"
