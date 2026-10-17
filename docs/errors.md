# Errors

Status: format and the codes of the mod API 0.1. More codes come with the features that use them.

## Format

Each error that the user or a mod can see has these fields:

| Field | Meaning |
|---|---|
| `code` | A stable code in `kebab-case`, for example `not-found`. The code never changes after a release. |
| `message` | A short text in Simplified Technical English. It tells what went wrong. |
| `hint` | Optional. It tells the user what to do next. |
| `details` | Optional. Technical data for the log, for example a `PageId`. |

In the mod API, this is the record `api-error` (`wit/types.wit`). It also has `command-index`, which is set when a transaction commit fails.

Rules for messages:

- Tell what went wrong, then what to do. Example: "The page does not exist. Reload the document and try again."
- Do not blame the user.
- Do not show a stack trace in the UI. Write it to the log.

## Codes

Codes of the mod API 0.1. A code is never removed or renamed. New codes can come in any release, so a mod must handle an unknown code as a general failure.

| Code | Source | Meaning |
|---|---|---|
| `invalid-argument` | any | An argument is not valid, for example a rotation that is not a multiple of 90, or JSON args that do not parse. |
| `not-found` | `document`, `edit` | The page ID or object ID does not exist in this document. |
| `no-document` | mods | The action needs an open document. |
| `no-page` | mods | The action needs a selected page. |
| `no-display` | `document.to-display`, `document.from-display` | There is no display, because the call runs in the batch world. |
| `unknown-action` | `tool.run-action` | The mod has no action with this ID. |
| `unsupported-format` | `edit.add-image`, `ui.register-overlay-image` | The image is not PNG or JPEG, or the file is damaged. |
| `command-failed` | `edit.transaction.commit` | A command could not be applied. `command-index` names it. Nothing changed. |
| `limit-exceeded` | `host.setting-set`, `ui` | A size limit was reached, for example 64 KB of settings per mod. |
| `file-too-large` | `files.picked-file.read` | The file is larger than 256 MB. |
| `cancelled` | any long call | The user cancelled the call. |
| `timeout` | host | The mod used more than its time limit for one call. The host stopped it. |
| `out-of-memory` | host | The mod used more than its memory limit. The host stopped it. |
| `internal` | host | An error in T-Rustable itself. The log has the details. |
