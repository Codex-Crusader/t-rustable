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

## Tools and commands

A manifest action with `"tool": true` is a tool. When the user selects it, the host calls `tool.activate` (if the mod is not active yet) and then `tool.run-action`. Then the mod gets pointer events, key events, and typed text until the user selects another tool. Other actions are commands: the host calls `tool.run-action` one time. Actions that `register-action` adds are commands.

Shortcuts: the manifest `shortcut` and `suggest-shortcut` are suggestions. The user file `keymap.json` always wins. If a suggestion conflicts with another binding, the host does not bind it, and the command palette shows "Not bound". A shortcut must have Ctrl, Alt, or Meta, or be an F key.

## Overlays

| Function | Notes |
|---|---|
| `set-overlay(shapes)` | Replaces the overlay of this mod. Shapes are in [types](types.md). |
| `clear-overlay()` | |
| `register-overlay-image(data)` | PNG or JPEG. Returns an image ID for `overlay-shape::image`. Register an image one time, then use its ID on each pointer move. |

`tool.on-pointer` also returns an overlay. Use `set-overlay` for changes that do not come from a pointer event, for example after a panel message.

The host draws overlays upright on the screen, in the same way as the core adds new text and images (ADR 0017). Text shapes use the bundled Noto fonts, so a caret at the width from `edit.measure-text` lines up with the text. A polite live region reads the `label` of each new shape.

## Panels

A panel is an HTML page from the mod package. It runs in a sandboxed iframe with no access to T-Rustable, the network, or other panels.

- The panel sends a message with `parent.postMessage(jsonString, "*")`. The host checks the sender and calls `tool.on-panel-message(panel-id, message)`.
- The mod sends a message to the panel with `post-to-panel(panel-id, message)`. The panel receives it as a `message` event. `event.data` is the string.
- Messages are JSON strings. The host sends only strings. The largest message is 1 MB.
- The panel can load `.html`, `.css`, `.js`, `.svg`, `.png`, `.woff2`, and `.json` files from the package, with relative paths. File names can have ASCII letters, digits, `.`, `-`, and `_`.
- Load scripts and styles from files in the package. Inline scripts and inline styles do not run. The panel cannot use the network.
- `localStorage` and cookies do not work in a panel. Keep state in the mod, or in its settings through `host`.

### Theme

The host puts a link to `t-rustable-theme.css` at the start of the head of the panel HTML. The file sets these CSS variables on `:root`: `--tr-bg`, `--tr-surface`, `--tr-surface-2`, `--tr-text`, `--tr-text-muted`, `--tr-border`, `--tr-accent`, `--tr-accent-text`, `--tr-focus`, `--tr-danger`, `--tr-warning`, `--tr-success`, `--tr-radius`, and `--tr-font-size`. Use them, so the panel follows the light, dark, and high-contrast themes.

```css
body {
  background: var(--tr-surface);
  color: var(--tr-text);
}
```

Do not use the name `t-rustable-theme.css` for a file of your own.

### Panel life

The host loads the panel again when the user changes the theme. Thus the panel must ask its mod for its state when it loads, for example with a message such as `{"kind":"hello"}`. The mod answers with `post-to-panel`.

While a panel is open, its mod gets `tool.on-document-changed` for each change of the active document, also when the mod is not the active tool.

## Dialogs and messages

| Function | Notes |
|---|---|
| `notify(level, message)` | A short message: info, success, warning, error. |
| `confirm(title, message)` | A yes or no dialog. Returns true for yes. The time limit of the call stops while the dialog is open. |

## Selection

`context()` returns the state of the active tab: `document-open`, `active-page`, `selected-pages`, `selected-objects`, `zoom`. It gives IDs only. Use the `doc` of the current call to read the document.
