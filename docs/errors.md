# Errors

Status: format, the codes of the mod API 0.1, and the codes of the core and the plugin host (Milestone 3). More codes come with the features that use them.

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
| `expired` | `edit`, `files` | The transaction or picked file belongs to an earlier call. Handles from the host are valid only during the export call that made them. |
| `command-failed` | `edit.transaction.commit` | A command could not be applied. `command-index` names it. Nothing changed. |
| `unsupported-content` | `edit.transaction.commit` | The page has content that the engine cannot keep when it writes the page again, for example a Type 3 font. Nothing changed. |
| `missing-glyph` | `edit.add-text`, `document.measure-text` | The font cannot show some characters. The hint names them. |
| `limit-exceeded` | `host.setting-set`, `ui` | A size limit was reached, for example 64 KB of settings per mod. |
| `file-too-large` | `files.picked-file.read` | The file is larger than 256 MB. |
| `cancelled` | any long call | The user cancelled the call. |
| `timeout` | host | The mod used more than its time limit for one call. The host stopped it. |
| `out-of-memory` | host | The mod used more than its memory limit. The host stopped it. |
| `internal` | host | An error in T-Rustable itself. The log has the details. |

## Codes of the core

The GUI and the CLI get these codes from the core. A mod can get the first four through the API too.

| Code | Meaning |
|---|---|
| `signed-document` | The document has a digital signature, and a save writes the whole file again, which makes the signature invalid. The caller must ask the user first, then save with `allow_invalidate_signatures`. |
| `file-in-use` | Another program has the target file open, so the save cannot replace it. The target did not change. |
| `password-required` | The document is encrypted. Open it again with the password. |
| `damaged-file` | The file is not a PDF file, or it is too damaged to open. |
| `io-error` | A file operation failed. `details` has the system error. |
| `unsaved-changes` | The document has changes that are not saved. The GUI asks the user before it closes the document. |
| `nothing-to-undo` | There is no transaction to undo. |
| `nothing-to-redo` | There is no transaction to redo. |
| `replay-failed` | A recovered journal does not replay on the original document, for example because the original changed. |
| `engine-restarted` | The engine restarted, or it lost the document, before a submitted render was done. Submit the render again. The session then reloads the document. |

## Codes of the plugin host

The host gives these codes when a mod call fails as a whole. They are not `api-error` values that the mod returns. Commits that the mod made before the failure stay in the document, and the user can undo them.

| Code | Meaning |
|---|---|
| `invalid-mod` | The mod cannot load: the manifest is not valid, the component is not valid, its imports do not match the manifest permissions, or it does not export what its manifest worlds need. |
| `no-export` | The mod does not export the interface for this call, for example a batch call to an interactive-only mod. |
| `mod-failed` | The mod stopped because of an error in its own code (a trap). The host gives it a new instance. |
| `timeout` | The mod used more than its time limit for one call. Time in host calls does not count. |
| `out-of-memory` | The mod used more than its memory limit. |
| `mod-disabled` | The mod failed 3 times in a row, so the host disabled it. The user can turn it on again. |
| `not-found` | No mod with this ID is loaded. |
| `internal` | An error in the host itself. The log has the details. |
