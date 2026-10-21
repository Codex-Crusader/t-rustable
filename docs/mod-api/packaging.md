# Packages and developer mode

A mod goes to users as one `.tmod` file. Users install it with Mods, Install mod. There is no mod store and no download in the app.

## Make a package

In your mod folder, run:

```
cargo build --release
t-rustable mods pack .
```

`mods pack` does these steps:

1. It reads `manifest.json`.
2. It takes the core module from `target/wasm32-unknown-unknown/release/<entry>` and makes a component of it. If there is no cargo output, it uses the `entry` file in the folder. Use `--module <file>` for another file.
3. It adds the panel folders and `README.md` or `LICENSE` if they exist.
4. It applies the package rules below and checks the imports against the permissions.
5. It writes `<id>-<version>.tmod` into the folder. Use `-o <file>` for another name.

The output names the permissions and the SHA-256 of the package. Users see the same permissions and the first 12 characters of the SHA-256 before they install.

## Package rules

A `.tmod` file is a zip file. The app refuses a package that breaks one of these rules. The error codes are in `docs/errors.md`.

| Rule | Value |
|---|---|
| Package size | 32 MiB or less |
| Number of files | 1,000 or less |
| One unpacked file | 64 MiB or less |
| All unpacked files | 128 MiB or less |
| Compression ratio of files over 1 MiB | 100 or less |
| Path length | 200 bytes or less |
| Folder depth | 8 or less |

- Names: ASCII letters, digits, `.`, `-`, `_`, and `/` only. No part can start or end with `.`.
- No Windows reserved names (`CON`, `NUL`, `COM1`, and so on), also not with an extension.
- No two names that differ only in case.
- Only stored or deflate compression. No encryption. No links.
- Files: `manifest.json`, the `entry` component, the files in your panel folders, and optional `README.md`, `LICENSE`, `LICENSE.txt`, or `LICENSE.md`. Panel files must be html, css, js, svg, png, woff2, or json. Any other file stops the install.
- Mod IDs that start with `t-rustable.` or `test.` are reserved.
- The component must import only what the permissions allow (see [permissions](permissions.md)), and it must export what the `worlds` need.
- The component must not import a function that the user's version of T-Rustable does not have. See [versioning](versioning.md).

## Install and update

The user picks the `.tmod` file, or opens it with T-Rustable. The app checks the package, then shows the permission screen. The mod goes to the data folder: `%APPDATA%\T-Rustable\mods` on Windows and `$XDG_DATA_HOME/t-rustable/mods` on Linux.

To update a mod, increase `version` and make a new package. The user installs it the same way. If the new version asks for a permission that the old version did not have, the permission screen marks it as new. The settings of the mod stay.

## Developer mode

Developer mode loads your mod folder directly, so you do not make a package after each change.

1. Open Mods, Manage mods.
2. Select Developer mode.
3. Select Add developer folder, and pick your mod folder.

The app packs the folder with the rules above and loads it. Then it checks the folder once each second. After `cargo build --release`, or after a change to `manifest.json` or a panel file, the mod loads again, and its open panels load again. If the new build fails, the old build stays loaded, and the app shows the error. Load again in the mod manager does the same at once.

While developer mode is on, the status bar shows "Developer mode", and your mod has the mark "(Developer)". A developer folder replaces an installed mod with the same ID until you turn off developer mode.
