//! M3 spike: can the core map object IDs to PDFium object indices across save and reload?
//! Also: image objects from a BGRA bitmap (alpha) and from an inline JPEG.
//!
//! Modes:
//!   ids <file.pdf>...   edit each page, then compare the object lists: live, reopened, reloaded
//!   image <file.jpg>    alpha bitmap and JPEG image objects, then save, reload, and render

use libloading::{Library, Symbol};
use std::ffi::{c_int, c_ulong, c_void};
use std::path::{Path, PathBuf};

type H = *mut c_void;

#[repr(C)]
struct FileWrite {
    version: c_int,
    write_block: extern "C" fn(*mut FileWrite, *const c_void, c_ulong) -> c_int,
}

/// `base` comes first, so PDFium's pointer is also a pointer to the Writer.
#[repr(C)]
struct Writer {
    base: FileWrite,
    out: Vec<u8>,
}

extern "C" fn write_block(this: *mut FileWrite, data: *const c_void, size: c_ulong) -> c_int {
    // SAFETY: `this` is the `base` field of a live Writer, and `data` has `size` bytes.
    unsafe {
        let w = &mut *(this as *mut Writer);
        w.out
            .extend_from_slice(std::slice::from_raw_parts(data as *const u8, size as usize));
    }
    1
}

#[repr(C)]
struct FileAccess {
    len: c_ulong,
    get_block: extern "C" fn(*mut c_void, c_ulong, *mut u8, c_ulong) -> c_int,
    param: *mut c_void,
}

extern "C" fn get_block(param: *mut c_void, pos: c_ulong, buf: *mut u8, size: c_ulong) -> c_int {
    // SAFETY: `param` is a live Vec<u8>. PDFium keeps pos + size inside its length.
    unsafe {
        let data = &*(param as *const Vec<u8>);
        let src = &data[pos as usize..(pos + size) as usize];
        std::ptr::copy_nonoverlapping(src.as_ptr(), buf, size as usize);
    }
    1
}

type F<'l, T> = Symbol<'l, T>;

struct Api<'l> {
    load_mem: F<'l, unsafe extern "C" fn(*const c_void, c_int, *const i8) -> H>,
    new_doc: F<'l, unsafe extern "C" fn() -> H>,
    close_doc: F<'l, unsafe extern "C" fn(H)>,
    page_count: F<'l, unsafe extern "C" fn(H) -> c_int>,
    new_page: F<'l, unsafe extern "C" fn(H, c_int, f64, f64) -> H>,
    load_page: F<'l, unsafe extern "C" fn(H, c_int) -> H>,
    close_page: F<'l, unsafe extern "C" fn(H)>,
    count_objects: F<'l, unsafe extern "C" fn(H) -> c_int>,
    get_object: F<'l, unsafe extern "C" fn(H, c_int) -> H>,
    obj_type: F<'l, unsafe extern "C" fn(H) -> c_int>,
    obj_bounds: F<'l, unsafe extern "C" fn(H, *mut f32, *mut f32, *mut f32, *mut f32) -> c_int>,
    insert_object: F<'l, unsafe extern "C" fn(H, H)>,
    remove_object: F<'l, unsafe extern "C" fn(H, H) -> c_int>,
    destroy_object: F<'l, unsafe extern "C" fn(H)>,
    new_rect: F<'l, unsafe extern "C" fn(f32, f32, f32, f32) -> H>,
    set_fill: F<'l, unsafe extern "C" fn(H, u32, u32, u32, u32) -> c_int>,
    draw_mode: F<'l, unsafe extern "C" fn(H, c_int, c_int) -> c_int>,
    generate: F<'l, unsafe extern "C" fn(H) -> c_int>,
    save: F<'l, unsafe extern "C" fn(H, *mut FileWrite, u32) -> c_int>,
    annot_count: F<'l, unsafe extern "C" fn(H) -> c_int>,
    get_annot: F<'l, unsafe extern "C" fn(H, c_int) -> H>,
    create_annot: F<'l, unsafe extern "C" fn(H, c_int) -> H>,
    close_annot: F<'l, unsafe extern "C" fn(H)>,
    annot_subtype: F<'l, unsafe extern "C" fn(H) -> c_int>,
    annot_get_rect: F<'l, unsafe extern "C" fn(H, *mut [f32; 4]) -> c_int>,
    annot_set_rect: F<'l, unsafe extern "C" fn(H, *const [f32; 4]) -> c_int>,
    new_image: F<'l, unsafe extern "C" fn(H) -> H>,
    image_set_bitmap: F<'l, unsafe extern "C" fn(*mut H, c_int, H, H) -> c_int>,
    image_load_jpeg: F<'l, unsafe extern "C" fn(*mut H, c_int, H, *mut FileAccess) -> c_int>,
    image_set_matrix: F<'l, unsafe extern "C" fn(H, f64, f64, f64, f64, f64, f64) -> c_int>,
    bitmap_create_ex: F<'l, unsafe extern "C" fn(c_int, c_int, c_int, *mut c_void, c_int) -> H>,
    bitmap_create: F<'l, unsafe extern "C" fn(c_int, c_int, c_int) -> H>,
    bitmap_fill: F<'l, unsafe extern "C" fn(H, c_int, c_int, c_int, c_int, c_ulong) -> c_int>,
    bitmap_buffer: F<'l, unsafe extern "C" fn(H) -> *mut u8>,
    bitmap_stride: F<'l, unsafe extern "C" fn(H) -> c_int>,
    bitmap_destroy: F<'l, unsafe extern "C" fn(H)>,
    render: F<'l, unsafe extern "C" fn(H, H, c_int, c_int, c_int, c_int, c_int, c_int)>,
    text_load: F<'l, unsafe extern "C" fn(H) -> H>,
    text_close: F<'l, unsafe extern "C" fn(H)>,
    text_count: F<'l, unsafe extern "C" fn(H) -> c_int>,
    text_get: F<'l, unsafe extern "C" fn(H, c_int, c_int, *mut u16) -> c_int>,
    page_width: F<'l, unsafe extern "C" fn(H) -> f32>,
    page_height: F<'l, unsafe extern "C" fn(H) -> f32>,
    text_obj_get: F<'l, unsafe extern "C" fn(H, H, *mut u16, c_ulong) -> c_ulong>,
}

impl<'l> Api<'l> {
    /// SAFETY: `lib` must be the pinned PDFium build. The types match its headers.
    unsafe fn load(lib: &'l Library) -> Self {
        unsafe {
            let init: F<unsafe extern "C" fn()> = lib.get(b"FPDF_InitLibrary").unwrap();
            init();
            Self {
                load_mem: lib.get(b"FPDF_LoadMemDocument").unwrap(),
                new_doc: lib.get(b"FPDF_CreateNewDocument").unwrap(),
                close_doc: lib.get(b"FPDF_CloseDocument").unwrap(),
                page_count: lib.get(b"FPDF_GetPageCount").unwrap(),
                new_page: lib.get(b"FPDFPage_New").unwrap(),
                load_page: lib.get(b"FPDF_LoadPage").unwrap(),
                close_page: lib.get(b"FPDF_ClosePage").unwrap(),
                count_objects: lib.get(b"FPDFPage_CountObjects").unwrap(),
                get_object: lib.get(b"FPDFPage_GetObject").unwrap(),
                obj_type: lib.get(b"FPDFPageObj_GetType").unwrap(),
                obj_bounds: lib.get(b"FPDFPageObj_GetBounds").unwrap(),
                insert_object: lib.get(b"FPDFPage_InsertObject").unwrap(),
                remove_object: lib.get(b"FPDFPage_RemoveObject").unwrap(),
                destroy_object: lib.get(b"FPDFPageObj_Destroy").unwrap(),
                new_rect: lib.get(b"FPDFPageObj_CreateNewRect").unwrap(),
                set_fill: lib.get(b"FPDFPageObj_SetFillColor").unwrap(),
                draw_mode: lib.get(b"FPDFPath_SetDrawMode").unwrap(),
                generate: lib.get(b"FPDFPage_GenerateContent").unwrap(),
                save: lib.get(b"FPDF_SaveAsCopy").unwrap(),
                annot_count: lib.get(b"FPDFPage_GetAnnotCount").unwrap(),
                get_annot: lib.get(b"FPDFPage_GetAnnot").unwrap(),
                create_annot: lib.get(b"FPDFPage_CreateAnnot").unwrap(),
                close_annot: lib.get(b"FPDFPage_CloseAnnot").unwrap(),
                annot_subtype: lib.get(b"FPDFAnnot_GetSubtype").unwrap(),
                annot_get_rect: lib.get(b"FPDFAnnot_GetRect").unwrap(),
                annot_set_rect: lib.get(b"FPDFAnnot_SetRect").unwrap(),
                new_image: lib.get(b"FPDFPageObj_NewImageObj").unwrap(),
                image_set_bitmap: lib.get(b"FPDFImageObj_SetBitmap").unwrap(),
                image_load_jpeg: lib.get(b"FPDFImageObj_LoadJpegFileInline").unwrap(),
                image_set_matrix: lib.get(b"FPDFImageObj_SetMatrix").unwrap(),
                bitmap_create_ex: lib.get(b"FPDFBitmap_CreateEx").unwrap(),
                bitmap_create: lib.get(b"FPDFBitmap_Create").unwrap(),
                bitmap_fill: lib.get(b"FPDFBitmap_FillRect").unwrap(),
                bitmap_buffer: lib.get(b"FPDFBitmap_GetBuffer").unwrap(),
                bitmap_stride: lib.get(b"FPDFBitmap_GetStride").unwrap(),
                bitmap_destroy: lib.get(b"FPDFBitmap_Destroy").unwrap(),
                render: lib.get(b"FPDF_RenderPageBitmap").unwrap(),
                text_load: lib.get(b"FPDFText_LoadPage").unwrap(),
                text_close: lib.get(b"FPDFText_ClosePage").unwrap(),
                text_count: lib.get(b"FPDFText_CountChars").unwrap(),
                text_get: lib.get(b"FPDFText_GetText").unwrap(),
                page_width: lib.get(b"FPDF_GetPageWidthF").unwrap(),
                page_height: lib.get(b"FPDF_GetPageHeightF").unwrap(),
                text_obj_get: lib.get(b"FPDFTextObj_GetText").unwrap(),
            }
        }
    }
}

const BGRA: c_int = 4;
const FPDF_NO_INCREMENTAL: u32 = 1 << 1;
const ANNOT_SQUARE: c_int = 5;
const FILL_WINDING: c_int = 2;

/// One page object or annotation: kind and bounds, rounded to 0.1 pt.
type Entry = (i32, [i32; 4]);

#[derive(Debug, PartialEq)]
struct PageSnap {
    objects: Vec<Entry>,
    annots: Vec<Entry>,
}

fn round(b: [f32; 4]) -> [i32; 4] {
    b.map(|v| (v * 10.0).round() as i32)
}

fn report(name: &str, ok: bool, detail: impl AsRef<str>) {
    let mark = if ok { "PASS" } else { "FAIL" };
    println!("[{mark}] {name}: {}", detail.as_ref());
}

fn spikes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn lib_path() -> PathBuf {
    let dir = if cfg!(windows) {
        "win-x64/bin"
    } else {
        "linux-x64/lib"
    };
    spikes_dir()
        .join(".pdfium")
        .join(dir)
        .join(libloading::library_filename("pdfium"))
}

unsafe fn snap_page(api: &Api, page: H) -> PageSnap {
    unsafe {
        let mut objects = vec![];
        for i in 0..(api.count_objects)(page) {
            let o = (api.get_object)(page, i);
            let (mut l, mut b, mut r, mut t) = (0f32, 0f32, 0f32, 0f32);
            (api.obj_bounds)(o, &mut l, &mut b, &mut r, &mut t);
            objects.push(((api.obj_type)(o), round([l, b, r, t])));
        }
        let mut annots = vec![];
        for i in 0..(api.annot_count)(page) {
            let a = (api.get_annot)(page, i);
            let mut rect = [0f32; 4];
            (api.annot_get_rect)(a, &mut rect);
            annots.push(((api.annot_subtype)(a), round(rect)));
            (api.close_annot)(a);
        }
        PageSnap { objects, annots }
    }
}

unsafe fn snap_doc(api: &Api, doc: H) -> Vec<PageSnap> {
    unsafe {
        (0..(api.page_count)(doc))
            .map(|i| {
                let page = (api.load_page)(doc, i);
                let s = snap_page(api, page);
                (api.close_page)(page);
                s
            })
            .collect()
    }
}

unsafe fn save(api: &Api, doc: H) -> Vec<u8> {
    unsafe {
        let mut w = Writer {
            base: FileWrite {
                version: 1,
                write_block,
            },
            out: vec![],
        };
        let ok = (api.save)(doc, &mut w.base, FPDF_NO_INCREMENTAL);
        assert!(ok != 0, "save failed");
        w.out
    }
}

unsafe fn load(api: &Api, bytes: &[u8]) -> H {
    unsafe {
        let doc = (api.load_mem)(bytes.as_ptr() as _, bytes.len() as c_int, std::ptr::null());
        assert!(!doc.is_null(), "load failed");
        doc
    }
}

/// Edits each page: remove the middle object, add a filled rect, add a Square annotation.
/// Returns the snapshot taken while the pages are still open.
unsafe fn edit_all(api: &Api, doc: H) -> Vec<PageSnap> {
    unsafe {
        let mut live = vec![];
        for i in 0..(api.page_count)(doc) {
            let page = (api.load_page)(doc, i);
            let n = (api.count_objects)(page);
            if n > 1 {
                let o = (api.get_object)(page, n / 2);
                (api.remove_object)(page, o);
                (api.destroy_object)(o);
            }
            let rect = (api.new_rect)(50.0, 60.0, 70.0, 30.0);
            (api.set_fill)(rect, 200, 30, 30, 255);
            (api.draw_mode)(rect, FILL_WINDING, 0);
            (api.insert_object)(page, rect);
            let a = (api.create_annot)(page, ANNOT_SQUARE);
            (api.annot_set_rect)(a, &[100.0, 100.0, 200.0, 150.0]);
            (api.close_annot)(a);
            (api.generate)(page);
            live.push(snap_page(api, page));
            (api.close_page)(page);
        }
        live
    }
}

fn first_diff(a: &[PageSnap], b: &[PageSnap]) -> String {
    if a.len() != b.len() {
        return format!("page count {} vs {}", a.len(), b.len());
    }
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x.objects.len() != y.objects.len() || x.annots.len() != y.annots.len() {
            return format!(
                "page {i}: objects {} vs {}, annots {} vs {}",
                x.objects.len(),
                y.objects.len(),
                x.annots.len(),
                y.annots.len()
            );
        }
        if let Some(j) = (0..x.objects.len()).find(|&j| x.objects[j] != y.objects[j]) {
            return format!(
                "page {i} object {j}: {:?} vs {:?}",
                x.objects[j], y.objects[j]
            );
        }
        if let Some(j) = (0..x.annots.len()).find(|&j| x.annots[j] != y.annots[j]) {
            return format!("page {i} annot {j}: {:?} vs {:?}", x.annots[j], y.annots[j]);
        }
    }
    "none".into()
}

fn ids(api: &Api, files: &[String]) {
    for path in files {
        let bytes = std::fs::read(path).expect("read input");
        // SAFETY: the documents and pages stay valid between load and close.
        unsafe {
            let doc = load(api, &bytes);
            let before = snap_doc(api, doc);
            let live = edit_all(api, doc);
            let reopened = snap_doc(api, doc);
            let saved = save(api, doc);
            (api.close_doc)(doc);
            let doc2 = load(api, &saved);
            let reloaded = snap_doc(api, doc2);
            let saved2 = save(api, doc2);
            (api.close_doc)(doc2);
            let doc3 = load(api, &saved2);
            let reloaded2 = snap_doc(api, doc3);
            (api.close_doc)(doc3);

            let objs: usize = before.iter().map(|p| p.objects.len()).sum();
            report(
                &format!("{path}: reopened page equals live page"),
                live == reopened,
                format!("first difference: {}", first_diff(&live, &reopened)),
            );
            report(
                &format!("{path}: reloaded document equals live document"),
                live == reloaded,
                format!(
                    "{} pages, {objs} objects before edit; first difference: {}",
                    before.len(),
                    first_diff(&live, &reloaded)
                ),
            );
            let kinds = |s: &[PageSnap]| -> Vec<(Vec<i32>, Vec<i32>)> {
                s.iter()
                    .map(|p| {
                        (
                            p.objects.iter().map(|o| o.0).collect(),
                            p.annots.iter().map(|a| a.0).collect(),
                        )
                    })
                    .collect()
            };
            let moved: Vec<String> = live
                .iter()
                .zip(&reloaded)
                .enumerate()
                .flat_map(|(i, (x, y))| {
                    x.objects
                        .iter()
                        .zip(&y.objects)
                        .enumerate()
                        .filter(|(_, (a, b))| a != b)
                        .map(move |(j, (a, b))| {
                            let d = (0..4).map(|k| (a.1[k] - b.1[k]).abs()).max().unwrap();
                            format!("p{i}o{j} kind {} by {:.1} pt", a.0, d as f32 / 10.0)
                        })
                })
                .collect();
            report(
                &format!("{path}: order and kinds equal after reload"),
                kinds(&live) == kinds(&reloaded),
                format!("{} objects with other bounds: {moved:?}", moved.len()),
            );
            report(
                &format!("{path}: second save and reload is stable"),
                reloaded == reloaded2,
                format!("first difference: {}", first_diff(&reloaded, &reloaded2)),
            );
        }
    }
}

unsafe fn pixel(api: &Api, page: H, w: c_int, h: c_int, x: c_int, y: c_int) -> [u8; 4] {
    unsafe {
        let bmp = (api.bitmap_create)(w, h, 1);
        (api.bitmap_fill)(bmp, 0, 0, w, h, 0xFFFF_FFFF);
        (api.render)(bmp, page, 0, 0, w, h, 0, 0);
        let stride = (api.bitmap_stride)(bmp) as usize;
        let p = (api.bitmap_buffer)(bmp).add(y as usize * stride + x as usize * 4);
        let px = [*p, *p.add(1), *p.add(2), *p.add(3)];
        (api.bitmap_destroy)(bmp);
        px
    }
}

fn image(api: &Api, jpeg_path: &str) {
    let jpeg = std::fs::read(jpeg_path).expect("read jpeg");
    // SAFETY: the bitmap buffer outlives the bitmap; the JPEG Vec outlives the load call.
    unsafe {
        let doc = (api.new_doc)();
        let mut page = (api.new_page)(doc, 0, 200.0, 200.0);

        // 100 x 100 BGRA: left half transparent, right half opaque blue.
        let mut buf = vec![0u8; 100 * 100 * 4];
        for y in 0..100 {
            for x in 50..100 {
                let i = (y * 100 + x) * 4;
                buf[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let bmp = (api.bitmap_create_ex)(100, 100, BGRA, buf.as_mut_ptr() as _, 400);
        let img = (api.new_image)(doc);
        let ok_bitmap = (api.image_set_bitmap)(&mut page, 1, img, bmp);
        (api.image_set_matrix)(img, 100.0, 0.0, 0.0, 100.0, 0.0, 100.0);
        (api.insert_object)(page, img);
        (api.bitmap_destroy)(bmp);

        let mut access = FileAccess {
            len: jpeg.len() as c_ulong,
            get_block,
            param: &jpeg as *const Vec<u8> as *mut c_void,
        };
        let jimg = (api.new_image)(doc);
        let ok_jpeg = (api.image_load_jpeg)(&mut page, 1, jimg, &mut access);
        (api.image_set_matrix)(jimg, 100.0, 0.0, 0.0, 100.0, 100.0, 0.0);
        (api.insert_object)(page, jimg);
        (api.generate)(page);

        // Render at 1 px per point. The page y axis goes up, the bitmap y axis goes down.
        let live_clear = pixel(api, page, 200, 200, 25, 50);
        let live_blue = pixel(api, page, 200, 200, 75, 50);
        (api.close_page)(page);
        let saved = save(api, doc);
        (api.close_doc)(doc);

        let doc2 = load(api, &saved);
        let page2 = (api.load_page)(doc2, 0);
        let snap = snap_page(api, page2);
        let clear = pixel(api, page2, 200, 200, 25, 50);
        let blue = pixel(api, page2, 200, 200, 75, 50);
        let jpeg_px = pixel(api, page2, 200, 200, 150, 150);
        (api.close_page)(page2);
        (api.close_doc)(doc2);

        report(
            "SetBitmap with BGRA alpha keeps transparency after save and reload",
            ok_bitmap != 0 && clear == [255, 255, 255, 255] && blue[0] > 200 && blue[2] < 50,
            format!(
                "live clear {live_clear:?} blue {live_blue:?}; reloaded clear {clear:?} blue {blue:?}"
            ),
        );
        report(
            "LoadJpegFileInline",
            ok_jpeg != 0 && snap.objects.len() == 2 && snap.objects.iter().all(|o| o.0 == 3),
            format!(
                "objects after reload {:?}, JPEG pixel {jpeg_px:?}, file {} KB",
                snap.objects,
                saved.len() / 1024
            ),
        );
    }
}

/// Page 0 only: object count before, after GenerateContent with no edit, and after reopen.
fn probe(api: &Api, path: &str) {
    let bytes = std::fs::read(path).expect("read input");
    // SAFETY: as in `ids`.
    unsafe {
        let doc = load(api, &bytes);
        let page = (api.load_page)(doc, 0);
        let before = snap_page(api, page);
        let ok = (api.generate)(page);
        let after_gen = snap_page(api, page);
        (api.close_page)(page);
        let page = (api.load_page)(doc, 0);
        let reopened = snap_page(api, page);
        (api.close_page)(page);
        let count = |s: &PageSnap, k: i32| s.objects.iter().filter(|o| o.0 == k).count();
        for (name, s) in [
            ("before", &before),
            ("after generate", &after_gen),
            ("reopened", &reopened),
        ] {
            println!(
                "{name}: {} objects (text {}, path {}, image {}, shading {}, form {})",
                s.objects.len(),
                count(s, 1),
                count(s, 2),
                count(s, 3),
                count(s, 4),
                count(s, 5)
            );
        }
        println!("generate returned {ok}");

        // One small edit: add a rect in the bottom left corner. Compare text and pixels.
        let page = (api.load_page)(doc, 0);
        let (text0, px0) = (page_text(api, page), page_pixels(api, page));
        let rect = (api.new_rect)(0.0, 0.0, 10.0, 10.0);
        (api.set_fill)(rect, 0, 0, 0, 255);
        (api.draw_mode)(rect, FILL_WINDING, 0);
        (api.insert_object)(page, rect);
        (api.generate)(page);
        let live = snap_page(api, page);
        (api.close_page)(page);
        let page = (api.load_page)(doc, 0);
        let reopened = snap_page(api, page);
        let (text1, px1) = (page_text(api, page), page_pixels(api, page));
        (api.close_page)(page);
        let changed = px0.iter().zip(&px1).filter(|(a, b)| a != b).count();
        println!(
            "after adding one rect: live {} objects, reopened {} objects; text equal: {} ({} vs {} chars); pixels changed: {changed} of {} (the rect is 10 x 10 pt)",
            live.objects.len(),
            reopened.objects.len(),
            text0 == text1,
            text0.chars().count(),
            text1.chars().count(),
            px0.len()
        );

        // Remove one text object near the middle. Compare again.
        let page = (api.load_page)(doc, 0);
        let (text0, px0) = (page_text(api, page), page_pixels(api, page));
        let n = (api.count_objects)(page);
        let idx = (0..n)
            .map(|i| (n / 2 + i) % n)
            .find(|&i| (api.obj_type)((api.get_object)(page, i)) == 1)
            .unwrap();
        let o = (api.get_object)(page, idx);
        (api.remove_object)(page, o);
        (api.destroy_object)(o);
        let ok = (api.generate)(page);
        let live = snap_page(api, page);
        let live_text = page_text(api, page);
        (api.close_page)(page);
        let page = (api.load_page)(doc, 0);
        let reopened = snap_page(api, page);
        let (text1, px1) = (page_text(api, page), page_pixels(api, page));
        (api.close_page)(page);
        let changed = px0.iter().zip(&px1).filter(|(a, b)| a != b).count();
        println!(
            "after removing text object {idx}: generate {ok}; live {} objects, reopened {} objects; chars {} before, {} live, {} reopened; pixels changed: {changed}",
            live.objects.len(),
            reopened.objects.len(),
            text0.chars().count(),
            live_text.chars().count(),
            text1.chars().count(),
        );
        (api.close_doc)(doc);
    }
}

/// The check that the engine will run: object kinds in order, and the text of each text object.
unsafe fn page_fingerprint(api: &Api, page: H) -> Vec<(i32, String)> {
    unsafe {
        let tp = (api.text_load)(page);
        let out = (0..(api.count_objects)(page))
            .map(|i| {
                let o = (api.get_object)(page, i);
                let kind = (api.obj_type)(o);
                let text = if kind == 1 {
                    let len = (api.text_obj_get)(o, tp, std::ptr::null_mut(), 0);
                    let mut buf = vec![0u16; len as usize / 2];
                    (api.text_obj_get)(o, tp, buf.as_mut_ptr(), len);
                    String::from_utf16_lossy(&buf[..buf.len().saturating_sub(1)])
                } else {
                    String::new()
                };
                (kind, text)
            })
            .collect();
        (api.text_close)(tp);
        out
    }
}

/// Removes the middle object of every page, generates content, reopens the page, and
/// compares the two checks: the per-object fingerprint and the page text.
fn verify(api: &Api, files: &[String]) {
    let (mut pages, mut fp_diff, mut text_diff) = (0, 0, 0);
    for path in files {
        let bytes = std::fs::read(path).expect("read input");
        // SAFETY: as in `ids`.
        unsafe {
            let doc = load(api, &bytes);
            for i in 0..(api.page_count)(doc) {
                let page = (api.load_page)(doc, i);
                let n = (api.count_objects)(page);
                if n > 1 {
                    let o = (api.get_object)(page, n / 2);
                    (api.remove_object)(page, o);
                    (api.destroy_object)(o);
                }
                (api.generate)(page);
                let (fp0, t0) = (page_fingerprint(api, page), page_text(api, page));
                (api.close_page)(page);
                let page = (api.load_page)(doc, i);
                let (fp1, t1) = (page_fingerprint(api, page), page_text(api, page));
                (api.close_page)(page);
                pages += 1;
                if fp0 != fp1 {
                    let j = (0..fp0.len().min(fp1.len())).find(|&j| fp0[j] != fp1[j]);
                    let show = |fp: &[(i32, String)], j: Option<usize>| {
                        j.map(|j| (fp[j].0, fp[j].1.chars().count()))
                    };
                    eprintln!(
                        "  file {} page {i}: {} vs {} objects, first diff at {j:?}: {:?} vs {:?} (kind, text length)",
                        files.iter().position(|f| f == path).unwrap(),
                        fp0.len(),
                        fp1.len(),
                        show(&fp0, j),
                        show(&fp1, j)
                    );
                }
                fp_diff += (fp0 != fp1) as u32;
                text_diff += (t0 != t1) as u32;
            }
            (api.close_doc)(doc);
        }
    }
    println!(
        "{} files, {pages} pages edited: fingerprint differs on {fp_diff} pages, page text differs on {text_diff} pages",
        files.len()
    );
}

unsafe fn page_text(api: &Api, page: H) -> String {
    unsafe {
        let tp = (api.text_load)(page);
        let n = (api.text_count)(tp);
        let mut buf = vec![0u16; n as usize + 1];
        (api.text_get)(tp, 0, n, buf.as_mut_ptr());
        (api.text_close)(tp);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

/// Renders at 1 px per point and returns the BGRA pixels.
unsafe fn page_pixels(api: &Api, page: H) -> Vec<[u8; 4]> {
    unsafe {
        let (w, h) = (
            (api.page_width)(page) as c_int,
            (api.page_height)(page) as c_int,
        );
        let bmp = (api.bitmap_create)(w, h, 1);
        (api.bitmap_fill)(bmp, 0, 0, w, h, 0xFFFF_FFFF);
        (api.render)(bmp, page, 0, 0, w, h, 0, 0);
        let stride = (api.bitmap_stride)(bmp) as usize;
        let buf = (api.bitmap_buffer)(bmp);
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let p = buf.add(y * stride + x * 4);
                out.push([*p, *p.add(1), *p.add(2), *p.add(3)]);
            }
        }
        (api.bitmap_destroy)(bmp);
        out
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // SAFETY: the pinned PDFium build.
    let lib = unsafe { Library::new(lib_path()).expect("load PDFium") };
    let api = unsafe { Api::load(&lib) };
    match args.get(1).map(String::as_str) {
        Some("ids") => ids(&api, &args[2..]),
        Some("image") => image(&api, &args[2]),
        Some("probe") => probe(&api, &args[2]),
        Some("verify") => verify(&api, &args[2..]),
        _ => eprintln!("usage: ids <file.pdf>... | image <file.jpg>"),
    }
}
