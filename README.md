# T-Rustable

T-Rustable is a local, offline PDF editor for Linux and Windows. Its main feature is modding: you can make your own tools and add them to the app. The built-in tools are mods too, and they use the same API as your mods.

Status: early development. The app does not work yet. See `KICKOFF.md` for the milestones.

## License

T-Rustable is source-available and free for non-commercial use.

| Part | License |
|---|---|
| App code (core, engine, plugin host, GUI, CLI, built-in mods) | PolyForm Noncommercial 1.0.0 (`LICENSE`) |
| Mod API (`wit/`), `t-rustable-sdk`, mod template, example mods | MIT |
| Documentation (`docs/`) | CC BY-NC 4.0 |
| Name and logos | Not licensed. See `TRADEMARKS.md`. |

`REUSE.toml` gives the license of each file. The license texts are in `LICENSES/`.

Mods that you write against the MIT parts can use any license that you choose.

## More information

- `docs/ARCHITECTURE.md`: how the app works.
- `docs/adr/`: the design decisions.
- `CONTRIBUTING.md`: how to contribute.
- `SECURITY.md`: the threat model and how to report a security problem.
