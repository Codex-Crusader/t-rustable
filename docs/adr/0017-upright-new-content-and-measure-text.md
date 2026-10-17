# 0017. New text and images read upright, and mods can measure text

Status: Accepted (2026-10-17). The owner selected both decisions during Milestone 3, when the add-text mod found two gaps in API 0.1.0.

## Context

The built-in tools are mods, so a gap that a built-in tool finds is a gap in the mod API (mod system rule 1). The add-text mod found two gaps:

1. A mod could not measure text. Thus add-text could not put the caret after the typed text.
2. `AddText` and `AddImage` had no rotation. Commands use PDF user space before the page rotation (ADR 0005). On a page with `/Rotate 90`, new text ran along user space x, so the reader saw it sideways. New images had the same problem.

Options for the rotation:

| Option | Result |
|---|---|
| The core turns new content with the page rotation | New content reads upright. No API change. Each mod gets the correct result. |
| A rotation parameter in new API functions | More control, but each mod must handle rotated pages itself. |
| No change, document the behavior | New text on rotated pages stays sideways. |

Options for measurement: a `measure-text` function, an overlay shape that the host draws with its own caret, or a caret that stays at the click point.

## Decision

1. New text and images read upright for the reader. On a page with a rotation of 90, 180, or 270 degrees, the core turns them counterclockwise in user space by the same number of quarter turns. For text, the baseline turns about `at`. For an image, the image fills `bounds`, and at 90 or 270 degrees its width runs along the height of `bounds`.
2. `AddText` and `AddImage` have an optional field `turns` (0 to 3). If it is empty, the core uses the page rotation. `Session::add` writes the resolved value into the command, so the journal always stores it. Thus a replay does not depend on this rule, and a later change of the rule cannot change old journals.
3. The host draws overlay text and overlay images upright in the same way. An overlay caret goes from `at` up by `height`, as the reader sees the page. The WIT doc comments state these rules.
4. API 0.1.1 adds `document.measure-text(text, style)`. It returns the width in points as the engine lays out `edit.add-text`: the sum of the glyph advances, without kerning. This is an additive change under ADR 0012: a mod built for 0.1.0 still loads.

## Consequences

- Mods give positions in user space and never turn content themselves.
- The M4 GUI must draw overlays by rule 3.
- A test renders text that the core added on a rotated page, and checks that it runs left to right. Another test checks that `measure-text` matches the PDFium layout within 0.05 points.
- A mod that wants text at another angle needs a later API addition, for example a function with an explicit rotation.
- A mod built with the 0.1.1 SDK imports `@0.1.1`, so it needs a 0.1.1 host or later.
