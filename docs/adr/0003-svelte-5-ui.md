# 0003. UI: Svelte 5 and TypeScript

Status: Accepted (2026-10-16)

## Context

The UI has many small parts that change often: tiles, overlays, panels, and thumbnails. It must be fast and keep the bundle small. All code is bundled, because the app is offline.

## Decision

Use Svelte 5 with TypeScript in `ui/`. Use Svelte runes for state.

## Consequences

- The compiler removes most framework code from the bundle.
- The UI dependencies go through the npm license check in CI.
- Mod panels do not use Svelte. They are plain HTML in iframes, so mod authors can use any tool.
