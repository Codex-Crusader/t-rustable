# Mod API 0.1

A T-Rustable mod is a WebAssembly component. It uses the WIT package `t-rustable:api@0.1.1` in `wit/`. Built-in tools are mods too, and they use the same API.

Status: design for review (REVIEW 2). There is no host until Milestone 3. The template and the guide are checked by building them, not by running them in the app.

License of this folder: MIT. You can copy the code samples into any mod.

## Start here

- [Your first mod in 15 minutes](first-mod-in-15-minutes.md)
- [Mod template](../../mods/template/)

## Reference

| Page | Content |
|---|---|
| [types](types.md) | IDs, geometry, colors, errors, overlay shapes |
| [document](document.md) | Read pages, text, objects, annotations, form fields, metadata |
| [edit](edit.md) | Transactions of core commands |
| [host](host.md) | Log, progress, cancel, settings |
| [ui](ui.md) | Actions, toolbar, panels, overlays, dialogs, selection |
| [files](files.md) | Files that the user picks |
| [worlds](worlds.md) | The `tool` and `batch-job` exports and the worlds |
| [permissions](permissions.md) | Import to permission table |
| [manifest](manifest.md) | `manifest.json` and its [JSON Schema](manifest.schema.json) |
| [versioning](versioning.md) | How the API changes without breaking mods |

## Rules that apply to every function

- Coordinates are PDF points (1/72 inch) in the user space of the page, before page rotation. The origin is often, but not always, the bottom left of the visible page: use `page-info.crop-box`. Only `to-display` and `from-display` use screen coordinates.
- Pages and objects have stable IDs. The API never uses page indices.
- Every function that can fail returns `result<_, api-error>`. Error codes are in `docs/errors.md`.
- Every call has a memory limit and a time limit. The time limit stops while the host waits for the user (`ui.confirm`, `files.pick`). A mod that fails 3 times in a row is disabled.
- A mod has no file or network access. It gets files only through `files.pick`.
- A mod cannot keep a document. The host lends a `doc` to each call. Every handle from the host (`doc`, `transaction`, `picked-file`) is valid only during the export call that made it. Store IDs, not handles.

## Check against later built-in tools

The API must serve every built-in tool with no special access. This table shows how each planned tool uses API 0.1, and what comes later as an addition (new functions only, see [versioning](versioning.md)).

| Tool (milestone) | Uses in 0.1 | Later addition |
|---|---|---|
| Page tools: delete, move, rotate, insert (M3) | `ui.context` selection, `edit` page methods | None |
| Add text (M3) | `on-pointer`, caret overlay, `on-text-input`, `edit.add-text` | None |
| Add image (M3) | `files.pick`, image overlay preview, `edit.add-image` | None |
| Shapes (M3) | `on-pointer` drag, overlays, `edit.add-shape` | Arrowheads on lines: a new `transaction` method |
| Replace text (later) | `document.text-runs` | `transaction.replace-text`, a font coverage query on `doc` |
| Redaction (later) | overlays for marks, `document.search` | `transaction.redact-area`, `transaction.apply-redactions` |
| Forms (later) | `document.form-fields`, form field highlight overlays | `transaction.set-form-field` |
| Annotations (later) | `document.annotations`, `edit.remove-object` | `transaction.add-annotation` |
| OCR (later) | `host.progress`, `host.is-cancelled` | `doc.render-page` for pixels, `transaction.add-invisible-text` |
| Outline panel (later) | | `doc.outline` |
