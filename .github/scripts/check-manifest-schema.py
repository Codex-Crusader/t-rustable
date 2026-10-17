"""The manifest JSON Schema must give the expected result for each case in
crates/t-rustable-sdk/tests/manifest-cases.json, and accept the template manifest.
The Rust test manifest_schema.rs checks the same cases against Manifest::parse."""

import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parents[2]
schema = json.loads((root / "docs/mod-api/manifest.schema.json").read_text(encoding="utf-8"))
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)

failures = []
template = json.loads((root / "mods/template/manifest.json").read_text(encoding="utf-8"))
if not validator.is_valid(template):
    failures.append("template manifest is not valid")

data = json.loads((root / "crates/t-rustable-sdk/tests/manifest-cases.json").read_text(encoding="utf-8"))
for case in data["cases"]:
    manifest = dict(data["base"], **case["set"])
    if validator.is_valid(manifest) != case["valid"]:
        failures.append(f"case {case['name']!r}: expected valid={case['valid']}")

for failure in failures:
    print(failure)
print(f"{len(data['cases'])} cases checked, {len(failures)} failures")
sys.exit(1 if failures else 0)
