#!/usr/bin/env bash
# Downloads the spike dependencies and checks their SHA-256:
# the non-V8 PDFium build (no JavaScript, no XFA) and the Noto Sans font (OFL).
# Works in Git Bash on Windows and in bash on Linux. Output: spikes/.pdfium/<platform>/
set -euo pipefail

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

"$HERE/../scripts/fetch-pdfium.sh" "$HERE/.pdfium"
