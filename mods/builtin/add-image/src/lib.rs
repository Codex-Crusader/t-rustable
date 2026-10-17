//! Built-in mod: adds a PNG or JPEG image to a page.
//!
//! The action opens a file dialog. Then a preview follows the pointer. A click adds the
//! image at its own size (96 pixels per inch), and a drag sets the size. The image
//! keeps its aspect ratio and reads upright on a rotated page. Escape cancels.

use std::cell::RefCell;
use t_rustable_sdk::document::Doc;
use t_rustable_sdk::exports::tool::{
    Cursor, KeyEvent, KeyKind, KeyResult, PointerButton, PointerEvent, PointerKind, PointerResult,
};
use t_rustable_sdk::files::{self, FileFilter, PickOptions};
use t_rustable_sdk::types::{ApiError, OverlayImage, OverlayShape, PageId, Point, Rect};
use t_rustable_sdk::ui::{self, NotifyLevel};
use t_rustable_sdk::{Command, Tool, api_error, export_interactive, run_commands};

/// Points per pixel at 96 pixels per inch.
const POINTS_PER_PIXEL: f64 = 0.75;
/// Drags shorter than this, in points, count as a click.
const MIN_DRAG: f64 = 4.0;

struct Pending {
    data: Vec<u8>,
    overlay_image: u32,
    /// Width and height in points.
    size: (f64, f64),
    drag: Option<(PageId, Point)>,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

/// Width and height in pixels, from the PNG or JPEG header.
fn pixel_size(data: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| Some(u16::from_be_bytes([*data.get(i)?, *data.get(i + 1)?]) as u32);
    let be32 = |i: usize| Some(u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?));
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if !data.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        // SOF markers. C4, C8, and CC are other segments.
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return Some((be16(i + 7)?, be16(i + 5)?));
        }
        i += 2 + be16(i + 2)? as usize;
    }
    None
}

/// The image size in page coordinates. New images read upright, so on a page that
/// turns by 90 or 270 degrees, the image width runs along the page height.
fn oriented(document: &Doc, page: PageId, (w, h): (f64, f64)) -> (f64, f64) {
    match document.page_info(page) {
        Ok(info) if info.rotation % 180 == 90 => (h, w),
        _ => (w, h),
    }
}

/// The size at 96 pixels per inch, made smaller to fit on the page if necessary.
fn natural_size(document: &Doc, page: PageId, size: (f64, f64)) -> (f64, f64) {
    let size = oriented(document, page, size);
    let Ok(info) = document.page_info(page) else {
        return size;
    };
    let scale = (info.width / size.0).min(info.height / size.1).min(1.0);
    (size.0 * scale, size.1 * scale)
}

fn centered(at: Point, (w, h): (f64, f64)) -> Rect {
    Rect {
        left: at.x - w / 2.0,
        bottom: at.y - h / 2.0,
        right: at.x + w / 2.0,
        top: at.y + h / 2.0,
    }
}

/// A rectangle from `start` with the width of the drag and the aspect ratio of the image.
fn dragged(start: Point, at: Point, (w, h): (f64, f64)) -> Rect {
    let width = (at.x - start.x).abs();
    let height = width * h / w;
    let x = start.x + width.copysign(at.x - start.x);
    let y = start.y + height.copysign(at.y - start.y);
    Rect::from_corners(start, Point::new(x, y))
}

fn preview(p: &Pending, page: PageId, bounds: Rect) -> PointerResult {
    PointerResult {
        overlay: vec![OverlayShape::Image(OverlayImage {
            page,
            bounds,
            image: p.overlay_image,
            opacity: 0.6,
            label: "Preview of the image to add".into(),
        })],
        cursor: Cursor::Crosshair,
        handled: true,
    }
}

fn not_handled() -> PointerResult {
    PointerResult {
        overlay: Vec::new(),
        cursor: Cursor::Default,
        handled: false,
    }
}

fn pick() -> Result<Option<Pending>, ApiError> {
    let options = PickOptions {
        title: "Add an image".into(),
        filters: vec![FileFilter {
            name: "Images".into(),
            extensions: vec!["png".into(), "jpg".into(), "jpeg".into()],
        }],
    };
    let Some(file) = files::pick(&options)? else {
        return Ok(None);
    };
    // The picked file expires at the end of this call, so read it now.
    let data = file.read()?;
    let (w, h) = pixel_size(&data)
        .filter(|&(w, h)| w > 0 && h > 0)
        .ok_or_else(|| api_error("unsupported-format", "The file is not a PNG or JPEG image."))?;
    let overlay_image = ui::register_overlay_image(&data)?;
    Ok(Some(Pending {
        data,
        overlay_image,
        size: (w as f64 * POINTS_PER_PIXEL, h as f64 * POINTS_PER_PIXEL),
        drag: None,
    }))
}

struct AddImage;

impl Tool for AddImage {
    fn deactivate() {
        PENDING.with(|p| *p.borrow_mut() = None);
    }

    fn run_action(
        _document: Option<&Doc>,
        action_id: String,
        _args: String,
    ) -> Result<(), ApiError> {
        if action_id != "add-image" {
            return Err(api_error(
                "unknown-action",
                format!("Unknown action {action_id:?}."),
            ));
        }
        let pending = pick()?;
        PENDING.with(|p| *p.borrow_mut() = pending);
        Ok(())
    }

    fn on_pointer(document: &Doc, event: PointerEvent) -> PointerResult {
        PENDING.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(p) = cell.as_mut() else {
                return not_handled();
            };
            match (event.kind, p.drag) {
                (PointerKind::Down, _) if event.button == PointerButton::Primary => {
                    p.drag = Some((event.page, event.at));
                    let size = natural_size(document, event.page, p.size);
                    preview(p, event.page, centered(event.at, size))
                }
                (PointerKind::Move, Some((page, start))) if page == event.page => {
                    let distance = (event.at.x - start.x).hypot(event.at.y - start.y);
                    let bounds = if distance < MIN_DRAG {
                        centered(start, natural_size(document, page, p.size))
                    } else {
                        dragged(start, event.at, oriented(document, page, p.size))
                    };
                    preview(p, page, bounds)
                }
                (PointerKind::Move, None) => {
                    let size = natural_size(document, event.page, p.size);
                    preview(p, event.page, centered(event.at, size))
                }
                (PointerKind::Up, Some((page, start))) => {
                    let at = if page == event.page { event.at } else { start };
                    let bounds = if (at.x - start.x).hypot(at.y - start.y) < MIN_DRAG {
                        centered(start, natural_size(document, page, p.size))
                    } else {
                        dragged(start, at, oriented(document, page, p.size))
                    };
                    let data = std::mem::take(&mut p.data);
                    *cell = None;
                    let command = Command::AddImage { page, bounds, data };
                    if let Err(e) = run_commands(document, "Add an image", vec![command]) {
                        ui::notify(NotifyLevel::Error, &e.message);
                    }
                    PointerResult {
                        overlay: Vec::new(),
                        cursor: Cursor::Default,
                        handled: true,
                    }
                }
                _ => not_handled(),
            }
        })
    }

    fn on_key(_document: &Doc, event: KeyEvent) -> KeyResult {
        let handled = event.kind == KeyKind::Down
            && event.key == "Escape"
            && PENDING.with(|p| p.borrow_mut().take().is_some());
        KeyResult { handled }
    }
}

export_interactive!(AddImage);

#[cfg(test)]
mod tests {
    use super::pixel_size;

    #[test]
    fn reads_png_and_jpeg_sizes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0, 200]);
        assert_eq!(pixel_size(&png), Some((256, 200)));
        // SOI, an APP0 segment of 4 bytes, then SOF0 with height 300 and width 400.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8, 1, 44, 1, 144, 3,
        ];
        assert_eq!(pixel_size(&jpeg), Some((400, 300)));
        assert_eq!(pixel_size(b"GIF89a"), None);
    }
}
