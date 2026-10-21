# Your first mod in 15 minutes

You make a mod with two actions: "Rotate page" (from the template) and "Delete page" (you add it). You write it in Rust and build it as a WebAssembly component.

You need T-Rustable with the `t-rustable` command. In the T-Rustable repository, `cargo run -p t-rustable-cli --` gives the same command.

## 1. Install the tools (5 minutes)

1. Install Rust with rustup: https://rustup.rs
2. Add the WebAssembly target:

   ```
   rustup target add wasm32-unknown-unknown
   ```

Do not use the `wasm32-wasip2` target. It adds WASI imports, and T-Rustable refuses a mod with WASI imports.

## 2. Make the mod folder (1 minute)

```
t-rustable mods new "Page tools" --id com.yourname.page-tools
```

The command writes the folder `page-tools` from the template. It sets the name, the ID, the crate name, and `entry` (`page_tools.wasm`). The SDK comes from the T-Rustable git repository. In `manifest.json`, set `author` and `description`.

## 3. Read the code (2 minutes)

`src/lib.rs` has one type and one method:

```rust
struct RotatePage;

impl Tool for RotatePage {
    fn run_action(document: Option<&Doc>, action_id: String, _args: String) -> Result<(), ApiError> {
        // 1. Check the action ID.
        // 2. Get the document and the active page.
        // 3. Run one command in one transaction.
    }
}

export_interactive!(RotatePage);
```

- `Tool` is the trait for interactive mods. Each method has a default, so you implement only what you need.
- The host calls `run_action` when the user runs an action from `manifest.json`.
- `ui::context()` gives the active page and the selection as IDs.
- `run_commands` opens a transaction, adds the commands, and commits it. One transaction is one undo step.
- `export_interactive!` exports the type as the `tool` interface.

## 4. Add an action (4 minutes)

Add the action to `manifest.json`:

```json
"actions": [
  { "id": "rotate-page", "title": "Rotate page" },
  { "id": "delete-page", "title": "Delete page", "description": "Deletes the current page." }
]
```

Replace `run_action` in `src/lib.rs`:

```rust
fn run_action(document: Option<&Doc>, action_id: String, _args: String) -> Result<(), ApiError> {
    let document = document.ok_or_else(|| api_error("no-document", "Open a document first."))?;
    let page = ui::context().active_page.ok_or_else(|| api_error("no-page", "Select a page first."))?;
    let (label, command) = match action_id.as_str() {
        "rotate-page" => ("Rotate page", Command::RotatePage { page, degrees: 90 }),
        "delete-page" => ("Delete page", Command::DeletePage(page)),
        other => return Err(api_error("unknown-action", format!("Unknown action {other:?}."))),
    };
    run_commands(document, label, vec![command])?;
    Ok(())
}
```

## 5. Build and pack (2 minutes)

On Linux, run `./build.sh`. On Windows, run `./build.ps1`.

The script runs `cargo build --release`, then `t-rustable mods pack .`. The pack step makes a component of the build, checks it, and writes `com.yourname.page-tools-0.1.0.tmod`. It prints the permissions:

```
Permissions: document.read, document.edit, ui
```

The imports decide the permissions: `document` needs `document.read`, `edit` needs `document.edit`, `ui` needs `ui`. The `permissions` in `manifest.json` must match exactly, or the pack step stops and names the difference. If you add `host::log` calls, nothing changes: `host` needs no permission. If you add `files::pick`, add `files.pick` to the manifest.

## 6. Install and try it (1 minute)

In T-Rustable, select Mods, Install mod, and pick the `.tmod` file. Read the permission screen, and select Install. Open a PDF, and run Rotate page or Delete page from the Tools menu or the command palette (Ctrl+K).

While you work on the mod, use developer mode instead. It loads the folder again after each `cargo build --release`. See [packages and developer mode](packaging.md).

## Next steps

- A pointer tool: implement `on_pointer`, return overlays while the user drags, and commit a transaction on the `up` event. See [worlds](worlds.md) and [types](types.md).
- A panel: add an HTML file and a `panels` entry, and handle `on_panel_message`. See [ui](ui.md).
- A batch job for the command line: implement `BatchJob` and use `export_batch!`, or `export_full!` for both. See [worlds](worlds.md).
