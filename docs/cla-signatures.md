# CLA signature collection: proposal

Status: proposal. Nothing is installed. The owner decides.

## Requirement

A GitHub Action collects CLA signatures on pull requests and stores the signatures in this repository.

## Candidates (checked 2026-10-16)

| Candidate | License | Status | Stores signatures in the repo |
|---|---|---|---|
| `contributor-assistant/github-action` (CLA Assistant Lite) | Apache-2.0 | Archived. Last release v2.6.1 on 2024-09-26. | Yes |
| `cla-assistant/cla-assistant` | Apache-2.0 | Maintained (last push 2026-10-13). | No. It is a hosted service with its own database. |
| Forks of CLA Assistant Lite (for example `iainmcgin/cla-github-action`, `SiliconLabsSoftware/action-cla-assistant`) | Apache-2.0 | Active, but each has 5 stars or fewer and a small user base. | Yes |

The hosted service fails the requirement. The archived action gets no security fixes. No fork has a large user base.

## Recommendation

Write a small workflow in this repository. It uses only `actions/github-script` (MIT, maintained by GitHub, v9.0.0 on 2026-04-09).

How it works:

1. Trigger on `pull_request_target` (opened, synchronize) and on `issue_comment` (created).
2. Get the list of commit authors of the pull request through the GitHub API.
3. Read `signatures/cla.json` from a separate branch named `cla-signatures`.
4. If a contributor writes the exact comment "I have read the CLA Document and I hereby sign the CLA", add their GitHub user ID, login, the pull request number, the comment ID, the CLA version, and the date to the file. Commit the change to `cla-signatures`.
5. Set a commit status `cla` to success when all authors signed, or failure when one or more did not sign.

Security rules for the workflow:

- Never check out or run code from the pull request. `pull_request_target` runs with write access.
- Give the job only `contents: write`, `pull-requests: write`, and `statuses: write`.
- Pin `actions/github-script` by commit SHA.
- Protect the `cla-signatures` branch so that only the workflow and the owner can push.

The workflow is about 100 lines of JavaScript. The owner controls all of it.

## Alternative

Use the archived CLA Assistant Lite, pinned to commit `ca4a40a7d1004f18d9960b404b97e5f30a505a08` (tag v2.6.1). It works and it is widely used, but it gets no fixes.
