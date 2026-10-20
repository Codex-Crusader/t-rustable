# 0018. GUI threads, tiles, and the bridge to mods

Status: Accepted (2026-10-20). Milestone 4. The owner approved the plan. The owner also selected the `tool` field in the manifest (decision 7).

## Context

Milestone 4 adds the Tauri 2 and Svelte 5 GUI on top of the core, the engine, and the plugin host. The GUI must solve these problems:

1. `ModHost` takes `&mut self`. `ui.confirm` and `files.pick` block until the user answers. A Tauri command must not block the main thread.
2. A tile render takes 3 to 100 ms. If the tile handler holds the session lock during the render, an edit waits for each tile.
3. Mods, the UI, and the core all use page positions. Two copies of the coordinate math can drift.
4. A mod panel is code from a third party. It must not reach Tauri IPC or the host page.
5. Shortcuts come from the app, the user, and the mods. They can conflict.
6. API 0.1 had no way to tell a pointer tool from a command that runs once.

## Decision

1. Threads. One mod thread owns `ModHost`. A command sends a closure to it and waits on a thread of the blocking pool. The engine keeps its own thread. The main thread never blocks.
2. Bridge. `GuiBridge` implements `UiBridge`:
   - Registrations, overlays, `notify`, `progress`, and panel messages become Tauri events.
   - `confirm` sends `confirm-request` with an ID and waits on a channel until the UI calls `answer_confirm`.
   - `context` reads a snapshot that the UI pushes on each change of the tab, the page, the selection, or the zoom. A mod call needs no round trip to the UI.
   - `files.pick` uses `tauri-plugin-dialog` from Rust. The page has the capability `core:default` only.
3. Tiles. The `trtile` scheme serves PNG tiles of 512 by 512 device pixels. The path is `/<doc>/<page-id>/<revision>/<scale>/<x>_<y>.png`, where `scale` is device pixels per point times 1000. `Session::submit_render` resolves the page under the session lock and returns a `RenderTicket`. The handler waits for the ticket without the lock. The UI cancels tiles that leave the view. After `engine-restarted`, the handler submits the render one more time. An LRU cache with a budget of 256 MB holds the PNG data.
4. Revisions. The app keeps a revision for each page. Each change (from the UI, a mod, undo, or redo) increments the revision of each affected page. The UI gets one `document-changed` event for each change. The active tool and each mod with an open panel get `on-document-changed`.
5. Coordinates. All conversion is in Rust (`wire.rs`), with `PageView` at zoom 1. The UI works in display points at zoom 1 and multiplies by its CSS scale. The UI has no copy of the math.
6. Panels. The `trpanel` scheme serves the files of a loaded mod at `/<mod-id>/<file>`:
   - Only `.html`, `.css`, `.js`, `.svg`, `.png`, `.woff2`, and `.json` files, and only inside the mod folder.
   - Each response has the panel CSP from the M1 spike.
   - A virtual file `t-rustable-theme.css` holds the theme variables. The scheme puts a link to it into the head of each HTML file. Panels load again when the theme changes.
   - Panels run in `<iframe sandbox="allow-scripts">`. The host accepts a message only when `event.source` is that iframe, and sends it only to the mod of that panel.
7. Tool actions. A manifest action can have `"tool": true`. When the user selects it, the host makes the mod the active tool, and the mod gets pointer and key events. Other actions run once.
8. Shortcuts. The app defaults come first, then `keymap.json` in the configuration folder, then the suggestions of the mods. The user file always wins. A mod suggestion that conflicts is not bound, and the palette shows "Not bound". A shortcut needs Ctrl, Alt, or Meta, or an F key.
9. Recovery. On start, the app offers to recover the journals that no open session uses. Only the first running instance does this. It holds a lock file in the sessions folder. Closing the window goes through the UI, which asks about unsaved changes and then closes all sessions.
10. Self-test. `t-rustable-app --self-test [pdf]` runs a scenario through the same calls as the UI and writes a JSON report. Without a file, it uses a generated six-page PDF. It includes the M1 panel escape checks. CI runs it on Windows and on Linux under `xvfb-run`.

## Consequences

- The main thread stays free. A slow mod blocks other mod calls, but not the viewer or the engine.
- Edits do not wait for tiles, and tiles do not wait for edits.
- Coordinates have one source. The UI cannot use a different rotation or crop box than the core.
- On Windows, Tauri puts its bridge objects into each frame, also into panels. They do nothing, because Tauri drops IPC from a panel (M1 report, spike 2). The self-test fails if a panel reaches the command `self_test_secret`, so a Tauri update cannot change this without notice.
- A panel must ask its mod for its state when it loads, because a theme change loads it again.
- Pointer tools have no keyboard method yet. Each action is available from the palette, but drawing a shape needs a pointer. This is an open accessibility item.
- Toolbar items that mods register show as text. Icons come later.
