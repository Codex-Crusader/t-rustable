//! The JSON Schema and `Manifest::parse` must agree, and the template manifest must be valid.

use serde_json::{Value, json};
use t_rustable_sdk::manifest::Manifest;

fn schema() -> jsonschema::Validator {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/mod-api/manifest.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&serde_json::from_str(&text).unwrap()).unwrap()
}

fn good() -> Value {
    json!({
        "id": "com.example.rotate",
        "name": "Rotate",
        "version": "1.0.0",
        "apiVersion": "0.1",
        "author": "Example",
        "description": "Rotates pages.",
        "permissions": ["document.read", "document.edit", "ui"],
        "entry": "mod.wasm",
        "worlds": ["interactive"],
        "panels": [{ "id": "options", "title": "Options", "entry": "ui/panel.html" }],
        "actions": [{ "id": "rotate-page", "title": "Rotate page", "shortcut": "Ctrl+Shift+R" }]
    })
}

fn with(field: &str, value: Value) -> Value {
    let mut m = good();
    m[field] = value;
    m
}

#[test]
fn template_manifest_is_valid() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mods/template/manifest.json"
    ))
    .unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert!(schema().is_valid(&value));
    Manifest::parse(&text).unwrap();
}

#[test]
fn schema_and_parser_agree() {
    let cases = [
        good(),
        with("id", json!("Com.Example")),
        with("id", json!("com..example")),
        with("id", json!("ab")),
        with("version", json!("1.0")),
        with("version", json!("01.0.0")),
        with("version", json!("1.0.0-beta.1")),
        with("apiVersion", json!("0.2")),
        with("permissions", json!(["network"])),
        with("permissions", json!(["ui", "ui"])),
        with("entry", json!("../mod.wasm")),
        with("entry", json!("/mod.wasm")),
        with("entry", json!(r"bin\mod.wasm")),
        with("entry", json!("C:mod.wasm")),
        with("entry", json!("a//mod.wasm")),
        with("entry", json!("mod.js")),
        with("worlds", json!([])),
        with("worlds", json!(["gui"])),
        with("name", json!(" ")),
        with("extra", json!(1)),
        with(
            "panels",
            json!([{ "id": "Options", "title": "x", "entry": "p.html" }]),
        ),
        with(
            "panels",
            json!([{ "id": "options", "title": "x", "entry": "./p.html" }]),
        ),
        with("actions", json!([{ "id": "a-", "title": "x" }])),
    ];
    let validator = schema();
    for case in cases {
        let by_schema = validator.is_valid(&case);
        let by_parser = Manifest::parse(&case.to_string()).is_ok();
        assert_eq!(
            by_schema, by_parser,
            "schema {by_schema}, parser {by_parser}: {case}"
        );
    }
}
