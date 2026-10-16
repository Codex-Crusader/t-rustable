# T-Rustable architecture

This document describes how T-Rustable works. The locked decisions are in `CLAUDE.md`. The reasons for each decision are in `docs/adr/`.

Status: design for Milestone 0. The spikes in Milestone 1 can change details. Items marked "Verify in M1" are not tested yet.

## 1. Components

```
          +------------------+        +------------------+
          |  t-rustable-app  |        |  t-rustable-cli  |
          |  Tauri 2 + ui/   |        |  batch front end |
          +--------+---------+        +---------+--------+
                   |                            |
                   +-------------+--------------+
                                 |
                       +---------v---------+
                       |  t-rustable-core  |  sessions, IDs, commands,
                       |                   |  transactions, undo, journal, save
                       +----+---------+----+
                            |         |
            +---------------v--+   +--v---------------------+
            | t-rustable-engine|   | t-rustable-plugin-host |
            | engine actor:    |   | wasmtime, WIT bindings,|
            | PDFium, qpdf     |   | permissions, limits    |
            +------------------+   +-----------+------------+
                                               |
                                     +---------v---------+
                                     |  t-rustable-sdk   |  MIT
                                     +-------------------+
```

| Crate | Responsibility | License |
|---|---|---|
| `t-rustable-sdk` | Shared types for mods and Rust helpers for mod authors. | MIT |
| `t-rustable-core` | Sessions, `IdMap`, command registry, transactions, checkpoints, undo and redo, journal, save pipeline. | PolyForm NC |
| `t-rustable-engine` | The engine actor thread. Adapters for PDFium and qpdf. | PolyForm NC |
| `t-rustable-plugin-host` | Loads WASM components, checks imports against the manifest, applies limits, implements the WIT interfaces. | PolyForm NC |
| `t-rustable-app` | Tauri shell. Connects the Svelte UI to the core. | PolyForm NC |
| `t-rustable-cli` | Batch front end. Uses the same core as the app. | PolyForm NC |

Dependency rules:

- MIT parts never depend on PolyForm parts. `t-rustable-sdk` and `wit/` have no dependency on any other crate in this workspace.
- The GUI and the CLI contain no editing logic. All editing logic is in the core.
- Only the engine crate calls PDFium or qpdf.

## 2. Engine actor protocol

PDFium is not thread-safe. One engine thread owns all PDFium calls. Other threads send messages to it.

Request:

| Field | Meaning |
|---|---|
| `request_id` | A unique `u64`. The reply carries the same ID. |
| `priority` | `Interactive` (visible tiles, user edits), `Normal` (thumbnails, search), `Background` (prefetch). |
| `session_id` | The session that the request is for. |
| `body` | The operation, for example open, render tile, apply command, save, extract text. |
| `cancel` | A shared cancel flag. The sender sets it to cancel the request. |

Rules:

1. The engine keeps one queue per priority. It always takes the next request from the highest non-empty queue.
2. Before it starts a request, the engine checks the cancel flag. If the flag is set, it replies `Cancelled` and does no work.
3. A long render uses PDFium progressive render. The pause callback reads the cancel flag, so a render can stop between steps. Verify in M1.
4. Each request gets exactly one reply: `Ok(result)`, `Err(error)`, or `Cancelled`.
5. A reply goes back on a channel that the sender gives with the request.
6. A Rust panic in the engine thread is caught at the request boundary. The engine replies with an error and marks that session as broken. PDFium state can be bad after a panic, so the engine also restarts PDFium before the next request. A restart closes the documents of all sessions, so the core reloads each open session from its journal.
7. A native crash (for example a segfault in PDFium or qpdf) cannot be caught. It stops the app. On the next start, the journal recovers the sessions.

The engine runs in the app process for v1. A later milestone can move it to a child process. Then the protocol goes over stdin and stdout pipes, never over a TCP socket.

## 3. Stable IDs

Commands and the mod API never use page indices or object indices. Indices change when pages move or objects are deleted.

- `PageId`: a random 128-bit ID. The core gives one to each page when it opens the document or when a command creates a page.
- `ObjectId`: a random 128-bit ID for each page object (text, image, path, form XObject) and each annotation.
- The `IdMap` maps each ID to the current PDFium handle or index. Only the core and the engine read the `IdMap`.
- A command that creates a page or an object contains the new ID. The core makes the ID when the command is created, not when it is applied. Thus replay makes the same IDs again, and replay stays deterministic.
- When the core opens the original document, it makes the IDs from a seed, in a fixed order (pages in order, objects in page order). `manifest.json` in the journal stores the seed, so a replay of the same original gives the same map.

## 4. Coordinates

- Commands store native PDF user space coordinates: points (1/72 inch), `f64`, origin at the bottom left of the page, before the page rotation.
- The mod API gives `to-display` and `from-display`. They convert between PDF coordinates and display coordinates (pixels, origin at the top left, after rotation and zoom).
- The host sends pointer events to mods in page coordinates.
- The UI does display conversion only for drawing. The UI never writes display coordinates into a command.

## 5. Commands

A command is plain data: a type name and fields. Example: `RotatePage { page: PageId, degrees: 90 }`.

Each command type has three handlers in the command registry:

| Handler | Job |
|---|---|
| `validate` | Checks the command against the current document. No change. Returns a clear error if the command is not valid. |
| `apply` | Changes the document through the engine. Must be deterministic: the same document and the same command give the same result. |
| `describe` | Returns a short text for the UI, for example "Rotate page 3 by 90 degrees". |

Rules:

- Only the core defines command types. Mods cannot add command types (ADR 0004).
- A command contains all of its inputs. An image command refers to an asset by its SHA-256 hash. It never refers to a file path.
- `apply` never reads the clock, a random source, or the file system.
- Each command type has unit tests for all three handlers.

## 6. Transactions

A transaction is a list of commands with a label. The user, a built-in mod, or an external mod submits it.

Lifecycle:

1. The submitter sends `submit(doc, label, commands)`.
2. The core runs `validate` for each command in order, against the state after the commands before it.
3. If one command is not valid, the core rejects the full transaction. Nothing changes.
4. The core runs `apply` for each command. If one `apply` fails, the core rolls back to the state before the transaction. Rollback uses the undo mechanism (section 7).
5. The core appends the transaction to the journal (section 8).
6. The core sends a `document-changed` event with the affected `PageId`s.

The transaction is the unit of undo. One user action is one undo step.

## 7. Undo with checkpoints

The core keeps the original document and the list of transactions. A document state is "the original plus the first N transactions".

- A checkpoint is a saved copy of the document bytes after transaction N. The core makes a checkpoint every K transactions (start value: K = 20) and after each expensive transaction. The core stores checkpoints in a temporary folder of the session.
- Undo to state N: load the nearest checkpoint at or before N, then replay the transactions up to N.
- Redo: replay the next transaction.
- A new transaction after an undo removes the redo branch.
- The core keeps a limited number of checkpoints. It removes old checkpoints first. The original is always kept.

Replay does not need any mod, because the journal stores core commands only.

## 8. Journal

The crash journal uses the `.trex` format (ADR 0008):

```
manifest.json      format version, app version, ID map seed, document info
original.pdf       the original file, unchanged
commands.jsonl     one transaction per line: label, source, commands
assets/<sha256>    images and fonts that commands refer to
```

- The core writes the journal to the app data folder (section 12) while the session is open. It is a folder with these files, not a zip, so that appends are cheap.
- After each transaction, the core appends one line to `commands.jsonl` and flushes it to disk.
- A partial last line (from a crash) is ignored on recovery.
- When the app starts, it lists the journals of sessions that did not close correctly and offers to recover them.
- When the user closes a session, the core deletes its journal.
- A `.trex` project file is the same content in a zip. The app warns that a `.trex` file contains the original document.

## 9. Save pipeline

The app never changes a file unless the user saves.

1. Apply: the current state is already in the engine.
2. PDFium save to bytes.
3. qpdf post-process: check the structure, repair if needed, linearize or compress if the user selected it.
4. Write the result to a temporary file in the same folder as the target.
5. Flush the temporary file to disk.
6. Rename the temporary file to the target name. On the same file system, this is atomic on Linux. On Windows, it depends on the API that the rename uses. Verify in M3.

Special rules:

- If the session has any redaction, the save is always a full rewrite, never an incremental save.
- After a redaction, the save verifies the output: no text in redacted areas, and no redacted string anywhere in the output bytes. Any match stops the save.
- The app opens an encrypted PDF only with the user's password.

## 10. Rendering

The UI shows pages as tiles.

- Tile size: 512 by 512 device pixels.
- Zoom buckets: the engine renders at fixed scales (start values: 25, 50, 100, 200, 400 percent, multiplied by the device pixel ratio). Between buckets, the UI scales the tiles of the nearest bucket with CSS. When zoom stops, the UI requests tiles at the correct bucket.
- Two passes: first a fast low-resolution pass for the full visible area, then full-quality tiles. The user sees something at once.
- Priority: visible tiles are `Interactive`. Tiles near the viewport are `Background` prefetch. Tiles that leave the viewport are cancelled.
- Cache: an LRU cache with a memory budget holds encoded tiles. The key is session, `PageId`, page revision, bucket, and tile position.
- Invalidation: each transaction reports its affected `PageId`s. Only those pages get a new revision. Their old tiles leave the cache. Other tiles stay.
- Encoding: PNG for now. Verify the encode time in M1. If it is too slow, use a faster PNG setting or a raw format.

Targets: first page visible in less than 1 second, page render at 100 percent in less than 100 ms. Verify in M1.

## 11. GUI and core communication

Three channels connect the Svelte UI to the Rust core:

| Channel | Direction | Use |
|---|---|---|
| Tauri `invoke` | UI to core | Requests: open, submit a transaction, run a mod action, save, undo. Each returns a result. |
| Tauri events | Core to UI | Notifications: `document-changed` with affected pages, progress, mod overlays, errors. |
| Custom URI scheme | UI to core | Tile images. An `<img>` loads a URL like `trtile://localhost/<session>/<page-id>/<revision>/<bucket>/<x>_<y>.png`. The revision in the URL prevents stale tiles. |

On Windows, WebView2 serves a custom scheme as `http://trtile.localhost/...`, not as `trtile://localhost/...`. The CSP `img-src` must allow the form of each platform. Verify in M1.

Tauri 2 can send `invoke` as a fetch request to an IPC origin. The locked CSP has `connect-src 'none'`. Milestone 1 must find out whether this CSP blocks `invoke` or forces a slower fallback. If the CSP must change, that change needs a new ADR.

Mod panels:

- Each mod panel runs in an `<iframe sandbox="allow-scripts">` with a strict CSP. The iframe has no Tauri IPC. Verify in M1.
- The panel sends `postMessage` to the host page. The host page checks the source frame and forwards the message to the mod through `invoke`. The mod replies through the host.
- Panel messages are JSON strings.

## 12. Storage folders

The app uses the standard folders of each platform.

| Content | Linux | Windows |
|---|---|---|
| Configuration (`keymap.json`, settings) | `$XDG_CONFIG_HOME/t-rustable` | `%APPDATA%\T-Rustable\config` |
| Installed mods (`mods/<id>/`) | `$XDG_DATA_HOME/t-rustable/mods` | `%APPDATA%\T-Rustable\mods` |
| Journals and checkpoints | `$XDG_STATE_HOME/t-rustable/sessions` | `%LOCALAPPDATA%\T-Rustable\sessions` |
| Log | `$XDG_STATE_HOME/t-rustable/log` | `%LOCALAPPDATA%\T-Rustable\log` |

Mods get their settings through the `host` interface only. Settings have a size limit. Mods have no file access to these folders.

## 13. Offline guarantees

The app has no network access (ADR 0011).

- No HTTP client crate in the dependency tree. `cargo deny` fails CI if one appears.
- No Tauri `http`, `shell`, or `updater` plugins. Capabilities allow only dialogs and files that the user picks.
- CSP `connect-src 'none'`. All scripts, styles, and fonts are bundled.
- No auto-updater, no telemetry, no crash reporter. Errors go to the local log.
- Mods get no WASI network or socket access.

## 14. Failure handling

| Failure | Behavior |
|---|---|
| A command is not valid | The transaction is rejected. The UI shows the error from `validate`. Nothing changes. |
| `apply` fails | Rollback to the state before the transaction. The UI shows the error. |
| The engine panics (Rust panic) | The session is marked broken. The engine restarts PDFium, and the core reloads all open sessions from their journals. |
| PDFium or qpdf crashes (native crash) | The app stops. On the next start, the app offers to recover each session from its journal. |
| A mod traps, uses too much memory, or runs too long | The host stops the call and shows an error. After 3 failures in a row, the host disables the mod. |
| The save fails | The target file does not change, because the rename is the last step. The temporary file is deleted. |
| The app crashes | On the next start, the app offers to recover each session from its journal. |

Error codes and messages are listed in `docs/errors.md`.

## 15. Accessibility

Accessibility is a v1 requirement.

- Each page has a text layer with real text and character positions. It is used for selection and search, and screen readers read it.
- Each mod overlay shape has a required `label`. The host gives the label to screen readers.
- Each action is available from the keyboard and from the command palette.
- The app respects the reduce motion setting of the operating system.
- There is a high-contrast theme. Theme values are CSS variables, and the host injects them into mod panels.
- All UI strings are in one file, so they can be translated later.
