//! Progressive render through the raw PDFium C API, loaded with libloading.
//! pdfium-render 0.9 does not expose these functions.

use libloading::{Library, Symbol};
use std::ffi::{CString, c_int, c_ulong, c_void};
use std::path::Path;
use std::time::{Duration, Instant};

type Handle = *mut c_void;

#[repr(C)]
struct Pause {
    version: c_int,
    need_to_pause_now: extern "C" fn(*mut Pause) -> c_int,
    user: *mut c_void,
}

/// State that the pause callback reads and updates.
struct PauseState {
    calls: u32,
}

extern "C" fn pause_always(this: *mut Pause) -> c_int {
    // SAFETY: `user` points to a PauseState that outlives the render.
    let state = unsafe { &mut *((*this).user as *mut PauseState) };
    state.calls += 1;
    1
}

const RENDER_TOBECONTINUED: c_int = 1;
const RENDER_DONE: c_int = 2;
const FLAG_ANNOT: c_int = 0x01;

struct Api<'l> {
    load_document: Symbol<'l, unsafe extern "C" fn(*const i8, *const i8) -> Handle>,
    page_count: Symbol<'l, unsafe extern "C" fn(Handle) -> c_int>,
    load_page: Symbol<'l, unsafe extern "C" fn(Handle, c_int) -> Handle>,
    close_page: Symbol<'l, unsafe extern "C" fn(Handle)>,
    page_width: Symbol<'l, unsafe extern "C" fn(Handle) -> f32>,
    page_height: Symbol<'l, unsafe extern "C" fn(Handle) -> f32>,
    bitmap_create: Symbol<'l, unsafe extern "C" fn(c_int, c_int, c_int) -> Handle>,
    bitmap_fill:
        Symbol<'l, unsafe extern "C" fn(Handle, c_int, c_int, c_int, c_int, c_ulong) -> c_int>,
    bitmap_destroy: Symbol<'l, unsafe extern "C" fn(Handle)>,
    render:
        Symbol<'l, unsafe extern "C" fn(Handle, Handle, c_int, c_int, c_int, c_int, c_int, c_int)>,
    render_start: Symbol<
        'l,
        unsafe extern "C" fn(
            Handle,
            Handle,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            c_int,
            *mut Pause,
        ) -> c_int,
    >,
    render_continue: Symbol<'l, unsafe extern "C" fn(Handle, *mut Pause) -> c_int>,
    render_close: Symbol<'l, unsafe extern "C" fn(Handle)>,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

pub fn progressive(lib_dir: &Path, pdf: &Path, page_index: Option<c_int>) {
    let lib_path = lib_dir.join(libloading::library_filename("pdfium"));
    // SAFETY: loading the pinned PDFium build; the symbols below match its C API.
    unsafe {
        let lib = Library::new(lib_path).expect("load PDFium");
        let init: Symbol<unsafe extern "C" fn()> = lib.get(b"FPDF_InitLibrary").unwrap();
        init();
        let api = Api {
            load_document: lib.get(b"FPDF_LoadDocument").unwrap(),
            page_count: lib.get(b"FPDF_GetPageCount").unwrap(),
            load_page: lib.get(b"FPDF_LoadPage").unwrap(),
            close_page: lib.get(b"FPDF_ClosePage").unwrap(),
            page_width: lib.get(b"FPDF_GetPageWidthF").unwrap(),
            page_height: lib.get(b"FPDF_GetPageHeightF").unwrap(),
            bitmap_create: lib.get(b"FPDFBitmap_Create").unwrap(),
            bitmap_fill: lib.get(b"FPDFBitmap_FillRect").unwrap(),
            bitmap_destroy: lib.get(b"FPDFBitmap_Destroy").unwrap(),
            render: lib.get(b"FPDF_RenderPageBitmap").unwrap(),
            render_start: lib.get(b"FPDF_RenderPageBitmap_Start").unwrap(),
            render_continue: lib.get(b"FPDF_RenderPage_Continue").unwrap(),
            render_close: lib.get(b"FPDF_RenderPage_Close").unwrap(),
        };
        let path = CString::new(pdf.to_str().unwrap()).unwrap();
        let doc = (api.load_document)(path.as_ptr(), std::ptr::null());
        assert!(!doc.is_null(), "cannot open {}", pdf.display());

        // Pick the slowest page at 200 percent, so a pause has work to interrupt.
        let scale = 2.0;
        let pages = (api.page_count)(doc).min(60);
        let (mut slowest, mut slowest_ms) = (0, 0.0);
        for i in 0..pages {
            let t = full_render(&api, doc, i, scale);
            if t > slowest_ms {
                (slowest, slowest_ms) = (i, t);
            }
        }
        if let Some(i) = page_index {
            (slowest, slowest_ms) = (i, full_render(&api, doc, i, scale));
        }
        println!(
            "baseline: page {slowest} renders in {slowest_ms:.1} ms (200%, full render, document has {pages} pages)"
        );

        // Progressive render that pauses at every chance.
        let page = (api.load_page)(doc, slowest);
        let (bitmap, w, h) = new_bitmap(&api, page, scale);
        let mut state = PauseState { calls: 0 };
        let mut pause = Pause {
            version: 1,
            need_to_pause_now: pause_always,
            user: &mut state as *mut _ as *mut c_void,
        };
        let started = Instant::now();
        let mut status = (api.render_start)(bitmap, page, 0, 0, w, h, 0, FLAG_ANNOT, &mut pause);
        let mut steps = 1;
        let mut longest_step = ms(started.elapsed());
        while status == RENDER_TOBECONTINUED {
            let step = Instant::now();
            status = (api.render_continue)(page, &mut pause);
            longest_step = longest_step.max(ms(step.elapsed()));
            steps += 1;
        }
        (api.render_close)(page);
        let total = ms(started.elapsed());
        crate::report(
            "progressive render with pause callback",
            status == RENDER_DONE && steps > 1,
            format!(
                "{steps} steps, {} pause checks, total {total:.1} ms, longest step {longest_step:.1} ms (this is the worst cancel delay)",
                state.calls
            ),
        );
        (api.bitmap_destroy)(bitmap);

        // Cancel after the first pause, then render again to confirm PDFium is still usable.
        let (bitmap, w, h) = new_bitmap(&api, page, scale);
        let started = Instant::now();
        let status = (api.render_start)(bitmap, page, 0, 0, w, h, 0, FLAG_ANNOT, &mut pause);
        let cancel = Instant::now();
        (api.render_close)(page);
        let cancel_ms = ms(cancel.elapsed());
        let first_step_ms = ms(cancel.duration_since(started));
        (api.bitmap_destroy)(bitmap);
        let again = full_render(&api, doc, slowest, scale);
        crate::report(
            "cancel in the middle",
            status == RENDER_TOBECONTINUED,
            format!(
                "first step {first_step_ms:.1} ms, close took {cancel_ms:.2} ms, full render after cancel {again:.1} ms"
            ),
        );
        (api.close_page)(page);
    }
}

unsafe fn new_bitmap(api: &Api, page: Handle, scale: f32) -> (Handle, c_int, c_int) {
    unsafe {
        let w = ((api.page_width)(page) * scale) as c_int;
        let h = ((api.page_height)(page) * scale) as c_int;
        let bitmap = (api.bitmap_create)(w, h, 0);
        (api.bitmap_fill)(bitmap, 0, 0, w, h, 0xFFFF_FFFF);
        (bitmap, w, h)
    }
}

unsafe fn full_render(api: &Api, doc: Handle, index: c_int, scale: f32) -> f64 {
    unsafe {
        let page = (api.load_page)(doc, index);
        let (bitmap, w, h) = new_bitmap(api, page, scale);
        let started = Instant::now();
        (api.render)(bitmap, page, 0, 0, w, h, 0, FLAG_ANNOT);
        let t = ms(started.elapsed());
        (api.bitmap_destroy)(bitmap);
        (api.close_page)(page);
        t
    }
}
