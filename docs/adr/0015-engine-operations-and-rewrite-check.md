# 0015. Engine operations as data, and a check after each page rewrite

Status: Accepted (2026-10-17).

## Context

The core owns commands and the `IdMap`. The engine owns PDFium. The engine cannot depend on core types, because the core depends on the engine. A later milestone can move the engine to a child process, and then each request must cross a pipe as a message.

The M3 spike (`docs/spikes/m3-report.md`, spike 2) found two facts:

1. When PDFium can write a page again, the object order and kinds stay the same after close and reopen, and after save and reload. Thus the core can map an `ObjectId` to an object index.
2. PDFium cannot always write a page again. With Type3 fonts it writes broken text, and on one other page one text object lost its character. Removing an object makes PDFium write the page again. Adding an object does not.

## Decision

1. The engine defines `EngineOp`, a serializable enum of PDF operations by page index and object index: for example rotate a page, insert a page, add a text object, remove an object, render, extract text. The core changes each core command into engine ops. Requests and replies are plain data.
2. After ops that change page content, the engine calls `FPDFPage_GenerateContent`, closes the page, and opens it again. Consecutive content ops on the same page in one request share one check. It then compares two things with the state before the close: the object kinds in order, and the text of each text object (`FPDFTextObj_GetText`). It does not compare bounds, because text bounds move a little when PDFium measures again.
3. The reopened page is the only state from then on. Later ops use the new handles. Thus a live session, a replay, and a checkpoint reload all hold the same objects, and the `IdMap` stays correct.
4. On a difference, the op fails with `unsupported-content`. The core rejects the transaction. The live document already contains the bad content, so the core rolls back from bytes: the nearest checkpoint and a replay, or the original and a replay. It never rolls back by keeping old handles.
5. Replay and crash recovery run the same check. A journal from an older PDFium version can give a different result. Then replay stops with a clear error and does not build a damaged document.

## Consequences

- The engine protocol can go over a pipe later with no redesign.
- `RemoveObject` fails on pages that PDFium cannot write again, for example pages with Type3 fonts. That was 10 of 144 test pages. The user gets an error, and no content is lost.
- `ReplaceText` and precise redaction also write pages again. Those milestones must handle the same pages, for example with the rasterize mode of redaction.
- Each check costs one content write, one page reopen, and two text extractions. Measured in a release build on Windows, for one add in its own request: 0.8 ms on a page with 100 objects, 6 ms with 1,000, 18 ms with 3,000, and 61 ms with 10,000.
- Each check writes a new content stream. After 1,000 separate commits on one page, one more add cost 25 ms instead of 6 ms. A later milestone can join the content streams of a page, for example at checkpoints.
- One request with 2,000 adds on one page runs one check. Without the shared check, 3,000 adds took 134 s.
