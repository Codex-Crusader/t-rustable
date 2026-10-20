# T-Rustable UI

The web part of the GUI: Svelte 5, TypeScript, and Vite. Tauri embeds the built files from `dist/` into the app (`crates/t-rustable-app`).

## Commands

| Command | Result |
|---|---|
| `npm ci` | Installs the exact package versions from `package-lock.json`. |
| `npm run build` | Builds `dist/`. Run it before `cargo build`. |
| `npm run watch` | Builds `dist/` again after each change. |
| `npm run check` | Type checks with `svelte-check`. Warnings fail. |
| `npm test` | Runs the Vitest tests. |

## Rules

- All UI text is in `src/lib/strings.ts`.
- Colors and sizes are `--tr-*` CSS variables in `src/theme.css`, with light, dark, and high-contrast values.
- Positions from the Rust side are display points at zoom 1. The UI multiplies them by its CSS scale. Do not convert to PDF coordinates in TypeScript: the Rust side does all conversion (ADR 0018).
- The page has no network access (`connect-src 'none'`). Do not load anything from outside `dist/`.
- Each new dependency needs a license from the allowed list in `CONTRIBUTING.md`. Pin its exact version.

## Files

| File | Content |
|---|---|
| `src/App.svelte` | The shell: menus, toolbar, tabs, side bars, dialogs, and the key handler. |
| `src/lib/store.svelte.ts` | The app state. Components change it only through its methods. |
| `src/lib/api.ts` | Typed calls to the Rust commands, and the events. |
| `src/lib/tiles.ts` | Tile math. |
| `src/lib/keymap.ts` | Shortcuts and `keymap.json`. |
| `src/lib/selftest.ts` | The scenario of `--self-test`. |
| `src/components/` | The viewer, pages, overlays, side bars, panels, and the palette. |

License: PolyForm-Noncommercial-1.0.0.
