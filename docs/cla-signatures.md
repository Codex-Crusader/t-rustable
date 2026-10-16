# CLA signature collection

Status: installed in `.github/workflows/cla.yml`. The logic is in `.github/scripts/cla.js`. CI runs its tests (`.github/scripts/cla.test.js`).

## Why our own workflow

Candidates that we checked on 2026-10-16:

| Candidate | License | Status | Stores signatures in the repo |
|---|---|---|---|
| `contributor-assistant/github-action` (CLA Assistant Lite) | Apache-2.0 | Archived. Last release v2.6.1 on 2024-09-26. | Yes |
| `cla-assistant/cla-assistant` | Apache-2.0 | Maintained. | No. It is a hosted service with its own database. |
| Forks of CLA Assistant Lite | Apache-2.0 | Active, but each has 5 stars or fewer. | Yes |

The hosted service stores signatures outside the repository. The archived action gets no security fixes. Thus we use a small workflow of our own. It uses only `actions/checkout` and `actions/github-script` (MIT, maintained by GitHub), both pinned by commit SHA.

## How it works

1. A pull request opens or changes, or someone comments on a pull request.
2. The script gets the commit authors of the pull request. The owner and bots do not need to sign.
3. The script reads `signatures/cla.json` from the branch `cla-signatures`.
4. If a commit author posts the exact sign phrase, the script adds their user ID, user name, CLA version, pull request number, comment ID, and date. The first signature makes `cla-signatures` as an orphan branch that holds only this file.
5. The script sets the commit status `cla` to success or failure, and posts or updates one bot comment.

Security rules:

- The workflow checks out only the default branch. It never checks out or runs code from the pull request.
- The job has only `contents: write`, `pull-requests: write`, and `statuses: write`.
- One run at a time (`concurrency: cla`), so two signatures cannot overwrite each other.

## Setup for the owner (after the first push)

1. In the branch protection rule for `main`, make the status check `cla` required.
2. After the first signature makes the branch `cla-signatures`, protect it. Do not permit force pushes or deletion.
3. If the CLA text changes, change `CLA_VERSION` and `SIGN_PHRASE` in `.github/scripts/cla.js`. Old signatures stay valid for the contributions that were made under them.
