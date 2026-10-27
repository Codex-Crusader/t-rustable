//! Core commands (ADR 0004). A command is plain data with all of its inputs.
//!
//! `step` is the `validate` and `apply` handler of every command: it checks the command
//! against a state, changes the state, and returns the engine ops. The session runs it on
//! a copy while a transaction is open, and again at commit to get the ops.

use crate::assets::{AssetId, Assets, ImageFormat, sniff_image};
use crate::error::{CoreError, Result};
use crate::fonts::check_glyphs;
use crate::ids::{IdMap, ObjectId, PageContent, PageEntry, PageId};
use serde::{Deserialize, Serialize};
use t_rustable_engine::{EditOp, ImageData, PathSegment, Point, Rect, Rgba, Stroke};

/// Largest page side and coordinate, in points (200 inches).
pub const MAX_COORD: f64 = 14_400.0;
pub const MAX_TEXT_CHARS: usize = 10_000;
pub const MAX_FONT_SIZE: f64 = 1_000.0;
pub const MAX_INSERTED_PAGES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Rect(Rect),
    Ellipse(Rect),
    Line(Point, Point),
    Polyline(Vec<Point>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Command {
    DeletePage {
        page: PageId,
    },
    /// Moves the page before `before`. None moves it to the end.
    MovePage {
        page: PageId,
        before: Option<PageId>,
    },
    /// Adds `degrees` to the rotation. A multiple of 90. Positive is clockwise.
    RotatePage {
        page: PageId,
        degrees: i32,
    },
    InsertBlankPage {
        id: PageId,
        before: Option<PageId>,
        width: f64,
        height: f64,
    },
    /// Copies pages of another PDF before `before`. None puts them at the end. `source`
    /// is the PDF file as an asset. `pages` are its page indices, from 0. Page `pages[k]`
    /// gets the ID `ids[k]`.
    InsertPdfPages {
        ids: Vec<PageId>,
        before: Option<PageId>,
        source: AssetId,
        pages: Vec<usize>,
    },
    AddText {
        id: ObjectId,
        page: PageId,
        at: Point,
        text: String,
        font: AssetId,
        size: f64,
        color: Rgba,
        /// Quarter turns counterclockwise, 0 to 3. None: upright for the reader at the
        /// page rotation. `Session::add` stores the resolved value in the journal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turns: Option<u8>,
    },
    /// At 1 or 3 turns, the image width runs along the height of `bounds`.
    AddImage {
        id: ObjectId,
        page: PageId,
        bounds: Rect,
        image: AssetId,
        /// Quarter turns counterclockwise, 0 to 3. None: upright for the reader at the
        /// page rotation. `Session::add` stores the resolved value in the journal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turns: Option<u8>,
    },
    AddShape {
        id: ObjectId,
        page: PageId,
        shape: Shape,
        stroke: Option<Stroke>,
        fill: Option<Rgba>,
    },
    RemoveObject {
        page: PageId,
        object: ObjectId,
    },
}

/// Reads the rotation and content IDs of a page of the live document.
pub trait PageReader {
    fn read(&mut self, page: PageId) -> Result<(u8, PageContent)>;
}

/// Engine ops of one command, and the object index that each op must report.
pub type Ops = Vec<(EditOp, Option<usize>)>;

fn page_index(state: &IdMap, page: PageId) -> Result<usize> {
    state
        .page_index(page)
        .ok_or_else(|| CoreError::not_found(format!("Page {page}")))
}

/// Reads the page from the live document the first time.
fn loaded<'a>(
    state: &'a mut IdMap,
    index: usize,
    reader: &mut dyn PageReader,
) -> Result<&'a mut PageEntry> {
    let entry = state.entry_mut(index);
    if entry.rotation.is_none() || entry.content.is_none() {
        let (rotation, content) = reader.read(entry.id)?;
        entry.rotation.get_or_insert(rotation);
        entry.content.get_or_insert(content);
    }
    Ok(entry)
}

/// The given quarter turns, or the page rotation, so that new content reads upright.
fn resolve_turns(
    state: &mut IdMap,
    index: usize,
    turns: Option<u8>,
    reader: &mut dyn PageReader,
) -> Result<u8> {
    match turns {
        Some(t) if t < 4 => Ok(t),
        Some(t) => Err(CoreError::invalid(format!(
            "The quarter turns must be 0 to 3. They are {t}."
        ))),
        None => Ok(loaded(state, index, reader)?.rotation.unwrap()),
    }
}

fn finite(name: &str, values: &[f64]) -> Result<()> {
    if values.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) {
        Ok(())
    } else {
        Err(CoreError::invalid(format!("The {name} is out of range.")))
    }
}

fn in_range(name: &str, v: f64, max: f64) -> Result<()> {
    if v.is_finite() && v > 0.0 && v <= max {
        Ok(())
    } else {
        Err(CoreError::invalid(format!(
            "The {name} must be more than 0 and not more than {max}. It is {v}."
        )))
    }
}

fn check_rect(name: &str, r: &Rect) -> Result<()> {
    finite(name, &[r.left, r.bottom, r.right, r.top])?;
    in_range(&format!("{name} width"), r.width(), MAX_COORD)?;
    in_range(&format!("{name} height"), r.height(), MAX_COORD)
}

fn check_new_object(state: &IdMap, id: ObjectId) -> Result<()> {
    if state.find_object(id).is_some() {
        return Err(CoreError::invalid(format!(
            "The object ID {id} is already in use."
        )));
    }
    Ok(())
}

/// Appends a new object ID to the page and returns its predicted engine index.
fn push_object(
    state: &mut IdMap,
    index: usize,
    id: ObjectId,
    reader: &mut dyn PageReader,
) -> Result<usize> {
    let content = loaded(state, index, reader)?.content.as_mut().unwrap();
    content.objects.push(id);
    Ok(content.objects.len() - 1)
}

/// Kappa for a circle from four cubic Bézier curves.
const KAPPA: f64 = 0.552_284_749_830_793_4;

pub fn shape_segments(shape: &Shape) -> Vec<PathSegment> {
    let p = |x, y| Point { x, y };
    match shape {
        Shape::Rect(r) => vec![
            PathSegment::MoveTo(p(r.left, r.bottom)),
            PathSegment::LineTo(p(r.right, r.bottom)),
            PathSegment::LineTo(p(r.right, r.top)),
            PathSegment::LineTo(p(r.left, r.top)),
            PathSegment::Close,
        ],
        Shape::Ellipse(r) => {
            let (cx, cy) = ((r.left + r.right) / 2.0, (r.bottom + r.top) / 2.0);
            let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
            let (kx, ky) = (KAPPA * rx, KAPPA * ry);
            vec![
                PathSegment::MoveTo(p(cx + rx, cy)),
                PathSegment::CubicTo(p(cx + rx, cy + ky), p(cx + kx, cy + ry), p(cx, cy + ry)),
                PathSegment::CubicTo(p(cx - kx, cy + ry), p(cx - rx, cy + ky), p(cx - rx, cy)),
                PathSegment::CubicTo(p(cx - rx, cy - ky), p(cx - kx, cy - ry), p(cx, cy - ry)),
                PathSegment::CubicTo(p(cx + kx, cy - ry), p(cx + rx, cy - ky), p(cx + rx, cy)),
                PathSegment::Close,
            ]
        }
        Shape::Line(a, b) => vec![PathSegment::MoveTo(*a), PathSegment::LineTo(*b)],
        Shape::Polyline(points) => points
            .iter()
            .enumerate()
            .map(|(i, &pt)| {
                if i == 0 {
                    PathSegment::MoveTo(pt)
                } else {
                    PathSegment::LineTo(pt)
                }
            })
            .collect(),
    }
}

fn check_shape(shape: &Shape, stroke: Option<&Stroke>, fill: Option<Rgba>) -> Result<()> {
    if stroke.is_none() && fill.is_none() {
        return Err(CoreError::invalid(
            "A shape needs a stroke, a fill, or both.",
        ));
    }
    if let Some(s) = stroke {
        in_range("stroke width", s.width, 1000.0)?;
        if s.dash
            .iter()
            .any(|d| !d.is_finite() || *d < 0.0 || *d > MAX_COORD)
            || (!s.dash.is_empty() && s.dash.iter().all(|d| *d == 0.0))
        {
            return Err(CoreError::invalid("The dash lengths are not valid."));
        }
    }
    match shape {
        Shape::Rect(r) | Shape::Ellipse(r) => check_rect("shape", r),
        Shape::Line(a, b) => {
            finite("line", &[a.x, a.y, b.x, b.y])?;
            if stroke.is_none() || fill.is_some() {
                return Err(CoreError::invalid("A line needs a stroke and no fill."));
            }
            Ok(())
        }
        Shape::Polyline(points) => {
            if points.len() < 2 || points.len() > 100_000 {
                return Err(CoreError::invalid("A polyline needs 2 to 100000 points."));
            }
            let coords: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y]).collect();
            finite("polyline", &coords)
        }
    }
}

impl Command {
    /// Fills an empty `turns` of `AddText` and `AddImage` from the page rotation in
    /// `state`. Thus the journal does not depend on the rule for upright content.
    pub fn resolve(&mut self, state: &mut IdMap, reader: &mut dyn PageReader) {
        if let Command::AddText {
            page,
            turns: t @ None,
            ..
        }
        | Command::AddImage {
            page,
            turns: t @ None,
            ..
        } = self
            && let Some(i) = state.page_index(*page)
            && let Ok(entry) = loaded(state, i, reader)
        {
            *t = entry.rotation;
        }
    }

    /// Checks the command against `state`, changes `state`, and returns the engine ops.
    /// On error, `state` can be partly changed. Use a copy.
    pub fn step(
        &self,
        state: &mut IdMap,
        assets: &Assets,
        reader: &mut dyn PageReader,
    ) -> Result<Ops> {
        Ok(match self {
            Command::DeletePage { page } => {
                let i = page_index(state, *page)?;
                if state.page_count() == 1 {
                    return Err(CoreError::invalid(
                        "A document must keep at least one page.",
                    ));
                }
                state.remove(i);
                vec![(EditOp::DeletePage { page: i }, None)]
            }
            Command::MovePage { page, before } => {
                let i = page_index(state, *page)?;
                let to = match before {
                    None => state.page_count() - 1,
                    Some(b) if b == page => {
                        return Err(CoreError::invalid("A page cannot move before itself."));
                    }
                    Some(b) => {
                        let j = page_index(state, *b)?;
                        if i < j { j - 1 } else { j }
                    }
                };
                let entry = state.remove(i);
                state.insert(to, [entry]);
                if to == i {
                    Vec::new()
                } else {
                    vec![(EditOp::MovePage { page: i, to }, None)]
                }
            }
            Command::RotatePage { page, degrees } => {
                if degrees % 90 != 0 {
                    return Err(CoreError::invalid(format!(
                        "The rotation must be a multiple of 90 degrees. It is {degrees}."
                    )));
                }
                let i = page_index(state, *page)?;
                let entry = loaded(state, i, reader)?;
                let turns = (entry.rotation.unwrap() as i32 + degrees / 90).rem_euclid(4) as u8;
                entry.rotation = Some(turns);
                vec![(
                    EditOp::SetRotation {
                        page: i,
                        quarter_turns: turns,
                    },
                    None,
                )]
            }
            Command::InsertBlankPage {
                id,
                before,
                width,
                height,
            } => {
                if state.page_index(*id).is_some() {
                    return Err(CoreError::invalid(format!(
                        "The page ID {id} is already in use."
                    )));
                }
                in_range("page width", *width, MAX_COORD)?;
                in_range("page height", *height, MAX_COORD)?;
                let at = match before {
                    Some(b) => page_index(state, *b)?,
                    None => state.page_count(),
                };
                let entry = PageEntry {
                    id: *id,
                    rotation: Some(0),
                    content: Some(PageContent::default()),
                };
                state.insert(at, [entry]);
                vec![(
                    EditOp::InsertBlankPage {
                        at,
                        width: *width,
                        height: *height,
                    },
                    None,
                )]
            }
            Command::InsertPdfPages {
                ids,
                before,
                source,
                pages,
            } => {
                if ids.is_empty() || ids.len() > MAX_INSERTED_PAGES {
                    return Err(CoreError::invalid(format!(
                        "Insert 1 to {MAX_INSERTED_PAGES} pages."
                    )));
                }
                if ids.len() != pages.len() {
                    return Err(CoreError::invalid(
                        "Each inserted page needs exactly one page ID.",
                    ));
                }
                let mut seen = std::collections::HashSet::new();
                for id in ids {
                    if state.page_index(*id).is_some() || !seen.insert(*id) {
                        return Err(CoreError::invalid(format!(
                            "The page ID {id} is already in use."
                        )));
                    }
                }
                let data = assets
                    .get(*source)
                    .ok_or_else(|| CoreError::not_found(format!("PDF asset {source}")))?;
                let at = match before {
                    Some(b) => page_index(state, *b)?,
                    None => state.page_count(),
                };
                // The core reads the new pages from the engine after the commit.
                state.insert(
                    at,
                    ids.iter().map(|id| PageEntry {
                        id: *id,
                        rotation: None,
                        content: None,
                    }),
                );
                vec![(
                    EditOp::ImportPages {
                        source: data.clone(),
                        pages: pages.clone(),
                        at,
                    },
                    None,
                )]
            }
            Command::AddText {
                id,
                page,
                at,
                text,
                font,
                size,
                color,
                turns,
            } => {
                let i = page_index(state, *page)?;
                check_new_object(state, *id)?;
                let quarter_turns = resolve_turns(state, i, *turns, reader)?;
                finite("text position", &[at.x, at.y])?;
                in_range("font size", *size, MAX_FONT_SIZE)?;
                if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS {
                    return Err(CoreError::invalid(format!(
                        "The text must have 1 to {MAX_TEXT_CHARS} characters."
                    )));
                }
                if text.chars().any(char::is_control) {
                    return Err(CoreError::invalid(
                        "The text contains a line break or another control character.",
                    )
                    .with_hint("Add each line as a separate text."));
                }
                let data = assets
                    .get(*font)
                    .ok_or_else(|| CoreError::not_found(format!("Font asset {font}")))?;
                check_glyphs(data, text)?;
                let index = push_object(state, i, *id, reader)?;
                let op = EditOp::AddText {
                    page: i,
                    origin: *at,
                    text: text.clone(),
                    font: data.clone(),
                    size: *size,
                    color: *color,
                    quarter_turns,
                };
                vec![(op, Some(index))]
            }
            Command::AddImage {
                id,
                page,
                bounds,
                image,
                turns,
            } => {
                let i = page_index(state, *page)?;
                check_new_object(state, *id)?;
                let quarter_turns = resolve_turns(state, i, *turns, reader)?;
                check_rect("image", bounds)?;
                let data = assets
                    .get(*image)
                    .ok_or_else(|| CoreError::not_found(format!("Image asset {image}")))?;
                let data = match sniff_image(data) {
                    Some(ImageFormat::Png) => ImageData::Png(data.clone()),
                    Some(ImageFormat::Jpeg) => ImageData::Jpeg(data.clone()),
                    None => {
                        return Err(CoreError::new(
                            crate::error::code::UNSUPPORTED_FORMAT,
                            "The image is not a PNG or JPEG file.",
                        ));
                    }
                };
                let index = push_object(state, i, *id, reader)?;
                vec![(
                    EditOp::AddImage {
                        page: i,
                        bounds: *bounds,
                        data,
                        quarter_turns,
                    },
                    Some(index),
                )]
            }
            Command::AddShape {
                id,
                page,
                shape,
                stroke,
                fill,
            } => {
                let i = page_index(state, *page)?;
                check_new_object(state, *id)?;
                check_shape(shape, stroke.as_ref(), *fill)?;
                let index = push_object(state, i, *id, reader)?;
                let op = EditOp::AddPath {
                    page: i,
                    segments: shape_segments(shape),
                    stroke: stroke.clone(),
                    fill: *fill,
                };
                vec![(op, Some(index))]
            }
            Command::RemoveObject { page, object } => {
                let i = page_index(state, *page)?;
                let content = loaded(state, i, reader)?.content.as_mut().unwrap();
                if let Some(index) = content.objects.iter().position(|o| o == object) {
                    content.objects.remove(index);
                    return Ok(vec![(EditOp::RemoveObject { page: i, index }, None)]);
                }
                let Some(index) = content.annotations.iter().position(|a| a.id == *object) else {
                    return Err(CoreError::not_found(format!(
                        "Object {object} on page {page}"
                    )));
                };
                if content.annotations[index].widget {
                    return Err(CoreError::invalid("API 0.1 cannot remove form fields."));
                }
                content.annotations.remove(index);
                vec![(EditOp::RemoveAnnotation { page: i, index }, None)]
            }
        })
    }

    /// A short text for the UI. Page numbers come from `state`, the state before the command.
    pub fn describe(&self, state: &IdMap) -> String {
        let n = |p: &PageId| {
            state
                .page_index(*p)
                .map_or("?".to_string(), |i| (i + 1).to_string())
        };
        match self {
            Command::DeletePage { page } => format!("Delete page {}", n(page)),
            Command::MovePage {
                page,
                before: Some(b),
            } => format!("Move page {} before page {}", n(page), n(b)),
            Command::MovePage { page, before: None } => format!("Move page {} to the end", n(page)),
            Command::RotatePage { page, degrees } => {
                format!("Rotate page {} by {degrees} degrees", n(page))
            }
            Command::InsertBlankPage {
                before: Some(b), ..
            } => format!("Insert a blank page before page {}", n(b)),
            Command::InsertBlankPage { before: None, .. } => {
                "Insert a blank page at the end".to_string()
            }
            Command::InsertPdfPages {
                ids,
                before: Some(b),
                ..
            } => format!("Insert {} pages before page {}", ids.len(), n(b)),
            Command::InsertPdfPages {
                ids, before: None, ..
            } => format!("Insert {} pages at the end", ids.len()),
            Command::AddText { page, text, .. } => {
                let short: String = text.chars().take(30).collect();
                let more = if text.chars().count() > 30 { "..." } else { "" };
                format!("Add text \"{short}{more}\" on page {}", n(page))
            }
            Command::AddImage { page, .. } => format!("Add an image on page {}", n(page)),
            Command::AddShape { page, shape, .. } => {
                let kind = match shape {
                    Shape::Rect(_) => "a rectangle",
                    Shape::Ellipse(_) => "an ellipse",
                    Shape::Line(..) => "a line",
                    Shape::Polyline(_) => "a polyline",
                };
                format!("Add {kind} on page {}", n(page))
            }
            Command::RemoveObject { page, .. } => format!("Remove an object from page {}", n(page)),
        }
    }

    /// The pages that the command changes, and whether pages were added, removed, or moved.
    pub fn affected(&self) -> (Option<PageId>, bool) {
        match self {
            Command::DeletePage { page } | Command::MovePage { page, .. } => (Some(*page), true),
            Command::InsertBlankPage { id, .. } => (Some(*id), true),
            Command::InsertPdfPages { ids, .. } => (ids.first().copied(), true),
            Command::RotatePage { page, .. }
            | Command::AddText { page, .. }
            | Command::AddImage { page, .. }
            | Command::AddShape { page, .. }
            | Command::RemoveObject { page, .. } => (Some(*page), false),
        }
    }

    /// Assets that the command needs.
    pub fn assets(&self) -> Option<AssetId> {
        match self {
            Command::AddText { font, .. } => Some(*font),
            Command::AddImage { image, .. } => Some(*image),
            Command::InsertPdfPages { source, .. } => Some(*source),
            _ => None,
        }
    }
}

/// Where an object is in `state`, for the commands that take only an object ID.
pub fn locate(state: &IdMap, object: ObjectId) -> Result<PageId> {
    state
        .find_object(object)
        .map(|(page, _)| page)
        .ok_or_else(|| CoreError::not_found(format!("Object {object}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::code;
    use crate::ids::{AnnotationEntry, Seed};
    use std::sync::Arc;

    /// A fake live document: each page has rotation 1, two objects, and one widget.
    struct Fake {
        seed: Seed,
        reads: usize,
    }

    impl PageReader for Fake {
        fn read(&mut self, page: PageId) -> Result<(u8, PageContent)> {
            self.reads += 1;
            let content = PageContent {
                objects: vec![self.seed.object(page, 0), self.seed.object(page, 1)],
                annotations: vec![AnnotationEntry {
                    id: self.seed.annotation(page, 0),
                    widget: true,
                }],
            };
            Ok((1, content))
        }
    }

    struct T {
        state: IdMap,
        assets: Assets,
        fake: Fake,
        font: AssetId,
        png: AssetId,
    }

    fn setup(pages: usize) -> T {
        let seed = Seed([3; 32]);
        let mut assets = Assets::default();
        let font = assets.add(
            crate::fonts::test_fonts()
                .select("Noto Sans", false, false)
                .unwrap(),
        );
        let png = assets.add(Arc::from(t_rustable_engine::testing::png(4, 4)));
        T {
            state: IdMap::original(&seed, pages),
            assets,
            fake: Fake { seed, reads: 0 },
            font,
            png,
        }
    }

    impl T {
        fn page(&self, i: usize) -> PageId {
            self.state.pages()[i].id
        }
        fn step(&mut self, c: &Command) -> Result<Ops> {
            c.step(&mut self.state, &self.assets, &mut self.fake)
        }
        fn err(&mut self, c: &Command) -> CoreError {
            let before = self.state.clone();
            let mut copy = self.state.clone();
            let e = c.step(&mut copy, &self.assets, &mut self.fake).unwrap_err();
            assert_eq!(self.state, before);
            e
        }
    }

    fn ops(o: Ops) -> Vec<EditOp> {
        o.into_iter().map(|(op, _)| op).collect()
    }

    const BOX: Rect = Rect {
        left: 10.0,
        bottom: 10.0,
        right: 50.0,
        top: 30.0,
    };
    const RED: Rgba = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };

    #[test]
    fn delete_page() {
        let mut t = setup(3);
        let p1 = t.page(1);
        let c = Command::DeletePage { page: p1 };
        assert_eq!(c.describe(&t.state), "Delete page 2");
        assert_eq!(ops(t.step(&c).unwrap()), [EditOp::DeletePage { page: 1 }]);
        assert_eq!(t.state.page_count(), 2);
        assert_eq!(t.err(&c).code, code::NOT_FOUND);
        let mut one = setup(1);
        let only = one.page(0);
        assert_eq!(
            one.err(&Command::DeletePage { page: only }).code,
            code::INVALID_ARGUMENT
        );
    }

    #[test]
    fn move_page_forward_backward_and_to_end() {
        let mut t = setup(4);
        let [a, b, c, d] = [t.page(0), t.page(1), t.page(2), t.page(3)];
        // Forward: a before d gives final index 2.
        let mv = Command::MovePage {
            page: a,
            before: Some(d),
        };
        assert_eq!(mv.describe(&t.state), "Move page 1 before page 4");
        assert_eq!(
            ops(t.step(&mv).unwrap()),
            [EditOp::MovePage { page: 0, to: 2 }]
        );
        assert_eq!(t.state.page_ids(), [b, c, a, d]);
        // Backward: d before b gives final index 0.
        assert_eq!(
            ops(t
                .step(&Command::MovePage {
                    page: d,
                    before: Some(b)
                })
                .unwrap()),
            [EditOp::MovePage { page: 3, to: 0 }]
        );
        assert_eq!(t.state.page_ids(), [d, b, c, a]);
        // To the end.
        assert_eq!(
            ops(t
                .step(&Command::MovePage {
                    page: b,
                    before: None
                })
                .unwrap()),
            [EditOp::MovePage { page: 1, to: 3 }]
        );
        assert_eq!(t.state.page_ids(), [d, c, a, b]);
        // Already in place: no op.
        assert!(
            t.step(&Command::MovePage {
                page: c,
                before: Some(a)
            })
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            t.err(&Command::MovePage {
                page: c,
                before: Some(c)
            })
            .code,
            code::INVALID_ARGUMENT
        );
    }

    #[test]
    fn rotations_add_up_in_one_state() {
        let mut t = setup(1);
        let p = t.page(0);
        let rot = |degrees| Command::RotatePage { page: p, degrees };
        assert_eq!(rot(90).describe(&t.state), "Rotate page 1 by 90 degrees");
        // The fake page starts at 1 quarter turn.
        assert_eq!(
            ops(t.step(&rot(90)).unwrap()),
            [EditOp::SetRotation {
                page: 0,
                quarter_turns: 2
            }]
        );
        assert_eq!(
            ops(t.step(&rot(-270)).unwrap()),
            [EditOp::SetRotation {
                page: 0,
                quarter_turns: 3
            }]
        );
        assert_eq!(
            ops(t.step(&rot(450)).unwrap()),
            [EditOp::SetRotation {
                page: 0,
                quarter_turns: 0
            }]
        );
        assert_eq!(t.fake.reads, 1, "the page is read once");
        assert_eq!(t.err(&rot(45)).code, code::INVALID_ARGUMENT);
    }

    #[test]
    fn insert_pdf_pages() {
        let mut t = setup(2);
        let p1 = t.page(1);
        let source = t.assets.add(Arc::from(&b"%PDF-1.7 source"[..]));
        let c = Command::InsertPdfPages {
            ids: vec![PageId(90), PageId(91)],
            before: Some(p1),
            source,
            pages: vec![4, 0],
        };
        assert_eq!(c.describe(&t.state), "Insert 2 pages before page 2");
        assert_eq!(c.assets(), Some(source));
        assert_eq!(
            ops(t.step(&c).unwrap()),
            [EditOp::ImportPages {
                source: t.assets.get(source).unwrap().clone(),
                pages: vec![4, 0],
                at: 1,
            }]
        );
        assert_eq!(t.state.page_index(PageId(90)), Some(1));
        assert_eq!(t.state.page_index(PageId(91)), Some(2));
        assert_eq!(t.state.pages()[1].content, None, "read after the commit");
        assert_eq!(t.err(&c).code, code::INVALID_ARGUMENT, "the IDs are in use");
        let bad = |ids: Vec<PageId>, pages: Vec<usize>, source| Command::InsertPdfPages {
            ids,
            before: None,
            source,
            pages,
        };
        let twice = bad(vec![PageId(92), PageId(92)], vec![0, 1], source);
        assert_eq!(t.err(&twice).code, code::INVALID_ARGUMENT);
        let count = bad(vec![PageId(93)], vec![0, 1], source);
        assert_eq!(t.err(&count).code, code::INVALID_ARGUMENT);
        let none = bad(vec![], vec![], source);
        assert_eq!(t.err(&none).code, code::INVALID_ARGUMENT);
        let missing = bad(vec![PageId(94)], vec![0], AssetId([7; 32]));
        assert_eq!(t.err(&missing).code, code::NOT_FOUND);
        let end = bad(vec![PageId(95)], vec![0], source);
        assert!(matches!(
            &ops(t.step(&end).unwrap())[..],
            [EditOp::ImportPages { at: 4, .. }]
        ));
    }

    #[test]
    fn insert_blank_page() {
        let mut t = setup(2);
        let p1 = t.page(1);
        let id = PageId(99);
        let c = Command::InsertBlankPage {
            id,
            before: Some(p1),
            width: 100.0,
            height: 200.0,
        };
        assert_eq!(c.describe(&t.state), "Insert a blank page before page 2");
        assert_eq!(
            ops(t.step(&c).unwrap()),
            [EditOp::InsertBlankPage {
                at: 1,
                width: 100.0,
                height: 200.0
            }]
        );
        assert_eq!(t.state.page_index(id), Some(1));
        assert_eq!(t.err(&c).code, code::INVALID_ARGUMENT, "the ID is in use");
        let end = Command::InsertBlankPage {
            id: PageId(100),
            before: None,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(
            ops(t.step(&end).unwrap()),
            [EditOp::InsertBlankPage {
                at: 3,
                width: 10.0,
                height: 10.0
            }]
        );
        let bad = Command::InsertBlankPage {
            id: PageId(101),
            before: None,
            width: 0.0,
            height: 10.0,
        };
        assert_eq!(t.err(&bad).code, code::INVALID_ARGUMENT);
        // Content ops on a new page do not read the live document.
        let reads = t.fake.reads;
        let rect = Command::AddShape {
            id: ObjectId(1),
            page: id,
            shape: Shape::Rect(BOX),
            stroke: None,
            fill: Some(RED),
        };
        assert_eq!(t.step(&rect).unwrap()[0].1, Some(0));
        assert_eq!(t.fake.reads, reads);
    }

    fn text_cmd(page: PageId, font: AssetId, s: &str) -> Command {
        Command::AddText {
            id: ObjectId(7),
            page,
            at: Point { x: 72.0, y: 700.0 },
            text: s.into(),
            font,
            size: 12.0,
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            turns: None,
        }
    }

    #[test]
    fn add_text() {
        let mut t = setup(1);
        let (p, font) = (t.page(0), t.font);
        let c = text_cmd(p, font, "Hello");
        assert_eq!(c.describe(&t.state), "Add text \"Hello\" on page 1");
        let o = t.step(&c).unwrap();
        assert_eq!(o[0].1, Some(2), "after the two objects of the fake page");
        assert!(matches!(&o[0].0, EditOp::AddText { page: 0, text, .. } if text == "Hello"));
        assert_eq!(t.state.find_object(ObjectId(7)).map(|f| f.0), Some(p));
        assert_eq!(t.err(&c).code, code::INVALID_ARGUMENT, "the ID is in use");

        let mut t = setup(1);
        let p = t.page(0);
        assert_eq!(
            t.err(&text_cmd(
                p,
                font,
                "two
lines"
            ))
            .code,
            code::INVALID_ARGUMENT
        );
        assert_eq!(t.err(&text_cmd(p, font, "日本")).code, code::MISSING_GLYPH);
        assert_eq!(t.err(&text_cmd(p, font, "")).code, code::INVALID_ARGUMENT);
        assert_eq!(
            t.err(&text_cmd(p, AssetId([0; 32]), "ok")).code,
            code::NOT_FOUND
        );
    }

    #[test]
    fn add_image() {
        let mut t = setup(1);
        let p = t.page(0);
        let c = Command::AddImage {
            id: ObjectId(5),
            page: p,
            bounds: BOX,
            image: t.png,
            turns: None,
        };
        assert_eq!(c.describe(&t.state), "Add an image on page 1");
        let o = t.step(&c).unwrap();
        assert!(matches!(
            &o[0],
            (
                EditOp::AddImage {
                    page: 0,
                    data: ImageData::Png(_),
                    ..
                },
                Some(2)
            )
        ));
        let gif = t.assets.add(Arc::from(&b"GIF89a...."[..]));
        let bad = Command::AddImage {
            id: ObjectId(6),
            page: p,
            bounds: BOX,
            image: gif,
            turns: None,
        };
        assert_eq!(t.err(&bad).code, code::UNSUPPORTED_FORMAT);
        let flat = Rect { top: 10.0, ..BOX };
        let bad = Command::AddImage {
            id: ObjectId(6),
            page: p,
            bounds: flat,
            image: t.png,
            turns: None,
        };
        assert_eq!(t.err(&bad).code, code::INVALID_ARGUMENT);
    }

    #[test]
    fn add_shape() {
        let mut t = setup(1);
        let p = t.page(0);
        let stroke = Stroke {
            color: RED,
            width: 2.0,
            dash: vec![],
        };
        let shape = |id, shape, stroke: Option<Stroke>, fill| Command::AddShape {
            id: ObjectId(id),
            page: p,
            shape,
            stroke,
            fill,
        };
        let c = shape(1, Shape::Ellipse(BOX), None, Some(RED));
        assert_eq!(c.describe(&t.state), "Add an ellipse on page 1");
        let o = t.step(&c).unwrap();
        let EditOp::AddPath { segments, .. } = &o[0].0 else {
            panic!()
        };
        assert_eq!(segments.len(), 6);
        assert_eq!(segments[0], PathSegment::MoveTo(Point { x: 50.0, y: 20.0 }));
        let line = shape(
            2,
            Shape::Line(Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 5.0 }),
            Some(stroke.clone()),
            None,
        );
        assert_eq!(t.step(&line).unwrap()[0].1, Some(3));
        let filled_line = shape(
            3,
            Shape::Line(Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 5.0 }),
            Some(stroke.clone()),
            Some(RED),
        );
        assert_eq!(t.err(&filled_line).code, code::INVALID_ARGUMENT);
        assert_eq!(
            t.err(&shape(3, Shape::Rect(BOX), None, None)).code,
            code::INVALID_ARGUMENT
        );
        assert_eq!(
            t.err(&shape(
                3,
                Shape::Polyline(vec![Point { x: 0.0, y: 0.0 }]),
                Some(stroke.clone()),
                None
            ))
            .code,
            code::INVALID_ARGUMENT
        );
        let nan = Shape::Rect(Rect {
            left: f64::NAN,
            ..BOX
        });
        assert_eq!(
            t.err(&shape(3, nan, None, Some(RED))).code,
            code::INVALID_ARGUMENT
        );
        let dash = Stroke {
            dash: vec![0.0, 0.0],
            ..stroke
        };
        assert_eq!(
            t.err(&shape(3, Shape::Rect(BOX), Some(dash), None)).code,
            code::INVALID_ARGUMENT
        );
    }

    #[test]
    fn remove_object_and_annotation() {
        let mut t = setup(2);
        let p = t.page(1);
        let seed = t.fake.seed;
        let second = seed.object(p, 1);
        let c = Command::RemoveObject {
            page: p,
            object: second,
        };
        assert_eq!(c.describe(&t.state), "Remove an object from page 2");
        assert_eq!(
            ops(t.step(&c).unwrap()),
            [EditOp::RemoveObject { page: 1, index: 1 }]
        );
        assert_eq!(t.err(&c).code, code::NOT_FOUND);
        // The fake annotation is a widget.
        let widget = Command::RemoveObject {
            page: p,
            object: seed.annotation(p, 0),
        };
        assert_eq!(t.err(&widget).code, code::INVALID_ARGUMENT);
        assert_eq!(locate(&t.state, seed.object(p, 0)).unwrap(), p);
        assert_eq!(locate(&t.state, second).unwrap_err().code, code::NOT_FOUND);
    }

    #[test]
    fn commands_round_trip_as_json() {
        let t = setup(1);
        let c = Command::AddShape {
            id: ObjectId(1),
            page: t.page(0),
            shape: Shape::Line(Point { x: 1.0, y: 2.0 }, Point { x: 3.0, y: 4.0 }),
            stroke: Some(Stroke {
                color: RED,
                width: 1.0,
                dash: vec![2.0, 1.0],
            }),
            fill: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.starts_with("{\"type\":\"add-shape\""), "{json}");
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c);
    }
}
