# mods/builtin

Built-in tools. Each built-in tool is a mod and uses only the public mod API, the same API as an external mod. If a built-in tool needs something that the API does not give, the API changes for all mods.

License: PolyForm-Noncommercial-1.0.0.

| Mod | World | What it does |
|---|---|---|
| `page-tools` | full | Rotates, deletes, moves, and inserts the selected pages. In the CLI, a batch job takes JSON with page numbers that start at 1, for example `{"action": "rotate", "pages": [1, 3], "degrees": 90}`. The actions are `rotate`, `delete`, `move-up`, `move-down`, and `insert-blank-after` (with `page`). |
| `shapes` | interactive | Draws a rectangle, an ellipse, or a line with a drag. Shift keeps the shape square, or the line at a multiple of 45 degrees. Escape cancels. |
| `add-text` | interactive | Click on a page, type, then press Enter. Backspace removes the last character, and Escape cancels. The caret uses `document.measure-text` (API 0.1.1). |
| `add-image` | interactive | Opens a file dialog for a PNG or JPEG image. A click adds it at 96 pixels per inch, and a drag sets its size. |

## Build

From the repository root:

```
cargo xtask build-mods
```

The output is in `target/mods/<mod id>/`: `manifest.json` and the component. The `entry` of each manifest must be the name of the core module that cargo builds, for example `page_tools.wasm`.

The test mods in `mods/fixtures/` are in the same workspace. They are not shipped.
