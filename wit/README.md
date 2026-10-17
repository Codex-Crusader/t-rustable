# wit

The mod API: WIT package `t-rustable:api@0.1.0`.

| File | Content |
|---|---|
| `types.wit` | Shared types: IDs, geometry, colors, errors, overlay shapes |
| `document.wit` | Read access to a document (`doc` resource) |
| `edit.wit` | Transactions of core commands |
| `host.wit` | Log, progress, cancel, settings |
| `ui.wit` | Actions, panels, overlays, dialogs, selection |
| `files.wit` | Files that the user picks |
| `worlds.wit` | The exported `tool` and `batch-job` interfaces and the worlds `interactive`, `batch`, `full` |

The reference is in `docs/mod-api/`. Check the package with `wasm-tools component wit wit/`.

License: MIT.
