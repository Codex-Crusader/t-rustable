# types

Interface `t-rustable:api/types@0.1.0`. Shared types. No permission.

All types are frozen within 0.1.x: no new fields and no new cases (ADR 0012).

## IDs

| Type | Meaning |
|---|---|
| `page-id { hi: u64, lo: u64 }` | Stable ID of a page. It does not change when pages move. |
| `object-id { hi: u64, lo: u64 }` | Stable ID of a page object, an annotation, or a form field. |
| `transaction-id` (`u64`) | ID of a committed transaction. |

In Rust, the SDK gives both ID types `Display`, `FromStr` (32 lowercase hex digits), `Eq`, and `Hash`. Use the text form in panel messages and in action arguments.

## Geometry and style

| Type | Fields | Notes |
|---|---|---|
| `point` | `x`, `y` | PDF points |
| `rect` | `left`, `bottom`, `right`, `top` | PDF points. The SDK adds `from_corners`, `width`, `height`, `contains`. |
| `color` | `r`, `g`, `b`, `a` | sRGB, 0 to 255. `a` 255 is opaque. |
| `stroke` | `color`, `width`, `dash` | `dash` lists on and off lengths in points. Empty is a solid line. |
| `text-style` | `font-family`, `size`, `color`, `bold`, `italic` | `size` in points |
| `log-level` | `debug`, `info`, `warn`, `error` | |

## api-error

| Field | Meaning |
|---|---|
| `code` | Stable code from `docs/errors.md`. Handle an unknown code as a general failure. |
| `message` | Short text for the user. |
| `hint` | Optional. What the user can do next. |
| `details` | Optional. Data for the log. |
| `command-index` | Set when a transaction commit fails: the index of the failing command. |

The SDK function `api_error(code, message)` makes one.

## Overlay shapes

An overlay is a list of `overlay-shape` values. The host draws them over the pages. Each shape names its page and uses page coordinates, so the host can zoom and scroll it. Each shape has a required `label` for screen readers.

| Case | Record | Use |
|---|---|---|
| `rect` | `overlay-rect` (page, bounds, stroke, fill, label) | Selection boxes, redaction marks, form field highlights |
| `ellipse` | `overlay-rect` | Ellipse inside `bounds` |
| `line` | `overlay-line` (page, start, end, stroke, start-head, end-head, label) | Lines and arrows (`arrowhead`: none, open, closed) |
| `polyline` | `overlay-polyline` (page, points, closed, stroke, fill, label) | Freehand ink, polygons, rotated boxes |
| `text` | `overlay-text` (page, at, text, style, label) | Text preview |
| `image` | `overlay-image` (page, bounds, image, opacity, label) | Image preview. `image` comes from `ui.register-overlay-image`. |
| `caret` | `overlay-caret` (page, at, height, color, label) | Text cursor |
| `handle` | `overlay-handle` (page, at, kind, label) | Selection grip (`handle-kind`: corner, edge, rotate, move). The host draws it at a fixed screen size. |

A new shape kind will come as a new `ui` function, not as a new case.
