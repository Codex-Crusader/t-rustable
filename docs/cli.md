# Command line

The `t-rustable` command works on PDF files without the app. It uses the same core and the same mods as the app. It is fully offline. ADR 0020 has the design decisions.

```
t-rustable info report.pdf
t-rustable merge a.pdf b.pdf:2-4 -o both.pdf
t-rustable export report.pdf --format png --dpi 150 --out-dir pages
t-rustable run t-rustable.page-tools:rotate *.pdf --args '{"degrees": 90}' --out-dir rotated
t-rustable recipe clean-up.json *.pdf --out-dir done --jobs 4
```

## Rules for all commands

- A command never changes an input file unless you add `--in-place`. If an output is an input file, the command stops before it reads any file.
- A command that saves needs exactly one of these options:
  - `-o <file>`: the output file. Only for one input file.
  - `--out-dir <folder>`: each output gets the name of its input. The command makes the folder if it does not exist.
  - `--in-place`: the changes go into the input files.
  - `--dry-run`: the command does all steps, but it writes no file.
- If two inputs would write the same output, for example `a/doc.pdf` and `b/doc.pdf` with `--out-dir`, the command stops.
- If a file fails, the command writes no output for that file. It continues with the other files.
- `--json` prints one JSON report on stdout. Without it, each file gets one line, and errors go to stderr.
- `--verbose` prints the log messages and the progress of the mods on stderr.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | All files are done. |
| 1 | At least one file failed. The other files are done. |
| 2 | The command did not start, for example because of a bad argument, a recipe that does not parse, or a mod that cannot run. No file changed. |

### JSON report

```json
{
  "ok": false,
  "dryRun": false,
  "files": [
    { "input": "a.pdf", "ok": true, "outputs": ["out/a.pdf"], "result": { "pages": 3 } },
    { "input": "b.pdf", "ok": false, "error": { "code": "damaged-file", "message": "..." } }
  ]
}
```

- `result` is what the command found: the document facts of `info`, or the reply of the mod. For a recipe, it is a list with one reply for each step.
- `notes` lists the messages of the mods, and the questions that the command line could not ask.
- If the command does not start, the report has `error` and an empty `files` list.
- The error codes are in [errors.md](errors.md).

### Encrypted files

The command line opens a file that needs a password only with the `T_RUSTABLE_PASSWORD` environment variable. A password on the command line would show in the list of processes. A save keeps the encryption of the file.

## info

```
t-rustable info <file>...
```

Shows the number of pages, the page sizes in points, page labels, encryption, digital signatures, form fields, outline entries, the document information (title, author, and so on), and the file size. A page size is the size that a reader sees: the crop box, turned with the page rotation.

## merge

```
t-rustable merge <file>[:<pages>]... -o <new file>
```

Puts the pages of the inputs, in order, into one new file. A page range follows the file name after a colon: `1-3,5,8-` means pages 1, 2, 3, 5, and 8 to the end. Page numbers start at 1. A range can change the order, for example `3,1`.

- The first input is the base. Its outline, form, and document information stay. A range on the first input can use each page only once. To use a page of the first file again, add the file again as a later input.
- The pages of the other inputs are copies. Their annotations stay. Form fields of copied pages are not added to the form of the base.
- Merge does not copy pages from an encrypted file, also not from a file that only has restrictions. The copy would not keep the restrictions.

## export

```
t-rustable export <file>... --format png|txt --out-dir <folder> [--dpi 150] [--pages 1-3]
```

- `png` writes `<name>-<page>.png` for each page. `--dpi` is 36 to 600. A page that is too large to render at that resolution fails the file.
- `txt` writes `<name>.txt` with the text of the pages. A form feed character ends each page.

## run

```
t-rustable run <mod-id>[:<action>] <file>... [--args <json> | --args-file <file>] <output option>
```

Runs the batch job of a mod on each file, then saves the file. The command line puts the action into the JSON arguments as `"action"`. These two commands are the same:

```
t-rustable run t-rustable.page-tools:rotate a.pdf --args '{"degrees": 90}' -o b.pdf
t-rustable run t-rustable.page-tools a.pdf --args '{"action": "rotate", "degrees": 90}' -o b.pdf
```

The batch job of `t-rustable.page-tools` has these actions. Page numbers start at 1.

| Action | Arguments |
|---|---|
| `rotate` | `degrees` (a multiple of 90), and `pages`. Without `pages`, it rotates all pages. |
| `delete` | `pages` |
| `move-up`, `move-down` | `pages` |
| `insert-blank-after` | `page` |

The command uses built-in mods and installed mods. It does not use developer folders. These mods stop the command with exit code 2:

- A mod that is not there. Run `t-rustable mods list`.
- A mod that you turned off in the app.
- A mod without a batch job. It works only in the app.

A mod that also has a tool for the app can ask a question or ask for a file. The command line cannot ask, so the answer is no and no file. The result of the file tells you about each question. In the command line, the mod settings start empty and are not kept, so the settings of the app do not change.

## recipe

```
t-rustable recipe <recipe.json> <file>... <output option>
```

A recipe is a list of mod steps:

```json
{
  "recipe": 1,
  "steps": [
    { "run": "t-rustable.page-tools:rotate", "args": { "degrees": 90 } },
    { "run": "example.page-numbers" }
  ]
}
```

All steps run on the same file, and the file saves once at the end. If a step fails, the file fails, and nothing is written for it. `recipe` must be 1.

## Worker processes

`--jobs <N>` (1 to 64) runs the files of `info`, `export`, `run`, and `recipe` on N worker processes. Each worker has its own engine and loads the mods again, so `--jobs` helps only with many files. The results print in the order of the inputs.

## mods

| Command | What it does |
|---|---|
| `mods list` | Lists the built-in and the installed mods: version, source, on or off, and whether the mod has a batch job. |
| `mods install <file.tmod> [--yes]` | Checks the package like the app, shows the permissions and the SHA-256, and installs it. Without a terminal, it needs `--yes`. |
| `mods remove <id>` | Removes an installed mod. Built-in mods cannot be removed. |
| `mods verify <file.tmod or id>` | Does all install checks on a package, or on a built-in or installed mod. It installs nothing. |
| `mods new <name>` | Writes a new mod folder from the template. See [the first mod guide](mod-api/first-mod-in-15-minutes.md). |
| `mods pack <folder>` | Checks a built mod folder and writes its `.tmod` package. See [packaging](mod-api/packaging.md). |

`mods install` and `mods remove` change only the mods folder. They never change the mod settings of the app, so a running app does not lose a change. A running app sees the change after a restart. Turn mods on and off in the app.

## Folders

The command line uses the app folders (ARCHITECTURE section 12). `T_RUSTABLE_HOME` puts all of them in one folder, for tests. The journals of the command line go to a temporary folder that the command deletes at the end, so the app does not offer to recover them.
