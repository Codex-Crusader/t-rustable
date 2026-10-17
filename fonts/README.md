# fonts

Bundled fonts. Only fonts under the SIL Open Font License (OFL) go here, each with its license file.

When you add a font, add a `REUSE.toml` entry for the font files and its license file.

| File | Font | SHA-256 |
|---|---|---|
| `NotoSans-Regular.ttf` | Noto Sans, regular | `f3961a9cde016d41a4879aecda1474d3a36d6bf54fa0e4643de029cc2248b0e8` |
| `NotoSans-Bold.ttf` | Noto Sans, bold | `87cb2d84472a7d66da659ee47b6cdb9552326e8c128245231f191b6ac72529d9` |
| `NotoSans-Italic.ttf` | Noto Sans, italic | `678288f868807d4d64a6f3b51466871d117d915780381ce9d0ed4b3bcbd06d37` |
| `NotoSans-BoldItalic.ttf` | Noto Sans, bold italic | `3d367743f371f28671d2764e911a53d7c20ec9b6aa8791d059e7090389fc52a5` |

Source: the unhinted TrueType files from `notofonts/notofonts.github.io`, `fonts/NotoSans/unhinted/ttf/`. License: `OFL.txt` (SIL Open Font License 1.1). Noto Sans covers Latin, Greek, and Cyrillic.

`AddText` refers to a font by its SHA-256, and the journal stores the font file. Thus an old project replays with the same font after an app update.
