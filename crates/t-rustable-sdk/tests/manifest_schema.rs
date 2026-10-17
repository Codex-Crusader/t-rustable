//! `Manifest::parse` must give the expected result for each case in `manifest-cases.json`.
//! CI checks the JSON Schema against the same cases (`.github/scripts/check-manifest-schema.py`),
//! so the parser and the schema agree.

use serde_json::Value;
use t_rustable_sdk::manifest::Manifest;

#[test]
fn template_manifest_is_valid() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mods/template/manifest.json"
    ))
    .unwrap();
    Manifest::parse(&text).unwrap();
}

#[test]
fn parser_matches_expected_cases() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/manifest-cases.json"
    ))
    .unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    for case in file["cases"].as_array().unwrap() {
        let mut manifest = file["base"].clone();
        for (field, value) in case["set"].as_object().unwrap() {
            manifest[field] = value.clone();
        }
        let valid = Manifest::parse(&manifest.to_string()).is_ok();
        assert_eq!(
            valid,
            case["valid"].as_bool().unwrap(),
            "case {}",
            case["name"]
        );
    }
}
