# Contributing to T-Rustable

Thank you for your interest. Read this file before you open a pull request.

## Before you start

1. Read `docs/ARCHITECTURE.md` and the decision records in `docs/adr/`. Each ADR is a locked decision.
2. For a large change, open an issue first. A change to a locked decision needs a new ADR in `docs/adr/` and approval from the owner.
3. Read the project rules below.

## Project rules

- Write all docs, comments, and commit messages in ASD-STE100 Simplified Technical English. Do not use em dashes. Keep code comments short.
- Keep solutions simple. Solve the current milestone only.
- The app is fully offline. Do not add an HTTP client, a network plugin, telemetry, or an auto-updater (ADR 0011).
- Dependencies must have one of these licenses: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0 (ADR 0010). Never AGPL or GPL. Fonts must be OFL.
- MIT parts (`wit/`, `t-rustable-sdk`, `mods/template/`, `mods/examples/`, `docs/mod-api/`) must never depend on PolyForm parts.
- Commands and the mod API use stable IDs, never indices (ADR 0005).
- Built-in tools are mods. If a built-in tool needs something that the mod API does not give, fix the API (ADR 0006).
- Call the project "source-available, free for non-commercial use". Never "open source".

## Sign the CLA

You must sign the Contributor License Agreement in `CLA.md` before the owner can merge your first pull request.

The CLA is a license grant, not a copyright assignment. You keep your copyright. You give the owner the right to license your contribution under any terms. The owner promises that your contribution also stays available under the project license that applies on the day that you submit it.

To sign:

1. Open your pull request. A bot posts a comment if you did not sign yet.
2. Read `CLA.md`.
3. Post a comment in the pull request with exactly this text:

   ```
   I have read the T-Rustable CLA version 1.0 in CLA.md and I agree to it.
   ```

You sign one time. The signature applies to all of your later pull requests. The CLA check gets the status "success" when all commit authors of the pull request signed.

Each commit must use an email that is linked to your GitHub account. If it is not, the check cannot find you.

The workflow stores your GitHub user name, your user ID, and the date in public, in the branch `cla-signatures`.

## Work that you do not own

Submit only work that you wrote yourself. If a contribution contains work by another person or organization, do these steps:

1. Mark that part clearly in the pull request.
2. Give its source, its copyright holder, and its license.
3. Make sure that the license is on the allowed list in "Project rules".

Do not submit work under AGPL or GPL. Do not submit work that you got under a confidentiality agreement.

If your employer can own the work that you do, get written approval from your employer before you sign the CLA.

## Pull request checklist

Run these commands before you open a pull request. All of them must pass.

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
reuse lint
```

Also:

- Use small commits with conventional commit messages, for example `feat(core): add RotatePage command`.
- Add unit tests for each command handler.
- If you add a new file, make sure that `REUSE.toml` gives it the correct license.
- Do not add a dependency with a license that is not on the allowed list.
- Do not add an HTTP client or any network access. The app is fully offline.
