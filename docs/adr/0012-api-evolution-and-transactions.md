# 0012. Mod API evolution and the transaction resource

Status: Accepted (2026-10-17). Changes the edit design in the kickoff plan for Milestone 2.

## Context

The mod API must grow for years without breaking installed mods (mod system rule 9: additive changes only within a major version). The kickoff plan described edits as `submit(doc, label, list<command>)` with one `command` variant case per core command.

A spike on 2026-10-17 tested how wasmtime 49 links a component against a newer host:

| Guest built against | Host provides | Result |
|---|---|---|
| `1.0.0` | `1.1.0` with one new function | Loads. Wasmtime matches `1.0.0` to `1.1.0`. |
| `1.0.0` | `1.1.0` with one new variant case | Refused: "a matching implementation was not found". |
| `0.1.0` | `0.1.1` with one new function | Loads. |
| world with two imported interfaces, one used | | The component imports only the used interface. |
| interface used only for a type | | The component imports that interface. |

Thus a new case in any variant, or a new field in any record, breaks every existing mod. With a `command` variant, each new core command (for example `ReplaceText`, redaction, form commands) would need a new major API version, and the host would need to keep a copy of every old version.

## Decision

1. Edits use a resource. `edit.begin(doc, label)` returns a `transaction`. The transaction has one method per core command. `transaction.commit(tx)` applies all commands at once. A new core command is a new method, which old mods do not see.
2. Types are frozen within a compatible version range. Never add a field to a record, or a case to a variant, enum, or flags type. New features come as new functions, new resource methods, new resources, or new interfaces.
3. New guest exports go in a new optional exported interface. The host checks whether the component exports it.
4. Errors are one record, `api-error`, with a string `code`. New error codes do not change the type.
5. New overlay shape kinds come through new `ui` functions, never as new cases of `overlay-shape`.
6. Exported functions do not use types from `edit`, `ui`, or `files`. If they did, every mod would import those interfaces and need their permissions. Thus `on-pointer` returns overlays only, not a transaction (the kickoff plan said "overlay shapes and an optional transaction"). A mod that edits on a pointer event commits its own transaction.
7. Version numbers: wasmtime treats `0.x` minor versions as incompatible. While the API is `0.x`, additive releases are patch releases (`0.1.0` to `0.1.1`). A breaking change is `0.2.0`. `1.0.0` comes with app version 1.

## Consequences

- Old mods keep working when the core gets new commands.
- Rust mod authors still get a `Command` enum in the SDK. It is Rust only, marked `#[non_exhaustive]`, and it calls the transaction methods.
- Each transaction method checks its command at once, so a mod gets an error at the call that is wrong. `commit` can still fail, and the error then has `command-index`.
- The host must keep one set of bindings per incompatible version (`0.1`, later `0.2` or `1.0`), not one per release.
- Any change to a frozen type needs a new incompatible version. Review new types carefully before a release.
