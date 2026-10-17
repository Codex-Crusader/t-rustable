# ui

Interface `t-rustable:api/ui@0.1.1`. Permission: `ui`. Interactive world only.

## Registration

The manifest declares actions and panels, so the command palette can show them before the mod runs. These functions add more at run time.

| Function | Notes |
|---|---|
| `register-action(action)` | `id`, `title`, `description` |
| `register-toolbar-item(item)` | `action-id`, `icon` (path of an SVG file in the package), `tooltip` |
| `register-panel(panel)` | `id`, `title`, `entry` (path of an HTML file in the package) |
| `suggest-shortcut(shortcut)` | `action-id`, `keys` such as "Ctrl+Shift+R". The user decides the final shortcut in `keymap.json`. |

## Overlays

| Function | Notes |
|---|---|
| `set-overlay(shapes)` | Replaces the overlay of this mod. Shapes are in [types](types.md). |
| `clear-overlay()` | |
| `register-overlay-image(data)` | PNG or JPEG. Returns an image ID for `overlay-shape::image`. Register an image one time, then use its ID on each pointer move. |

`tool.on-pointer` also returns an overlay. Use `set-overlay` for changes that do not come from a pointer event, for example after a panel message.

## Panels

A panel is an HTML page from the mod package. It runs in a sandboxed iframe with no access to T-Rustable, the network, or other panels.

- The panel sends a message with `parent.postMessage(jsonString, "*")`. The host checks the sender and calls `tool.on-panel-message(panel-id, message)`.
- The mod sends a message to the panel with `post-to-panel(panel-id, message)`. The panel receives it as a `message` event.
- Messages are JSON strings.
- Load scripts and styles from files in the package. Inline scripts do not run.

## Dialogs and messages

| Function | Notes |
|---|---|
| `notify(level, message)` | A short message: info, success, warning, error. |
| `confirm(title, message)` | A yes or no dialog. Returns true for yes. The time limit of the call stops while the dialog is open. |

## Selection

`context()` returns the state of the active tab: `document-open`, `active-page`, `selected-pages`, `selected-objects`, `zoom`. It gives IDs only. Use the `doc` of the current call to read the document.
