#!/usr/bin/env bash
# Writes the notices of the native libraries in scripts/native-licenses.list to stdout.
# Fails if the PDFium licenses/ folder has a file that the list does not name.
# Needs: scripts/fetch-pdfium.sh first, cargo, and python3.
# Usage: scripts/native-notices.sh >> THIRD_PARTY_LICENSES
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
LIST="$REPO/scripts/native-licenses.list"
case "$(uname -s)" in
  Linux*) PDFIUM="$REPO/.pdfium/linux-x64/licenses" ;;
  *) PDFIUM="$REPO/.pdfium/win-x64/licenses" ;;
esac
[ -d "$PDFIUM" ] || { echo "$PDFIUM is missing. Run scripts/fetch-pdfium.sh first." >&2; exit 1; }
QPDF_SYS="$(cd "$REPO" && cargo metadata --format-version 1 --locked |
  python3 -c 'import json, os, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "qpdf-sys":
        print(os.path.dirname(p["manifest_path"]))')"
[ -n "$QPDF_SYS" ] || { echo "qpdf-sys is not in the dependency graph." >&2; exit 1; }

for file in "$PDFIUM"/*; do
  name="$(basename "$file")"
  if ! grep -qE "^pdfium +$name " "$LIST"; then
    echo "PDFium has the license file $name, which $LIST does not name. Check it (ADR 0016)." >&2
    exit 1
  fi
done

printf '\n\n# Native libraries in PDFium and qpdf\n\n'
printf 'Portions of this software are copyright The FreeType Project (www.freetype.org). All rights reserved.\n'
printf 'This software is based in part on the work of the Independent JPEG Group.\n'
grep -vE '^(#|$)' "$LIST" | while read -r source path library; do
  case "$source" in
    pdfium) dir="$PDFIUM" ;;
    qpdf-sys) dir="$QPDF_SYS" ;;
    *) echo "Unknown source $source in $LIST" >&2; exit 1 ;;
  esac
  [ -f "$dir/$path" ] || { echo "$dir/$path is missing." >&2; exit 1; }
  printf '\n## %s\n\n' "$library"
  cat "$dir/$path"
done
