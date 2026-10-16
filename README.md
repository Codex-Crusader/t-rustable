<p align="center"><img src="branding/logo.png" alt="T-Rustable logo: a T-Rex with black glasses on an orange background" width="200"></p>

# T-Rustable

T-Rustable is a local, offline PDF editor for Linux and Windows. Its main feature is modding: you can make your own tools and add them to the app. The built-in tools are mods too, and they use the same API as your mods.

Status: early development. The app does not work yet.

## Roadmap

1. Repository, licenses, and design documents (done).
2. Risk spikes: rendering speed, mod panel isolation, WASM mod host, PDFium edit APIs.
3. Mod API v0.1.
4. Core, command model, and mod host.
5. GUI.
6. External mods: `.tmod` packages and the mod manager.
7. Command line and batch jobs.

## License

T-Rustable is source-available and free for non-commercial use.

- You can use it free of charge for personal use, study, research, and hobby projects. Schools, charities, public research groups, and government bodies can also use it free of charge.
- You cannot sell it, and you cannot use it to make a paid product.
- A company that wants to use it for its business needs a separate license from the owner.
- Your own mods are yours. You can give them any license, including a paid license.

| Part | License |
|---|---|
| App code (core, engine, plugin host, GUI, CLI, built-in mods) | PolyForm Noncommercial 1.0.0 (`LICENSE`) |
| Mod API (`wit/`), `t-rustable-sdk`, mod template, example mods, mod API docs (`docs/mod-api/`) | MIT |
| Other documentation (`docs/`) | CC BY-NC 4.0 |
| Name and logos (`branding/`) | Not licensed. See `TRADEMARKS.md`. |

`REUSE.toml` gives the license of each file. The license texts are in `LICENSES/`.

Mods that you write against the MIT parts can use any license that you choose.

## More information

- `docs/ARCHITECTURE.md`: how the app works.
- `docs/adr/`: the design decisions.
- `CONTRIBUTING.md`: how to contribute.
- `SECURITY.md`: the threat model and how to report a security problem.
