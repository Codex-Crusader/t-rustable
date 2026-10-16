//! Spike 1: rendering speed.
//!
//!   spike-render-tiles bench [extra.pdf]   numbers without a window
//!   spike-render-tiles [file.pdf]          Tauri window, tiles over the `trtile` URI scheme

mod engine;

use engine::{PX_PER_PT, Request, TILE, encode_png, render_tile, tiles_for};
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::http::Response;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn stats(name: &str, mut v: Vec<f64>) -> serde_json::Value {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pick = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "  {name}: n={} mean={mean:.1} p50={:.1} p95={:.1} max={:.1}",
        v.len(),
        pick(0.5),
        pick(0.95),
        pick(1.0)
    );
    serde_json::json!({ "n": v.len(), "mean": mean, "p50": pick(0.5), "p95": pick(0.95), "max": pick(1.0) })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("bench") {
        bench(args.get(2).map(PathBuf::from));
    } else {
        gui(args.get(1).map(PathBuf::from));
    }
}

fn bench(extra: Option<PathBuf>) {
    let pdfium = engine::pdfium();
    let sample = engine::ensure_sample(&pdfium);
    let mut files = vec![("generated 50-page sample", sample)];
    if let Some(p) = extra {
        files.push(("local third-party PDF", p));
    }
    for (label, path) in files {
        let size_kb = std::fs::metadata(&path).unwrap().len() / 1024;
        let started = Instant::now();
        let doc = pdfium.load_pdf_from_file(&path, None).unwrap();
        let open_ms = ms(started);
        let pages = doc.pages();
        println!(
            "{label}: {} pages, {size_kb} KB, open {open_ms:.1} ms",
            pages.len()
        );

        // First page at 100 percent: open, render all tiles, encode them.
        let first = pages.get(0).unwrap();
        let (cols, rows) = tiles_for(first.width().value, first.height().value, PX_PER_PT);
        let t = Instant::now();
        for ty in 0..rows {
            for tx in 0..cols {
                let tile = render_tile(&first, PX_PER_PT, tx, ty).unwrap();
                encode_png(&tile, png::Compression::Fast);
            }
        }
        println!(
            "  first page (open + {} tiles + PNG): {:.1} ms",
            cols * rows,
            open_ms + ms(t)
        );

        for (zoom, scale) in [
            ("100%", PX_PER_PT),
            ("200% (or 100% on a 2x screen)", 2.0 * PX_PER_PT),
        ] {
            let mut page_ms = vec![];
            let mut tile_ms = vec![];
            let (mut fast_ms, mut fast_kb, mut def_ms, mut def_kb) =
                (vec![], vec![], vec![], vec![]);
            for (i, page) in pages.iter().take(50).enumerate() {
                let (cols, rows) = tiles_for(page.width().value, page.height().value, scale);
                let mut sum = 0.0;
                for ty in 0..rows {
                    for tx in 0..cols {
                        let tile = render_tile(&page, scale, tx, ty).unwrap();
                        sum += tile.render_ms;
                        tile_ms.push(tile.render_ms);
                        if i < 10 {
                            let (bytes, t) = encode_png(&tile, png::Compression::Fast);
                            fast_ms.push(t);
                            fast_kb.push(bytes.len() as f64 / 1024.0);
                            let (bytes, t) = encode_png(&tile, png::Compression::Balanced);
                            def_ms.push(t);
                            def_kb.push(bytes.len() as f64 / 1024.0);
                        }
                    }
                }
                page_ms.push(sum);
            }
            println!(" zoom {zoom}:");
            stats("page render ms (all tiles of a page)", page_ms);
            stats("tile render ms", tile_ms);
            stats("PNG encode ms per tile, Fast", fast_ms);
            stats("PNG KB per tile, Fast", fast_kb);
            stats("PNG encode ms per tile, Balanced", def_ms);
            stats("PNG KB per tile, Balanced", def_kb);
        }
    }
}

struct App {
    engine: Mutex<Sender<Request>>,
    tiles: Arc<Mutex<Vec<(f64, f64)>>>,
    file: String,
}

#[tauri::command]
fn doc_info(app: tauri::State<App>) -> serde_json::Value {
    let (tx, rx) = channel();
    app.engine.lock().unwrap().send(Request::Info(tx)).unwrap();
    let sizes = rx.recv().unwrap();
    serde_json::json!({ "pages": sizes, "pxPerPt": PX_PER_PT, "tile": TILE, "file": app.file })
}

#[tauri::command]
fn report(app: tauri::State<App>, handle: tauri::AppHandle, result: serde_json::Value) {
    let tiles = app.tiles.lock().unwrap().clone();
    println!(
        "GUI result: {}",
        serde_json::to_string_pretty(&result).unwrap()
    );
    println!("server side, per tile:");
    let render = stats("tile render ms", tiles.iter().map(|t| t.0).collect());
    let encode = stats("PNG encode ms (Fast)", tiles.iter().map(|t| t.1).collect());
    let all = serde_json::json!({ "gui": result, "tileRender": render, "pngEncode": encode });
    let name = format!("render-gui-{}.json", std::env::consts::OS);
    std::fs::write(
        engine::spikes_dir().join("out").join(name),
        serde_json::to_string_pretty(&all).unwrap(),
    )
    .unwrap();
    handle.exit(0);
}

fn gui(file: Option<PathBuf>) {
    let label = file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or("generated sample".into());
    let (engine_tx, _engine_thread) = engine::spawn(file);
    let tiles = Arc::new(Mutex::new(Vec::new()));

    let scheme_engine = Mutex::new(engine_tx.clone());
    let scheme_tiles = tiles.clone();
    tauri::Builder::default()
        .manage(App {
            engine: Mutex::new(engine_tx),
            tiles,
            file: label,
        })
        .invoke_handler(tauri::generate_handler![doc_info, report])
        .register_asynchronous_uri_scheme_protocol("trtile", move |_ctx, request, responder| {
            // Path: /<page>/<scale x 1000>/<tx>_<ty>.png
            let parts: Vec<String> = request
                .uri()
                .path()
                .trim_matches('/')
                .split('/')
                .map(String::from)
                .collect();
            let engine = scheme_engine.lock().unwrap().clone();
            let tiles = scheme_tiles.clone();
            std::thread::spawn(move || {
                let parsed = (|| {
                    let page: u16 = parts.first()?.parse().ok()?;
                    let scale = parts.get(1)?.parse::<f32>().ok()? / 1000.0;
                    let (tx, ty) = parts.get(2)?.trim_end_matches(".png").split_once('_')?;
                    Some((page, scale, tx.parse().ok()?, ty.parse().ok()?))
                })();
                let Some((page, scale, tx, ty)) = parsed else {
                    responder.respond(Response::builder().status(400).body(Vec::new()).unwrap());
                    return;
                };
                let (reply, rx) = channel();
                engine
                    .send(Request::Tile {
                        page,
                        scale,
                        tx,
                        ty,
                        reply,
                    })
                    .unwrap();
                match rx.recv().unwrap() {
                    Ok(tile) => {
                        let (png, encode_ms) = encode_png(&tile, png::Compression::Fast);
                        tiles.lock().unwrap().push((tile.render_ms, encode_ms));
                        responder.respond(
                            Response::builder()
                                .header("Content-Type", "image/png")
                                .header("Cache-Control", "no-store")
                                .body(png)
                                .unwrap(),
                        );
                    }
                    Err(e) => responder.respond(
                        Response::builder()
                            .status(500)
                            .body(e.into_bytes())
                            .unwrap(),
                    ),
                }
            });
        })
        .run(tauri::generate_context!())
        .expect("run Tauri");
}
