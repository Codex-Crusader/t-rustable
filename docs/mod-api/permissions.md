# Permissions

Permissions come from the imports of the component (ADR 0007). The host reads the imports before it runs any mod code.

| Import | Permission | The user sees |
|---|---|---|
| `t-rustable:api/types` | none | |
| `t-rustable:api/host` | none | |
| `t-rustable:api/document` | `document.read` | Read the text, pages, and objects of open documents. |
| `t-rustable:api/edit` | `document.edit` | Change open documents. Each change can be undone. |
| `t-rustable:api/ui` | `ui` | Add actions, toolbar items, panels, and overlays. |
| `t-rustable:api/files` | `files.pick` | Read files that you pick in a file dialog. |
| anything else, for example `wasi:*` | refused | |

Rules:

- The permissions in `manifest.json` must match the imports exactly. A missing permission or an unused permission stops the install.
- Any `0.1.x` version of an interface is accepted.
- A mod that receives a `doc` in its exports imports `document`, so every mod that works on documents needs `document.read`.
- Build Rust mods for `wasm32-unknown-unknown`. A `wasm32-wasip2` build imports WASI interfaces, and the host refuses it.

The table is in the SDK (`t_rustable_sdk::permissions`). To check a built mod:

```
wasm-tools component wit mod.wasm > mod.wit
cargo run -p t-rustable-sdk --example check-mod -- manifest.json mod.wit
```
