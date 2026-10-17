# files

Interface `t-rustable:api/files@0.1.0`. Permission: `files.pick`. Interactive world only.

A mod has no file system access. The user picks each file in a host dialog.

| Function | Notes |
|---|---|
| `pick(options)` | Shows an open dialog. `options`: `title` and `filters` (name and extensions, for example `{ name: "Images", extensions: ["png", "jpg"] }`). Returns none when the user cancels. The time limit of the call stops while the dialog is open. |
| `picked-file.name()` | The file name, without the folder. |
| `picked-file.size()` | Size in bytes. |
| `picked-file.read()` | The content. Error `file-too-large` above 256 MB. |

A dialog to save files is not in 0.1. It can come later as a new function.
