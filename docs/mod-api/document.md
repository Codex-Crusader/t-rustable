# document

Interface `t-rustable:api/document@0.1.0`. Read access to a document. Permission: `document.read`.

## The `doc` resource

The host lends a `doc` to each call of an exported function (`borrow<doc>`). A mod cannot keep it after the call. To act on a document later, store page and object IDs, and use the `doc` of the next call.

| Method | Result | Notes |
|---|---|---|
| `page-ids()` | `list<page-id>` | In page order |
| `page-info(page)` | `page-info` | ID, display label (for example "iv"), width and height in points, rotation (0, 90, 180, 270) |
| `text-runs(page)` | `list<text-run>` | Text, font name, font size, bounds, and the box of each character |
| `objects(page)` | `list<object-info>` | ID, kind (text, image, path, shading, form-xobject, unknown), bounds |
| `annotations(page)` | `list<annotation-info>` | ID, PDF subtype (for example "Highlight"), bounds, contents |
| `form-fields()` | `list<form-field-info>` | All fields of the document: ID, page, name, kind, value, bounds, read-only |
| `search(query, options)` | `list<search-hit>` | `options`: match-case, whole-word, max-hits. Each hit has a page, the found text, and its boxes. |
| `metadata()` | `metadata` | Title, author, subject, keywords, creator, producer, page count |
| `to-display(page, point)` | `display-point` | CSS pixels from the top left of the page view. Error `no-display` in the batch world. |
| `from-display(page, display-point)` | `point` | The reverse. Error `no-display` in the batch world. |

A page ID or object ID that does not exist gives the error `not-found`.

## Example (Rust)

```rust
use t_rustable_sdk::document::{Doc, SearchOptions};

fn count_hits(document: &Doc, word: &str) -> usize {
    let options = SearchOptions { match_case: false, whole_word: true, max_hits: 1000 };
    document.search(word, options).map(|hits| hits.len()).unwrap_or(0)
}
```
