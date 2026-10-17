# Your first mod in 15 minutes

You make a mod with two actions: "Rotate page" (from the template) and "Delete page" (you add it). You write it in Rust and build it as a WebAssembly component.

Status: API 0.1 is in review. The app cannot load mods yet: the mod host comes in Milestone 3, and `.tmod` packages come in Milestone 5. You can build and check your mod today. You cannot run it in the app yet.

## 1. Install the tools (5 minutes)

1. Install Rust with rustup: https://rustup.rs
2. Add the WebAssembly target:

   ```
   rustup target add wasm32-unknown-unknown
   ```

3. Install `wasm-tools`. Download the archive for your platform from https://github.com/bytecodealliance/wasm-tools/releases, and put `wasm-tools` on your `PATH`.

Do not use the `wasm32-wasip2` target. It adds WASI imports, and T-Rustable refuses a mod with WASI imports.

## 2. Copy the template (1 minute)

Copy `mods/template` from the T-Rustable repository to a new folder, for example `page-tools`.

If your folder is outside the T-Rustable repository, change the SDK dependency in `Cargo.toml` to:

```toml
t-rustable-sdk = { git = "https://github.com/Codex-Crusader/t-rustable" }
```

In `Cargo.toml`, change `name` to `page-tools`. In `manifest.json`, change `id` to your own ID (for example `com.yourname.page-tools`), and set `name`, `author`, and `entry` (`page_tools.wasm`). In `build.sh` and `build.ps1`, change `rotate_page` to `page_tools`.

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

## 5. Build (2 minutes)

On Linux, run `./build.sh`. On Windows, run `./build.ps1`.

The script runs `cargo build --release`, then `wasm-tools component new` to make the component, and `wasm-tools validate` to check it. Then it prints the imports:

```
import t-rustable:api/types@0.1.0;
import t-rustable:api/document@0.1.0;
import t-rustable:api/edit@0.1.0;
import t-rustable:api/ui@0.1.0;
```

## 6. Check the permissions (1 minute)

The imports decide the permissions: `document` needs `document.read`, `edit` needs `document.edit`, `ui` needs `ui`. The `permissions` in `manifest.json` must match exactly. In the T-Rustable repository, run:

```
wasm-tools component wit page_tools.wasm > page_tools.wit
cargo run -p t-rustable-sdk --example check-mod -- path/to/manifest.json page_tools.wit
```

If you add `host::log` calls, nothing changes: `host` needs no permission. If you add `files::pick`, add `files.pick` to the manifest.

## Next steps

- A pointer tool: implement `on_pointer`, return overlays while the user drags, and commit a transaction on the `up` event. See [worlds](worlds.md) and [types](types.md).
- A panel: add an HTML file and a `panels` entry, and handle `on_panel_message`. See [ui](ui.md).
- A batch job for the command line: implement `BatchJob` and use `export_batch!`, or `export_full!` for both. See [worlds](worlds.md).
