# 0019. Mod install, mod sources, and developer mode

Status: Accepted (2026-10-21)

## Context

ADR 0009 sets the rules for `.tmod` packages but not their values. Milestone 5 lets users install mods from `.tmod` files, turn mods off, update them, and load mod folders while they write a mod. The package reader is security-critical: it reads files that users download.

## Decision

### Package reader

A new crate, `t-rustable-tmod`, reads, checks, and writes packages. It has no WASM engine, so its fuzz target stays small. It uses the `zip` crate with deflate only.

| Limit | Value |
|---|---|
| Package file | 32 MiB |
| Entries | 1,000 |
| One unpacked file | 64 MiB |
| All unpacked files | 128 MiB |
| Compression ratio, for files over 1 MiB | 100 |
| Path length | 200 bytes |
| Folder depth | 8 |

- The reader counts the bytes that it unpacks. It does not trust the sizes in the zip headers.
- It reads the end records of the zip before the `zip` crate does. It refuses a zip64 archive, a directory over 1 MiB, and an entry count over the limit. If the count in the end record differs from the count that the `zip` crate gives, the package has two entries with the same name, and the reader refuses it.
- Only stored and deflate entries. No encrypted entries and no symbolic links.
- Names use ASCII letters, digits, `.`, `-`, `_`, and `/`. No empty part, no `.` or `..`, no part that starts with `.` or ends with `.`. No Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`), also with an extension. No two paths that differ only in case. No file with the same name as a folder.
- Content: `manifest.json`, the `entry` component, the panel files of the allowed types (html, css, js, svg, png, woff2, json) in the panel folders, and optional `README.md` and `LICENSE` files. Any other file stops the install.
- Mod IDs that start with `t-rustable.` or `test.` are reserved for built-in mods and test mods.

The reader is fuzzed with cargo-fuzz in CI. A stable test also mutates the seed packages 5,000 times, so Windows runs the same code paths.

### Install

1. The app reads the package and checks it with the reader.
2. If the same package (the same SHA-256) is installed, the install stops.
3. The host checks the component without running it: the imports match the permissions, the host has each imported function, and the manifest worlds are exported.
4. The package unpacks into `mods/.staging/`.
5. The user sees the permission screen. For an update, it names the old version and marks new permissions.
6. Install moves the folder to `mods/<id>/` and writes `mods/index.json` (SHA-256, version, permissions). An update moves the old folder away first and deletes it after.

### Sources and state

The app loads mods from three sources, in this order: built-in mods, developer folders (only in developer mode), and installed mods. A mod ID loads from the first source that has it. So a developer folder replaces an installed mod with the same ID while developer mode is on. A developer folder with the ID of a built-in mod does not load.

`config/mods.json` holds the mods that the user turned off, developer mode, and the developer folders. A mod that is off does not load. A mod that the host stopped after 3 failures shows "Stopped after errors" and can be turned on again. Built-in mods can be turned off but not removed.

### Developer mode

A developer folder is the author's mod folder. The app packs it with the same rules as an install and loads the result from a cache folder. So a folder from `t-rustable mods new` works right after `cargo build --release`, and an author sees package errors before they share a package.

A thread checks the size and change time of the folder files once each second: the manifest, the component or the cargo output, and the panel files. When they change and then stay the same for one second, the mod loads again, and its open panels load again. If the new build fails, the old build loads again, and the app shows the error. The app polls because the `notify` crate is CC0, which ADR 0010 does not allow.

Markers: a "Developer mode" label in the status bar, and "(Developer)" on the mod in the palette, on its panel titles, and in the mod manager.

### Command line

`t-rustable mods new` writes a mod from the template, which is built into the program. `t-rustable mods pack` builds the component from the cargo output, applies the package rules and the import check, and writes the `.tmod`. Authors need no `wasm-tools` and no zip tool. Windows PowerShell 5.1 `Compress-Archive` writes backslashes into paths, which the reader refuses, so a hand-made zip often fails.

## Consequences

- A user can install, update, turn off, and remove mods without a network.
- An install compiles the component twice: once for the check and once to load it. A compile cache can come later.
- Developer folders cost one folder scan each second while developer mode is on.
- Signing is still open (a later milestone).
