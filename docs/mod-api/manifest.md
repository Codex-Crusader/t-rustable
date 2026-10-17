# manifest.json

Each mod has a `manifest.json`. The schema is [manifest.schema.json](manifest.schema.json) (JSON Schema 2020-12). The SDK checks the same rules in `t_rustable_sdk::manifest::Manifest::parse`, plus rules that a schema cannot express.

## Fields

| Field | Required | Rule |
|---|---|---|
| `id` | yes | 3 to 64 characters. Lowercase letters and digits, separated by single dots or dashes. Starts with a letter. Example: `com.example.rotate-page`. |
| `name` | yes | Shown to the user. |
| `version` | yes | `MAJOR.MINOR.PATCH`, with an optional pre-release such as `1.0.0-beta.1`. |
| `apiVersion` | yes | `"0.1"` |
| `author` | yes | |
| `description` | yes | |
| `license` | no | SPDX expression, for example `MIT`. |
| `permissions` | yes | `document.read`, `document.edit`, `ui`, `files.pick`. Must match the imports. |
| `entry` | yes | Relative path of the `.wasm` component in the package. |
| `worlds` | yes | `interactive`, `batch`, or both. |
| `panels` | no | `id`, `title`, `entry` (relative path of an `.html` file). |
| `actions` | no | `id`, `title`, `description`, `shortcut` (a suggestion). |

Paths use forward slashes. They cannot start with `/`, contain `\` or `:`, or have empty, `.`, or `..` parts.

## Rules beyond the schema

- `ui` and `files.pick` need the `interactive` world.
- Panels and actions need the `interactive` world.
- Panels need the `ui` permission.
- Panel IDs and action IDs are unique in the mod.

## Example

```json
{
  "id": "com.example.rotate-page",
  "name": "Rotate page",
  "version": "0.1.0",
  "apiVersion": "0.1",
  "author": "Your name",
  "description": "Rotates the current page by 90 degrees.",
  "license": "MIT",
  "permissions": ["document.read", "document.edit", "ui"],
  "entry": "rotate_page.wasm",
  "worlds": ["interactive"],
  "actions": [
    { "id": "rotate-page", "title": "Rotate page", "shortcut": "Ctrl+Shift+R" }
  ]
}
```
