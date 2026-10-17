# edit

Interface `t-rustable:api/edit@0.1.0`. Changes to a document. Permission: `document.edit`.

A mod changes a document only through core commands in a transaction. The journal records the core commands, not the mod, so undo and replay never need the mod (ADR 0004).

## Transactions

1. `begin(document, label)` opens a `transaction`. The label shows in the undo list, for example "Rotate page".
2. Add commands with the methods below. Each method checks its command at once and returns an error if the command is not valid.
3. `transaction.commit(tx)` applies all commands at once. It returns the `transaction-id`. If one command fails, nothing changes, and the error has `command-index`.

To discard a transaction, drop it without `commit`. One transaction is one undo step.

## Commands in 0.1

| Method | Result | Notes |
|---|---|---|
| `delete-page(page)` | | |
| `move-page(page, before)` | | `before` none moves the page to the end. |
| `rotate-page(page, degrees)` | | A multiple of 90. Positive is clockwise. |
| `insert-blank-page(before, width, height)` | `page-id` | Size in points. `before` none inserts at the end. |
| `add-text(page, at, text, style)` | `object-id` | `at` is the start of the baseline. |
| `add-image(page, bounds, data)` | `object-id` | `data` is a PNG or JPEG file. The host stores it by SHA-256. |
| `add-shape(page, shape, stroke, fill)` | `object-id` | `shape`: rect, ellipse, line, polyline |
| `remove-object(object)` | | Page objects and annotations |

A method that makes a page or an object returns its new ID. Later commands in the same transaction can use that ID.

New core commands come as new methods in later 0.1.x versions. See [versioning](versioning.md).

## Example (Rust)

With the SDK `Command` enum:

```rust
use t_rustable_sdk::{Command, run_commands};

run_commands(document, "Rotate pages", vec![
    Command::RotatePage { page: first, degrees: 90 },
    Command::RotatePage { page: second, degrees: 90 },
])?;
```

With the transaction methods:

```rust
use t_rustable_sdk::edit::{self, Transaction};

let tx = edit::begin(document, "Insert page")?;
let page = tx.insert_blank_page(None, 595.0, 842.0)?;
tx.add_text(page, Point::new(72.0, 770.0), "New page", &style)?;
Transaction::commit(tx)?;
```

## Pointer tools

`on-pointer` returns overlays, not a transaction. A tool that changes the document after a gesture, for example on the pointer `up` event, opens and commits its own transaction in that call.
