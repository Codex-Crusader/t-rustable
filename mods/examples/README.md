# mods/examples

Example mods for mod authors. Each one is small and has comments, so you can copy it.

| Folder | Mod ID | Shows |
|---|---|---|
| `stamp/` | `example.stamp` | A tool with a pointer preview, a panel with JSON messages, theme variables, and settings. |
| `page-numbers/` | `example.page-numbers` | A batch job with arguments, `measure-text`, and one transaction for all pages. |

`cargo xtask build-mods` builds them and writes `target/examples/<mod id>.tmod`. Install a package in the app with Mods, Install mod. The app does not load the examples by itself.

License: MIT.
