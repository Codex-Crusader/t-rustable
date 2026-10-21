# Page numbers (example mod)

A batch job without UI. It adds a page number at the bottom of each page, as the reader sees the page, also on a rotated page.

Arguments (JSON, all optional):

| Field | Default | Meaning |
|---|---|---|
| `format` | `"Page {n} of {total}"` | The text. `{n}` is the page number and `{total}` the page count. |
| `position` | `"center"` | `"left"`, `"center"`, or `"right"`. |
| `size` | `10` | Font size in points, 4 to 72. |
| `margin` | `24` | Distance from the bottom edge to the baseline, in points. |

The job returns `{"pages": <count>}`.

The example shows the `batch` world, argument parsing with `parse_args`, `measure-text`, and one transaction for all pages, so one undo removes all numbers.

Build and pack it from the repository root with `cargo xtask build-mods`. The package is `target/examples/example.page-numbers.tmod`.

License: MIT.
