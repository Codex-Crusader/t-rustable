#!/usr/bin/env bash
# Downloads the spike dependencies and checks their SHA-256:
# the non-V8 PDFium build (no JavaScript, no XFA) and the Noto Sans font (OFL).
# Works in Git Bash on Windows and in bash on Linux. Output: spikes/.pdfium/<platform>/
set -euo pipefail

RELEASE="chromium/8100"
case "$(uname -s)" in
  Linux*)  ASSET="pdfium-linux-x64.tgz"; SHA="1c1894a12d4d96ee01400c3874444126254180c5c7d2ab628576b67f8120320a"; DIR="linux-x64" ;;
  MINGW*|MSYS*|CYGWIN*) ASSET="pdfium-win-x64.tgz"; SHA="51621bcc1d9be9ea448f4b84b0516862b928e3968d3fca899af1a6d6e2848d6d"; DIR="win-x64" ;;
  *) echo "Unsupported platform: $(uname -s)" >&2; exit 1 ;;
esac

HERE="$(cd "$(dirname "$0")" && pwd)"

fetch_font() {
  local dir="$HERE/assets"
  mkdir -p "$dir"
  if [ ! -f "$dir/NotoSans-Regular.ttf" ]; then
    curl -sSfL -o "$dir/NotoSans-Regular.ttf" https://raw.githubusercontent.com/notofonts/notofonts.github.io/main/fonts/NotoSans/hinted/ttf/NotoSans-Regular.ttf
    curl -sSfL -o "$dir/OFL.txt" https://raw.githubusercontent.com/notofonts/latin-greek-cyrillic/main/OFL.txt
  fi
  echo "478c558ea716033cd60c03438f628dfa75694dcf6b5f6d505a2f05fd2b4f3823  $dir/NotoSans-Regular.ttf" | sha256sum -c -
}
fetch_font

ROOT="$HERE/.pdfium/$DIR"
if [ -f "$ROOT/.ok" ]; then echo "PDFium already in $ROOT"; exit 0; fi
mkdir -p "$ROOT"
TMP="$ROOT/$ASSET"
curl -sSfL -o "$TMP" "https://github.com/bblanchon/pdfium-binaries/releases/download/$RELEASE/$ASSET"
echo "$SHA  $TMP" | sha256sum -c -
tar -xzf "$TMP" -C "$ROOT"
rm "$TMP"
touch "$ROOT/.ok"
echo "PDFium $RELEASE in $ROOT"
