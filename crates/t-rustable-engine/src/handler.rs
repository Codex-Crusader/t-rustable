//! Runs one request on the engine thread.

use crate::pdfium::{Document, Pdfium};
use crate::protocol::{
    Applied, DocKey, DocSummary, EditOp, EngineError, ObjectKind, Query, Request, Response,
    SearchHit,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct State {
    pub pdfium: Rc<Pdfium>,
    pub docs: HashMap<DocKey, Document>,
}

const MAX_PAGE_SIZE: f64 = 14_400.0;

fn summary(d: &Document) -> DocSummary {
    DocSummary {
        page_count: d.page_count(),
        signature_count: d.signature_count(),
        metadata: d.metadata(),
        encrypted: d.encrypted(),
    }
}

fn doc(state: &mut State, key: DocKey) -> Result<&mut Document, EngineError> {
    state.docs.get_mut(&key).ok_or(EngineError::UnknownDocument)
}

pub(crate) fn handle(
    state: &mut State,
    req: Request,
    cancel: &AtomicBool,
) -> Result<Response, EngineError> {
    match req {
        Request::Open {
            doc: key,
            bytes,
            password,
        } => {
            state.docs.remove(&key);
            let d = Document::open(&state.pdfium, bytes, password.as_deref())?;
            let s = summary(&d);
            state.docs.insert(key, d);
            Ok(Response::Opened(s))
        }
        Request::Close { doc: key } => {
            state.docs.remove(&key);
            Ok(Response::Closed)
        }
        Request::Save { doc: key } => Ok(Response::Saved(doc(state, key)?.save()?)),
        Request::Apply { doc: key, ops } => {
            // An apply runs to the end once it starts. A cancel in the middle would leave a
            // partial transaction.
            let d = doc(state, key)?;
            let mut out = Vec::with_capacity(ops.len());
            let mut i = 0;
            while i < ops.len() {
                let fail = |index: usize| {
                    move |e| EngineError::OpFailed {
                        index,
                        error: Box::new(e),
                    }
                };
                match content_page(&ops[i]) {
                    Some(page) => {
                        let end = (i..ops.len())
                            .find(|&j| content_page(&ops[j]) != Some(page))
                            .unwrap_or(ops.len());
                        out.extend(
                            edit_page(d, page, &ops[i..end]).map_err(|(k, e)| fail(i + k)(e))?,
                        );
                        i = end;
                    }
                    None => {
                        out.push(apply_structure(d, &ops[i]).map_err(fail(i))?);
                        i += 1;
                    }
                }
            }
            Ok(Response::Applied(out))
        }
        Request::Query { doc: key, query } => query_doc(doc(state, key)?, query, cancel),
        Request::Render {
            doc: key,
            page,
            scale,
            clip,
        } => {
            if !(scale > 0.0 && scale <= 64.0) {
                return Err(EngineError::InvalidArgument(format!(
                    "scale {scale} is out of range"
                )));
            }
            Ok(Response::Rendered(
                doc(state, key)?.page(page)?.render(scale, clip, cancel)?,
            ))
        }
        #[cfg(feature = "test-hooks")]
        Request::TestPanic => panic!("test panic on the engine thread"),
        #[cfg(feature = "test-hooks")]
        Request::TestSleep { millis } => {
            std::thread::sleep(std::time::Duration::from_millis(millis));
            Ok(Response::Closed)
        }
    }
}

fn query_doc(d: &mut Document, query: Query, cancel: &AtomicBool) -> Result<Response, EngineError> {
    Ok(match query {
        Query::Summary => Response::Summary(summary(d)),
        Query::PageInfo { page } => Response::PageInfo(d.page(page)?.info()),
        Query::Objects { page } => Response::Objects(d.page(page)?.objects()),
        Query::Annotations { page } => Response::Annotations(d.page(page)?.annotations()),
        Query::TextRuns { page } => Response::TextRuns(d.page(page)?.text_runs()?),
        Query::PageText { page } => Response::PageText(d.page(page)?.text()?),
        Query::FormFields => Response::FormFields(d.form_fields()?),
        Query::Outline => Response::Outline(d.outline()),
        Query::Search { query, options } => {
            if query.is_empty() {
                return Err(EngineError::InvalidArgument(
                    "the search text is empty".into(),
                ));
            }
            let mut hits = Vec::new();
            for page in 0..d.page_count() {
                if hits.len() >= options.max_hits {
                    break;
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err(EngineError::Cancelled);
                }
                let left = options.max_hits - hits.len();
                for (text, boxes) in
                    d.page(page)?
                        .search(&query, options.match_case, options.whole_word, left)?
                {
                    hits.push(SearchHit { page, text, boxes });
                }
            }
            Response::SearchHits(hits)
        }
    })
}

fn positive(name: &str, v: f64, max: f64) -> Result<(), EngineError> {
    if v.is_finite() && v > 0.0 && v <= max {
        Ok(())
    } else {
        Err(EngineError::InvalidArgument(format!(
            "{name} {v} is out of range"
        )))
    }
}

/// The page of an op that changes page content, else None.
fn content_page(op: &EditOp) -> Option<usize> {
    match op {
        EditOp::AddText { page, .. }
        | EditOp::AddImage { page, .. }
        | EditOp::AddPath { page, .. }
        | EditOp::RemoveObject { page, .. } => Some(*page),
        _ => None,
    }
}

fn turns(quarter_turns: u8) -> Result<(), EngineError> {
    if quarter_turns < 4 {
        Ok(())
    } else {
        Err(EngineError::InvalidArgument(format!(
            "quarter turns must be 0 to 3, not {quarter_turns}"
        )))
    }
}

fn validate_content(op: &EditOp) -> Result<(), EngineError> {
    match op {
        EditOp::AddText {
            text,
            size,
            quarter_turns,
            ..
        } => {
            positive("font size", *size, 1000.0)?;
            turns(*quarter_turns)?;
            if text.is_empty() {
                return Err(EngineError::InvalidArgument("the text is empty".into()));
            }
        }
        EditOp::AddImage {
            bounds,
            quarter_turns,
            ..
        } => {
            turns(*quarter_turns)?;
            positive("image width", bounds.width(), MAX_PAGE_SIZE)?;
            positive("image height", bounds.height(), MAX_PAGE_SIZE)?;
        }
        EditOp::AddPath { stroke, fill, .. } => {
            if let Some(s) = stroke {
                positive("stroke width", s.width, 1000.0)?;
            }
            if stroke.is_none() && fill.is_none() {
                return Err(EngineError::InvalidArgument(
                    "a path needs a stroke, a fill, or both".into(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Runs content ops on one page, writes the content once, opens the page again, and
/// compares the result with the expected fingerprint (ADR 0015). The reopened page is
/// the new state. An error gives the index of the op in `ops`.
fn edit_page(
    d: &mut Document,
    page: usize,
    ops: &[EditOp],
) -> Result<Vec<Applied>, (usize, EngineError)> {
    let mut fonts = Vec::with_capacity(ops.len());
    for (k, op) in ops.iter().enumerate() {
        validate_content(op).map_err(|e| (k, e))?;
        fonts.push(match op {
            EditOp::AddText { font, .. } => Some(d.font(font).map_err(|e| (k, e))?),
            _ => None,
        });
    }
    let p = d.page(page).map_err(|e| (0, e))?;
    let mut want = p.fingerprint().map_err(|e| (0, e))?;
    let mut out = Vec::with_capacity(ops.len());
    for (k, (op, font)) in ops.iter().zip(fonts).enumerate() {
        let applied = match op {
            EditOp::AddText {
                origin,
                text,
                size,
                color,
                quarter_turns,
                ..
            } => {
                let font = font.expect("font loaded above");
                want.push((ObjectKind::Text, text.clone()));
                p.add_text(font, *origin, text, *size, *color, *quarter_turns)
                    .map(|index| Applied::Object { index })
            }
            EditOp::AddImage {
                bounds,
                data,
                quarter_turns,
                ..
            } => {
                want.push((ObjectKind::Image, String::new()));
                p.add_image(*bounds, data, *quarter_turns)
                    .map(|index| Applied::Object { index })
            }
            EditOp::AddPath {
                segments,
                stroke,
                fill,
                ..
            } => {
                want.push((ObjectKind::Path, String::new()));
                p.add_path(segments, stroke.as_ref(), *fill)
                    .map(|index| Applied::Object { index })
            }
            EditOp::RemoveObject { index, .. } => p.remove_object(*index).map(|_| {
                want.remove(*index);
                Applied::Done
            }),
            _ => unreachable!("content_page accepts only content ops"),
        };
        out.push(applied.map_err(|e| (k, e))?);
    }
    p.generate().map_err(|e| (0, e))?;
    drop(p);
    let got = d
        .page(page)
        .and_then(|p| p.fingerprint())
        .map_err(|e| (0, e))?;
    if got != want {
        let at = want
            .iter()
            .zip(&got)
            .position(|(a, b)| a != b)
            .unwrap_or(want.len().min(got.len()));
        return Err((
            0,
            EngineError::UnsupportedContent(format!(
                "PDFium cannot write page {page} again without loss (object {at} of {} changed).                  This happens for example with Type3 fonts.",
                want.len()
            )),
        ));
    }
    Ok(out)
}

fn apply_structure(d: &mut Document, op: &EditOp) -> Result<Applied, EngineError> {
    match op {
        EditOp::DeletePage { page } => d.delete_page(*page),
        EditOp::MovePage { page, to } => d.move_page(*page, *to),
        EditOp::SetRotation {
            page,
            quarter_turns,
        } => d.page(*page).map(|p| p.set_rotation(*quarter_turns)),
        EditOp::InsertBlankPage { at, width, height } => {
            positive("page width", *width, MAX_PAGE_SIZE)?;
            positive("page height", *height, MAX_PAGE_SIZE)?;
            d.insert_blank_page(*at, *width, *height)
        }
        EditOp::ImportPages { source, pages, at } => d.import_pages(source.clone(), pages, *at),
        EditOp::RemoveAnnotation { page, index } => d.page(*page)?.remove_annotation(*index),
        _ => unreachable!("content ops go through edit_page"),
    }
    .map(|_| Applied::Done)
}
