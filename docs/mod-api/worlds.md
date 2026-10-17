# Worlds and exports

## Worlds

| World | Imports | Exports | Runs in |
|---|---|---|---|
| `interactive` | document, edit, host, ui, files | `tool` | GUI |
| `batch` | document, edit, host | `batch-job` | CLI |
| `full` | as `interactive` | `tool` and `batch-job` | GUI and CLI |

A component imports only the interfaces that it uses. Unused imports are removed when the component is built. The imports decide the permissions (see [permissions](permissions.md)).

In Rust, export with the SDK macros: `export_interactive!(T)`, `export_batch!(T)`, or `export_full!(T)`.

## tool

The host calls these functions in an interactive mod. In Rust, implement the `Tool` trait. Each method has a default, so implement only what you need.

| Function | When the host calls it |
|---|---|
| `activate()` | The user selects this tool. |
| `deactivate()` | The user selects another tool. |
| `run-action(document, action-id, args)` | The user runs an action of this mod. `document` is none when no document is open. `args` is a JSON string. |
| `on-pointer(document, event)` | A pointer event on a page while this tool is active. Returns `pointer-result`: overlay, cursor, handled. |
| `on-key(document, event)` | A key event while this tool is active. Returns `handled`. |
| `on-text-input(document, text)` | Text that the user typed, after input method composition. Use it for text entry. |
| `on-panel-message(panel-id, message)` | A JSON message from a panel of this mod. |
| `on-document-changed(document, change)` | The document changed, from any source. `change` has the transaction ID, the affected pages, and `structure-changed`. |

`pointer-event` has `kind` (down, move, up, enter, leave, double-click), `page`, `at` (page coordinates), `button`, `modifiers`, and `pressure`. `key-event` has `kind`, `key` and `code` (DOM values), `modifiers`, and `repeat`.

## batch-job

| Function | Notes |
|---|---|
| `run(document, args)` | `args` and the result are JSON strings. In Rust, implement the `BatchJob` trait. |

A batch job has no `ui` and no `files`. `to-display` and `from-display` return `no-display`.
