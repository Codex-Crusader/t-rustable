# 0005. Stable IDs and PDF coordinates

Status: Accepted (2026-10-16)

## Context

Page indices and object indices change when pages move or objects are deleted. A command with an index can point at the wrong object after an earlier change or after undo. Display coordinates change with zoom and rotation.

## Decision

- Each page has a `PageId` and each object has an `ObjectId`. Both are 128-bit IDs.
- Commands and the mod API use only these IDs, never indices.
- The core maps IDs to engine handles in the `IdMap`.
- IDs of the original document come from a seed that the journal stores. A command that creates a page or object carries the new ID. Thus replay gives the same IDs.
- Commands store native PDF coordinates: points, `f64`, PDF user space.
- The mod API gives `to-display` and `from-display`. Pointer events arrive in page coordinates.

## Consequences

- The core must keep the `IdMap` correct after each command, undo, and replay.
- Mods never need to know the zoom or rotation to make a correct command.
- Commands are a little larger than with indices.
