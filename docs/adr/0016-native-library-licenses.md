# 0016. Licenses of native libraries inside PDFium and qpdf

Status: Accepted (2026-10-17). Amends ADR 0010. The owner approved it with the Milestone 3 plan.

## Context

ADR 0010 lists the allowed dependency licenses. That list was written for Rust crates and npm packages. PDFium (ADR 0001) is a locked decision, and the pinned build contains C and C++ libraries under other permissive licenses. The license files are in the PDFium download, in `licenses/`. The qpdf build (ADR 0014) also contains IJG libjpeg.

| Library | Inside | License | Duty |
|---|---|---|---|
| FreeType | PDFium | FTL. FreeType is also under GPL-2.0, and we use the FTL. | Give credit in the documentation. |
| libjpeg-turbo | PDFium | IJG and BSD-3-Clause | Say in the documentation that the software is based in part on the work of the Independent JPEG Group. |
| IJG libjpeg 9f | qpdf | IJG | Same as above. |
| libpng | PDFium | libpng-2.0 | Keep the notice. |
| Anti-Grain Geometry 2.3 | PDFium | AGG notice (permissive) | Keep the notice. |
| HarfBuzz | PDFium | MIT-Modern-Variant ("Old MIT") | Keep the notice. |
| dragonbox | PDFium | Apache-2.0 WITH LLVM-exception, or BSL-1.0. We use Apache. | None more. |

The other libraries in the build use licenses that ADR 0010 already allows: BSD-3-Clause (PDFium), Apache-2.0 (abseil, qpdf), MIT (fast_float, simdutf, lcms), BSD-2-Clause (OpenJPEG), Unicode-3.0 (ICU), Zlib (zlib), and Apache-2.0 WITH LLVM-exception (LLVM libc).

## Decision

1. Native libraries inside PDFium and qpdf can use the licenses in the table above. All are permissive. None is copyleft.
2. Rust crates and npm packages keep the ADR 0010 list. This amendment does not change `deny.toml` or the npm check.
3. `THIRD_PARTY_LICENSES` includes the full notice of each native library from a checked-in list.
4. The README and the later About dialog contain these two lines:
   - "Portions of this software are copyright The FreeType Project (www.freetype.org). All rights reserved."
   - "This software is based in part on the work of the Independent JPEG Group."
5. A new PDFium or qpdf version needs a check of its license files. A new license that is not in this ADR or ADR 0010 needs approval from the owner.

## Consequences

- The app can ship the pinned PDFium build and qpdf with no conflict with ADR 0010.
- Each PDFium update adds one manual step: compare the `licenses/` folder with the checked-in list.
