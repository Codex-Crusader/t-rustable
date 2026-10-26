//! Engine requests and replies. Plain data, so that a later child-process engine can send
//! them over a pipe (ADR 0015). Pages and objects are addressed by index. Only the core
//! maps stable IDs to indices.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The core picks the key when it opens a document. One key per open document.
pub type DocKey = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// Visible tiles and user edits.
    Interactive,
    /// Thumbnails, search, mod queries.
    Normal,
    /// Prefetch.
    Background,
}

/// PDF user space, in points.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// PDF user space, in points. `bottom` is less than `top`.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl Rect {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.top - self.bottom
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f64,
    /// Dash and gap lengths in points. Empty for a solid line.
    pub dash: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    /// Cubic Bezier: two control points, then the end point.
    CubicTo(Point, Point, Point),
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ImageData {
    Png(Arc<[u8]>),
    Jpeg(Arc<[u8]>),
}

/// One change to a document. The engine applies a list of ops in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EditOp {
    DeletePage {
        page: usize,
    },
    /// Moves the page so that it gets index `to` (in the list after the move).
    MovePage {
        page: usize,
        to: usize,
    },
    /// Sets the page rotation: 0 to 3 quarter turns, clockwise.
    SetRotation {
        page: usize,
        quarter_turns: u8,
    },
    /// Inserts a blank page at index `at`.
    InsertBlankPage {
        at: usize,
        width: f64,
        height: f64,
    },
    /// Adds a text object at the end of the page object list. `origin` is the start of the
    /// baseline. `font` is a TrueType file, loaded as a CID font.
    AddText {
        page: usize,
        origin: Point,
        text: String,
        font: Arc<[u8]>,
        size: f64,
        color: Rgba,
        /// Quarter turns counterclockwise about `origin`, 0 to 3.
        #[serde(default)]
        quarter_turns: u8,
    },
    /// Adds an image object at the end of the page object list.
    AddImage {
        page: usize,
        bounds: Rect,
        data: ImageData,
        /// Quarter turns counterclockwise of the image inside `bounds`, 0 to 3.
        /// At 1 or 3, the image width runs along the height of `bounds`.
        #[serde(default)]
        quarter_turns: u8,
    },
    /// Adds a path object at the end of the page object list.
    AddPath {
        page: usize,
        segments: Vec<PathSegment>,
        stroke: Option<Stroke>,
        fill: Option<Rgba>,
    },
    RemoveObject {
        page: usize,
        index: usize,
    },
    RemoveAnnotation {
        page: usize,
        index: usize,
    },
    /// Copies pages of another PDF to index `at`. The source must not be encrypted.
    ImportPages {
        source: Arc<[u8]>,
        pages: Vec<usize>,
        at: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Text,
    Image,
    Path,
    Shading,
    Form,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub kind: ObjectKind,
    pub bounds: Rect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnotationInfo {
    /// PDF subtype name, for example "Highlight".
    pub subtype: String,
    pub bounds: Rect,
    pub contents: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    pub label: Option<String>,
    pub media_box: Rect,
    pub crop_box: Rect,
    /// 0 to 3 quarter turns, clockwise.
    pub quarter_turns: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharInfo {
    pub c: char,
    pub bounds: Rect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    /// Index of the text object on the page, if PDFium knows it.
    pub object: Option<usize>,
    pub text: String,
    pub font_name: String,
    pub font_size: f64,
    pub bounds: Rect,
    pub chars: Vec<CharInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub page: usize,
    pub text: String,
    pub boxes: Vec<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    Text,
    Checkbox,
    Radio,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub page: usize,
    /// Index of the widget annotation on the page.
    pub annotation: usize,
    pub name: String,
    pub kind: FieldKind,
    pub value: Option<String>,
    pub bounds: Rect,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocSummary {
    pub page_count: usize,
    pub signature_count: usize,
    pub metadata: Metadata,
    /// The file has a security handler (a user or owner password).
    #[serde(default)]
    pub encrypted: bool,
}

/// One bookmark of the document outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    /// The page that the bookmark goes to. None if it has no destination in this document.
    pub page: Option<usize>,
    pub children: Vec<OutlineItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub max_hits: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Query {
    Summary,
    PageInfo {
        page: usize,
    },
    Objects {
        page: usize,
    },
    Annotations {
        page: usize,
    },
    TextRuns {
        page: usize,
    },
    /// All text of the page, as PDFium extracts it.
    PageText {
        page: usize,
    },
    Search {
        query: String,
        options: SearchOptions,
    },
    FormFields,
    /// The bookmarks. The engine stops at 64 levels and at 10,000 items.
    Outline,
}

/// Part of a page, in device pixels after scaling. Used for tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Opens a document. A document with the same key closes first.
    Open {
        doc: DocKey,
        bytes: Arc<[u8]>,
        password: Option<String>,
    },
    Close {
        doc: DocKey,
    },
    /// Saves to bytes. Always a full rewrite.
    Save {
        doc: DocKey,
    },
    /// Applies the ops in order. On an error, the ops before it stay applied. The caller
    /// must then open the document again from bytes (ADR 0015).
    Apply {
        doc: DocKey,
        ops: Vec<EditOp>,
    },
    Query {
        doc: DocKey,
        query: Query,
    },
    /// Renders a page at `scale` device pixels per point, with rotation and annotations.
    Render {
        doc: DocKey,
        page: usize,
        scale: f64,
        clip: Option<Clip>,
    },
    /// Panics on the engine thread. Tests of the restart path only.
    #[cfg(feature = "test-hooks")]
    TestPanic,
    /// Test hook: holds the engine thread for a time.
    #[cfg(feature = "test-hooks")]
    TestSleep {
        millis: u64,
    },
}

/// BGRA pixels, top row first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bgra: Vec<u8>,
}

impl Bitmap {
    /// PNG with `Fast` compression (M1: under 1 ms per tile).
    pub fn to_png(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity((self.width * self.height * 3) as usize);
        for row in self
            .bgra
            .chunks_exact(self.stride as usize)
            .take(self.height as usize)
        {
            for px in row[..(self.width * 4) as usize].as_chunks::<4>().0 {
                rgb.extend_from_slice(&[px[2], px[1], px[0]]);
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        // Writing to a Vec cannot fail, and the size matches the header.
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(&rgb).expect("PNG data");
        drop(writer);
        out
    }
}

/// Result of one op in `Request::Apply`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Applied {
    Done,
    /// The op added a page object at this index.
    Object {
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Opened(DocSummary),
    Closed,
    Saved(Vec<u8>),
    Applied(Vec<Applied>),
    Summary(DocSummary),
    PageInfo(PageInfo),
    Objects(Vec<ObjectInfo>),
    Annotations(Vec<AnnotationInfo>),
    TextRuns(Vec<TextRun>),
    PageText(String),
    SearchHits(Vec<SearchHit>),
    FormFields(Vec<FormField>),
    Outline(Vec<OutlineItem>),
    Rendered(Bitmap),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EngineError {
    /// The sender cancelled the request.
    Cancelled,
    /// The engine panicked during this request, then restarted PDFium. All documents
    /// are closed. Open them again.
    Restarted,
    /// No open document has this key. After a restart, all keys are gone.
    UnknownDocument,
    /// The document needs a password, or the password is wrong.
    Password,
    /// The bytes are not a PDF that PDFium can open.
    BadFile(String),
    /// A page or object index is out of range, or a value is not valid.
    InvalidArgument(String),
    /// PDFium cannot write this page again without loss, for example with Type3 fonts.
    UnsupportedContent(String),
    /// An image is not a PNG or JPEG that the engine can read.
    UnsupportedImage(String),
    /// Op `index` of an `Apply` request failed. The ops before it stay applied.
    OpFailed {
        index: usize,
        error: Box<EngineError>,
    },
    /// An error in PDFium or in the engine.
    Internal(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "the request was cancelled"),
            Self::Restarted => write!(f, "the engine restarted after an internal error"),
            Self::UnknownDocument => write!(f, "the document is not open in the engine"),
            Self::Password => write!(f, "the document needs a password, or the password is wrong"),
            Self::BadFile(m) => write!(f, "the file cannot be opened: {m}"),
            Self::InvalidArgument(m) => write!(f, "invalid argument: {m}"),
            Self::UnsupportedContent(m) => write!(f, "unsupported page content: {m}"),
            Self::UnsupportedImage(m) => write!(f, "unsupported image: {m}"),
            Self::OpFailed { index, error } => write!(f, "operation {index} failed: {error}"),
            Self::Internal(m) => write!(f, "internal engine error: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_skips_the_row_padding() {
        let b = Bitmap {
            width: 2,
            height: 2,
            stride: 12,
            bgra: vec![
                0, 0, 255, 255, 0, 255, 0, 255, 9, 9, 9, 9, //
                255, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9, 9,
            ],
        };
        let png = b.to_png();
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..12], &[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
    }
}
