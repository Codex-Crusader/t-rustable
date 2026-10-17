# Versioning policy

The API is the WIT package `t-rustable:api@<version>`. The goal: a mod that works today keeps working when T-Rustable adds features. The reasons are in ADR 0012.

## Compatible and incompatible versions

| Change | Version |
|---|---|
| New function, new resource method, new resource, new interface | Compatible |
| New field in a record, new case in a variant, enum, or flags type | Incompatible |
| Changed or removed function, changed type | Incompatible |

While the API is `0.x`, a compatible release changes the last number (`0.1.0` to `0.1.1`), and an incompatible release changes the middle number (`0.1` to `0.2`). From `1.0.0`, compatible releases change the middle number and incompatible releases change the first number. This matches how wasmtime links versions.

## Promises

- A mod built for `0.1.x` loads in every later `0.1.y` host.
- T-Rustable keeps support for old incompatible versions (mod system rule 9). When `0.2` or `1.0` comes, `0.1` mods keep working.
- New core commands come as new `transaction` methods, never as new cases of a type.
- New overlay shape kinds come as new `ui` functions.
- New exports go in new optional interfaces. The host checks whether a mod exports them. Old mods do not need them.
- Error codes are strings. New codes can come in any release. Handle an unknown code as a general failure.

## For mod authors

- Set `apiVersion` to the first two numbers, for example `"0.1"`.
- Build with the newest SDK for that version. It can use functions that older hosts do not have: such a mod needs that host version. The `.tmod` install (Milestone 5) refuses a mod that imports a function the host does not have.
