# Stamp (example mod)

A tool with a panel. Select the Stamp tool and click on a page: the stamp text in a frame goes where you click. A preview follows the pointer. The Stamp panel sets the text and the color, and the mod keeps them in its settings.

The example shows:

- a tool action (`"tool": true` in `manifest.json`) with pointer events and an overlay preview,
- a panel that talks to the mod with JSON string messages,
- panel styles from the `--tr-*` theme variables,
- settings through the `host` interface,
- new content that reads upright on a rotated page.

Build and pack it from the repository root with `cargo xtask build-mods`. The package is `target/examples/example.stamp.tmod`.

License: MIT.
