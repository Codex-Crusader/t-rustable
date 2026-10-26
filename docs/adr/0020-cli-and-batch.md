# 0020. Command line, batch runs, and merge

Status: Accepted (2026-10-26)

## Context

Milestone 6 adds the document commands to `t-rustable`: `info`, `merge`, `export`, `run`, and `recipe`, plus `mods list|install|remove|verify`. The command line uses the same core as the app. Users run it on many files and in scripts, so its exit codes, its output, and its data safety must be stable. No core command copied pages from another PDF, so `merge` needs one.

## Decision

### Merge is a core command

`Command::InsertPdfPages { ids, before, source, pages }` copies pages of another PDF. `source` is the PDF file as an asset (ADR 0008), so undo, replay, and recovery do not need the source file. The engine op `ImportPages` uses `FPDF_ImportPagesByIndex`. PDFium copies all objects that the pages use, so the source closes at once. A spike test showed that the copied pages render and save after the source bytes are gone.

The core reads the new pages from the engine after the commit, as it does for the pages of an opened file. A command on a new page in the same transaction gets `invalid-argument` and a hint to commit first.

The engine refuses an encrypted source, also a source that only an owner password protects. Such a file opens without a password, but a copy of its pages would not keep its restrictions. That would be a restriction bypass (CLAUDE.md data safety rules). `DocSummary.encrypted` tells the caller before it tries.

The first input of `merge` is the base. Its outline, form, and metadata stay. A page range on the base deletes the other pages and orders the rest. The pages of the other inputs are copies. Their annotations stay. The widgets of form fields stay on the copied pages, but the copy adds no field to the form of the base document. A test shows this, and other PDF readers can show such a widget as a plain annotation.

The mod API does not get this command in Milestone 6. It can come as `transaction.insert-pdf-pages` in API 0.1.2 when a GUI tool needs it.

### Outputs and exit codes

- A command that saves needs exactly one of `-o <file>` (one input only), `--out-dir <dir>`, `--in-place`, or `--dry-run`. A command never writes to an input file without `--in-place`. It compares the output and the inputs as files, so a different spelling of the same path is caught. Two inputs that would write the same output stop the command.
- Saves use the core save pipeline. Exported files go to a temporary file first, then a rename.
- Exit code 0: all files are done. 1: at least one file failed, and the other files are done. 2: the command cannot start, for example bad arguments, a recipe that does not parse, or a mod that cannot run. A failed file writes no output.
- `--json` prints one report on stdout: `ok`, `dryRun`, and for each file `input`, `ok`, `outputs`, `error`, `result`, and `notes`. The text output goes to stdout, and the errors go to stderr.
- A password comes only from `T_RUSTABLE_PASSWORD`. A password on the command line would show in the process list.

### Journals

Each run (and each worker) keeps its journals in a new temporary folder and deletes it at the end. The journals of the command line never go to the app's sessions folder, so the app does not offer to recover them.

### Mods in batch runs

- `run <mod-id>[:<action>]` calls the `batch-job` export. The export has no action parameter, so the command line puts `"action": "<action>"` into the JSON arguments. That is the convention of the built-in batch jobs. If the arguments name another action, the command stops.
- Only the mods that the command names load: a built-in mod (`t-rustable.*`) or an installed mod. Developer folders do not load. A mod that the user turned off in the app, or a mod without the `batch` world, stops the command with exit code 2.
- A `full` mod can call `ui` and `files` during a batch run. In the command line, registrations and panel messages fail with an error, overlays do nothing, `confirm` returns no, and `files.pick` returns no file. The result of the file tells the user each question that the command line could not ask. `notify` messages go into the result too. `log` and `progress` go to stderr with `--verbose`.
- Mod settings are in memory only. The command line does not change the settings that the app keeps.

### Recipes

A recipe is JSON: `{"recipe": 1, "steps": [{"run": "<mod-id>:<action>", "args": {}}]}`. All steps run on one session per file, and the file saves once at the end. If a step fails, the file fails, and nothing is written for it. The page-tools batch job rotates all pages when `pages` is missing, so one recipe works for files with different page counts. Its other actions still need `pages`, so a missing list cannot delete all pages.

### Worker processes

`--jobs N` starts N copies of the program with the hidden `worker` command. Each worker has its own engine and mod host, because PDFium allows one engine per process. The parent and the workers talk through stdin and stdout pipes, one JSON value per line, never through a socket (CLAUDE.md offline rules). First the parent sends the plan, and each worker answers when its mods are loaded. If a worker cannot start, no file runs, and the exit code is 2. If a worker stops during a file, that file fails, and the parent starts a new worker. Results print in input order.

### Mod install from the command line

`mods install` does the same checks as the app and shows the permissions and the SHA-256. Without a terminal it needs `--yes`. The command line changes only the mods folder and `index.json`. It never writes `config/mods.json`, because a running app keeps that file in memory and would write over the change. A running app sees an install or a remove after a restart.

## Consequences

- Scripts can depend on the exit codes and the JSON report.
- A `.trex` project and a journal can now hold a second PDF as an asset. A merged file can be large in the journal.
- Each worker compiles the mods again. For a few files, `--jobs 1` is faster.
- Encrypted files cannot be merge sources.
- A failed export removes the files that it wrote for that input.
