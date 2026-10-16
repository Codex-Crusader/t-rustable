//! Engine thread: owns PDFium, renders 512 px tiles. PNG encoding runs outside it.

use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::Instant;

pub const TILE: u32 = 512;
/// CSS pixels per PDF point at 100 percent zoom (96 dpi / 72).
pub const PX_PER_PT: f32 = 96.0 / 72.0;

pub fn spikes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

pub fn pdfium() -> Pdfium {
    let platform = if cfg!(windows) { "win-x64/bin" } else { "linux-x64/lib" };
    let lib = Pdfium::pdfium_platform_library_name_at_path(&spikes_dir().join(".pdfium").join(platform));
    Pdfium::new(Pdfium::bind_to_library(lib).expect("load PDFium"))
}

/// Raw BGRA pixels of one tile.
pub struct Tile {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub render_ms: f64,
}

pub enum Request {
    Info(Sender<Vec<(f32, f32)>>),
    Tile { page: u16, scale: f32, tx: u32, ty: u32, reply: Sender<Result<Tile, String>> },
}

/// `path` None means the generated sample. PDFium can load only once per process,
/// so the engine thread makes the sample itself.
pub fn spawn(path: Option<PathBuf>) -> (Sender<Request>, JoinHandle<()>) {
    let (tx, rx) = channel();
    let handle = std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || run(path, rx))
        .unwrap();
    (tx, handle)
}

fn run(path: Option<PathBuf>, rx: Receiver<Request>) {
    let pdfium = pdfium();
    let path = path.unwrap_or_else(|| ensure_sample(&pdfium));
    let doc = pdfium.load_pdf_from_file(&path, None).expect("open PDF");
    for request in rx {
        match request {
            Request::Info(reply) => {
                let sizes = doc.pages().iter().map(|p| (p.width().value, p.height().value)).collect();
                let _ = reply.send(sizes);
            }
            Request::Tile { page, scale, tx, ty, reply } => {
                let result = doc
                    .pages()
                    .get(page as i32)
                    .map_err(|e| e.to_string())
                    .and_then(|p| render_tile(&p, scale, tx, ty).map_err(|e| e.to_string()));
                let _ = reply.send(result);
            }
        }
    }
}

/// Renders the tile at (tx, ty) of a page rendered at `scale` device pixels per point.
pub fn render_tile(page: &PdfPage, scale: f32, tx: u32, ty: u32) -> Result<Tile, PdfiumError> {
    let started = Instant::now();
    let full_w = (page.width().value * scale).round() as u32;
    let full_h = (page.height().value * scale).round() as u32;
    let width = TILE.min(full_w.saturating_sub(tx * TILE)).max(1);
    let height = TILE.min(full_h.saturating_sub(ty * TILE)).max(1);
    let mut bitmap = PdfBitmap::empty(width as i32, height as i32, PdfBitmapFormat::BGRA)?; // cleared to white by the config
    let config = PdfRenderConfig::new()
        .set_fixed_size(full_w as i32, full_h as i32)
        .set_origin(-((tx * TILE) as i32), -((ty * TILE) as i32));
    page.render_into_bitmap_with_config(&mut bitmap, &config)?;
    let bgra = bitmap.as_raw_bytes();
    Ok(Tile { width, height, bgra, render_ms: started.elapsed().as_secs_f64() * 1000.0 })
}

pub fn tiles_for(page_w: f32, page_h: f32, scale: f32) -> (u32, u32) {
    let w = (page_w * scale).round() as u32;
    let h = (page_h * scale).round() as u32;
    (w.div_ceil(TILE), h.div_ceil(TILE))
}

/// Encodes BGRA pixels as an RGB PNG. Returns the bytes and the time in ms.
pub fn encode_png(tile: &Tile, compression: png::Compression) -> (Vec<u8>, f64) {
    let started = Instant::now();
    let mut rgb = Vec::with_capacity((tile.width * tile.height * 3) as usize);
    for px in tile.bgra.chunks_exact(4) {
        rgb.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, tile.width, tile.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgb).unwrap();
    }
    (out, started.elapsed().as_secs_f64() * 1000.0)
}

/// Writes out/sample-50.pdf: 50 pages with text, vector shapes, and one image each.
pub fn ensure_sample(pdfium: &Pdfium) -> PathBuf {
    let path = spikes_dir().join("out/sample-50.pdf");
    if path.exists() {
        return path;
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let font_bytes = std::fs::read(spikes_dir().join("assets/NotoSans-Regular.ttf")).unwrap();
    let mut doc = pdfium.create_new_pdf().unwrap();
    let font = doc.fonts_mut().load_true_type_from_bytes(&font_bytes, true).unwrap();
    let words = "The quick brown fox jumps over the lazy dog while the engine renders tiles";
    for n in 0..50u32 {
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        let objects = page.objects_mut();
        objects
            .create_text_object(PdfPoints::new(56.0), PdfPoints::new(790.0), format!("Page {}", n + 1), font, PdfPoints::new(20.0))
            .unwrap();
        for line in 0..40 {
            let y = 760.0 - line as f32 * 12.0;
            if (12..24).contains(&line) {
                continue;
            }
            objects
                .create_text_object(PdfPoints::new(56.0), PdfPoints::new(y), words, font, PdfPoints::new(10.0))
                .unwrap();
        }
        for i in 0..60u32 {
            let (x, y) = (300.0 + (i % 10) as f32 * 22.0, 470.0 + (i / 10) as f32 * 22.0);
            let color = PdfColor::new((i * 4 + n) as u8, 90, (200 - i * 3) as u8, 255);
            objects
                .create_path_object_circle_at(PdfPoints::new(x), PdfPoints::new(y), PdfPoints::new(9.0), None, None, Some(color))
                .unwrap();
        }
        let img = image::RgbImage::from_fn(400, 300, |x, y| image::Rgb([(x / 2) as u8, (y * 255 / 300) as u8, ((x + y + n) % 256) as u8]));
        objects
            .create_image_object(
                PdfPoints::new(56.0),
                PdfPoints::new(470.0),
                &image::DynamicImage::ImageRgb8(img),
                Some(PdfPoints::new(220.0)),
                None,
            )
            .unwrap();
        page.regenerate_content().unwrap();
    }
    doc.save_to_file(&path).unwrap();
    path
}
