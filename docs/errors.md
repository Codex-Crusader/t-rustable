# Errors

Status: format only. The error codes come with the features that use them.

## Format

Each error that the user or a mod can see has these fields:

| Field | Meaning |
|---|---|
| `code` | A stable code in `kebab-case`, for example `page-not-found`. The code never changes after a release. |
| `message` | A short text in Simplified Technical English. It tells what went wrong. |
| `hint` | Optional. It tells the user what to do next. |
| `details` | Optional. Technical data for the log, for example a `PageId`. |

Rules for messages:

- Tell what went wrong, then what to do. Example: "The page does not exist. Reload the document and try again."
- Do not blame the user.
- Do not show a stack trace in the UI. Write it to the log.

## Codes

| Code | Source | Meaning |
|---|---|---|
| | | No codes yet. |
