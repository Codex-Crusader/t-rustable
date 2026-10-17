//! Safe wrappers over the PDFium C API. This is the only module with unsafe code (ADR 0013).
//! All values here live on the engine thread. None of them is `Send`.

mod ffi;

use crate::protocol::{
    AnnotationInfo, Bitmap, CharInfo, Clip, EngineError, FieldKind, FormField, ImageData, Metadata,
    ObjectInfo, ObjectKind, PageInfo, PathSegment, Point, Rect, Rgba, Stroke, TextRun,
};
use ffi::{Fns, Handle, WChar};
use libloading::Library;
use std::collections::HashMap;
use std::ffi::{CString, c_int, c_ulong, c_void};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// PDFium state is global to the process, so only one `Pdfium` can exist at a time.
static LOADED: AtomicBool = AtomicBool::new(false);

/// The loaded PDFium library. Init on load, destroy on drop.
pub struct Pdfium {
    f: Fns,
    _lib: Library,
}

impl Pdfium {
    pub fn load(path: &Path) -> Result<Rc<Self>, String> {
        if LOADED.swap(true, Ordering::AcqRel) {
            return Err("another engine is running in this process".into());
        }
        // SAFETY: the path names the pinned non-V8 PDFium build (ADR 0001). Its C API
        // matches the declarations in `ffi`.
        let loaded = unsafe {
            Library::new(path)
                .map_err(|e| format!("cannot load {}: {e}", path.display()))
                .and_then(|lib| match Fns::load(&lib) {
                    Ok(f) => Ok((lib, f)),
                    Err(e) => Err(format!("missing PDFium function: {e}")),
                })
        };
        match loaded {
            Ok((lib, f)) => {
                // SAFETY: the guard above makes this the only init in the process.
                unsafe { (f.init_library)() };
                Ok(Rc::new(Self { f, _lib: lib }))
            }
            Err(e) => {
                LOADED.store(false, Ordering::Release);
                Err(e)
            }
        }
    }

    /// Shuts PDFium down and starts it again. Close all documents first.
    pub fn restart(&self) {
        // SAFETY: M1 verified destroy and init in one process (docs/spikes/m1-report.md).
        unsafe {
            (self.f.destroy_library)();
            (self.f.init_library)();
        }
    }
}

impl Drop for Pdfium {
    fn drop(&mut self) {
        // SAFETY: init ran in `load`. Documents hold an Rc, so none is open now.
        unsafe { (self.f.destroy_library)() }
        LOADED.store(false, Ordering::Release);
    }
}

fn wide(s: &str) -> Vec<WChar> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Calls a PDFium getter that fills a UTF-16 buffer and returns its size in bytes,
/// including the terminating NUL. Returns None for an empty value.
fn read_wide(get: impl Fn(*mut WChar, c_ulong) -> c_ulong) -> Option<String> {
    let bytes = get(std::ptr::null_mut(), 0) as usize;
    if bytes <= 2 {
        return None;
    }
    let mut buf = vec![0u16; bytes / 2];
    get(buf.as_mut_ptr(), bytes as c_ulong);
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

fn rgba_args(c: Rgba) -> (u32, u32, u32, u32) {
    (c.r as u32, c.g as u32, c.b as u32, c.a as u32)
}

fn check(ok: ffi::Bool, what: &str) -> Result<(), EngineError> {
    if ok != 0 {
        Ok(())
    } else {
        Err(EngineError::Internal(format!("PDFium: {what} failed")))
    }
}

/// A font that `Document::font` loaded. Valid while its document is open.
#[derive(Clone, Copy)]
pub struct FontRef(Handle);

pub struct Document {
    p: Rc<Pdfium>,
    h: Handle,
    fonts: Vec<(Arc<[u8]>, Handle)>,
    form: Option<(Handle, Box<ffi::FormFillInfo>)>,
    // PDFium reads from this buffer while the document is open.
    _bytes: Arc<[u8]>,
}

impl Document {
    pub fn open(
        p: &Rc<Pdfium>,
        bytes: Arc<[u8]>,
        password: Option<&str>,
    ) -> Result<Self, EngineError> {
        let password = password.map(CString::new).transpose().map_err(|_| {
            EngineError::InvalidArgument("password contains a NUL character".into())
        })?;
        let pw = password.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
        // SAFETY: `bytes` stays alive in the returned Document while PDFium uses it.
        let h =
            unsafe { (p.f.load_mem_document)(bytes.as_ptr() as *const c_void, bytes.len(), pw) };
        if h.is_null() {
            // SAFETY: no arguments.
            let code = unsafe { (p.f.get_last_error)() };
            return Err(if code == ffi::FPDF_ERR_PASSWORD {
                EngineError::Password
            } else {
                EngineError::BadFile(format!("PDFium error code {code}"))
            });
        }
        Ok(Self {
            p: p.clone(),
            h,
            fonts: Vec::new(),
            form: None,
            _bytes: bytes,
        })
    }

    fn f(&self) -> &Fns {
        &self.p.f
    }

    pub fn page_count(&self) -> usize {
        // SAFETY: `h` is an open document.
        unsafe { (self.f().get_page_count)(self.h).max(0) as usize }
    }

    pub fn signature_count(&self) -> usize {
        // SAFETY: `h` is an open document.
        unsafe { (self.f().get_signature_count)(self.h).max(0) as usize }
    }

    fn meta(&self, tag: &str) -> Option<String> {
        let tag = CString::new(tag).unwrap();
        // SAFETY: the buffer has `len` bytes.
        read_wide(|buf, len| unsafe {
            (self.f().get_meta_text)(self.h, tag.as_ptr(), buf as *mut c_void, len)
        })
    }

    pub fn metadata(&self) -> Metadata {
        Metadata {
            title: self.meta("Title"),
            author: self.meta("Author"),
            subject: self.meta("Subject"),
            keywords: self.meta("Keywords"),
            creator: self.meta("Creator"),
            producer: self.meta("Producer"),
        }
    }

    pub fn page_label(&self, index: usize) -> Option<String> {
        // SAFETY: the buffer has `len` bytes. PDFium checks the index.
        read_wide(|buf, len| unsafe {
            (self.f().get_page_label)(self.h, index as c_int, buf as *mut c_void, len)
        })
    }

    fn check_page(&self, index: usize) -> Result<(), EngineError> {
        if index < self.page_count() {
            Ok(())
        } else {
            Err(EngineError::InvalidArgument(format!(
                "page index {index} is out of range"
            )))
        }
    }

    /// Loads a page. The page closes when the value drops.
    pub fn page(&self, index: usize) -> Result<Page<'_>, EngineError> {
        self.check_page(index)?;
        // SAFETY: the index is in range.
        let h = unsafe { (self.f().load_page)(self.h, index as c_int) };
        if h.is_null() {
            return Err(EngineError::Internal(format!(
                "PDFium cannot load page {index}"
            )));
        }
        Ok(Page {
            doc: self,
            h,
            index,
        })
    }

    pub fn delete_page(&mut self, index: usize) -> Result<(), EngineError> {
        self.check_page(index)?;
        // SAFETY: the index is in range.
        unsafe { (self.f().page_delete)(self.h, index as c_int) };
        Ok(())
    }

    pub fn move_page(&mut self, index: usize, to: usize) -> Result<(), EngineError> {
        self.check_page(index)?;
        self.check_page(to)?;
        let indices = [index as c_int];
        // SAFETY: one valid index, and `to` is in range.
        check(
            unsafe { (self.f().move_pages)(self.h, indices.as_ptr(), 1, to as c_int) },
            "move page",
        )
    }

    pub fn insert_blank_page(
        &mut self,
        at: usize,
        width: f64,
        height: f64,
    ) -> Result<(), EngineError> {
        if at > self.page_count() {
            return Err(EngineError::InvalidArgument(format!(
                "page index {at} is out of range"
            )));
        }
        // SAFETY: the index is in range. The new page closes at once.
        unsafe {
            let page = (self.f().page_new)(self.h, at as c_int, width, height);
            if page.is_null() {
                return Err(EngineError::Internal("PDFium cannot make a page".into()));
            }
            (self.f().close_page)(page);
        }
        Ok(())
    }

    /// Loads a TrueType font as a CID font, once per document and font file.
    pub fn font(&mut self, data: &Arc<[u8]>) -> Result<FontRef, EngineError> {
        if let Some((_, h)) = self
            .fonts
            .iter()
            .find(|(d, _)| Arc::ptr_eq(d, data) || **d == **data)
        {
            return Ok(FontRef(*h));
        }
        // SAFETY: PDFium copies the font data.
        let h = unsafe {
            (self.f().text_load_font)(
                self.h,
                data.as_ptr(),
                data.len() as u32,
                ffi::FPDF_FONT_TRUETYPE,
                1,
            )
        };
        if h.is_null() {
            return Err(EngineError::InvalidArgument(
                "PDFium cannot load the font".into(),
            ));
        }
        self.fonts.push((data.clone(), h));
        Ok(FontRef(h))
    }

    pub fn save(&self) -> Result<Vec<u8>, EngineError> {
        #[repr(C)]
        struct Writer {
            base: ffi::FileWrite,
            out: Vec<u8>,
        }
        extern "C" fn write_block(
            this: *mut ffi::FileWrite,
            data: *const c_void,
            size: c_ulong,
        ) -> c_int {
            // SAFETY: `this` is the first field of a live Writer, and `data` has `size` bytes.
            unsafe {
                let w = &mut *(this as *mut Writer);
                w.out.extend_from_slice(std::slice::from_raw_parts(
                    data as *const u8,
                    size as usize,
                ));
            }
            1
        }
        let mut w = Writer {
            base: ffi::FileWrite {
                version: 1,
                write_block,
            },
            out: Vec::new(),
        };
        // SAFETY: the writer lives until the call returns.
        let ok = unsafe { (self.f().save_as_copy)(self.h, &mut w.base, ffi::FPDF_NO_INCREMENTAL) };
        check(ok, "save")?;
        Ok(w.out)
    }

    fn form_handle(&mut self) -> Handle {
        if let Some((h, _)) = &self.form {
            return *h;
        }
        let mut info = Box::new(ffi::FormFillInfo {
            version: 1,
            rest: [0; 128],
        });
        // SAFETY: the info box lives until `form_exit` in Drop. All callbacks are null.
        let h = unsafe { (self.p.f.form_init)(self.h, &mut *info) };
        self.form = Some((h, info));
        h
    }

    pub fn form_fields(&mut self) -> Result<Vec<FormField>, EngineError> {
        const WIDGET: c_int = 20;
        let form = self.form_handle();
        if form.is_null() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for index in 0..self.page_count() {
            let page = self.page(index)?;
            for (a, annot) in page.annot_handles() {
                let f = page.f();
                // SAFETY: `annot` is open until `annot_close` below.
                unsafe {
                    if (f.annot_get_subtype)(annot) == WIDGET {
                        let kind = match (f.annot_get_form_field_type)(form, annot) {
                            1 => FieldKind::PushButton,
                            2 => FieldKind::Checkbox,
                            3 => FieldKind::Radio,
                            4 => FieldKind::ComboBox,
                            5 => FieldKind::ListBox,
                            6 => FieldKind::Text,
                            7 => FieldKind::Signature,
                            _ => FieldKind::Unknown,
                        };
                        let flags = (f.annot_get_form_field_flags)(form, annot);
                        out.push(FormField {
                            page: index,
                            annotation: a,
                            name: read_wide(|b, l| {
                                (f.annot_get_form_field_name)(form, annot, b, l)
                            })
                            .unwrap_or_default(),
                            kind,
                            value: read_wide(|b, l| {
                                (f.annot_get_form_field_value)(form, annot, b, l)
                            }),
                            bounds: annot_rect(f, annot),
                            read_only: flags & ffi::FPDF_FORMFLAG_READONLY != 0,
                        });
                    }
                    (f.annot_close)(annot);
                }
            }
        }
        Ok(out)
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: fonts and the form handle belong to this document. Close them first.
        unsafe {
            if let Some((h, _)) = self.form.take() {
                (self.p.f.form_exit)(h);
            }
            for (_, h) in self.fonts.drain(..) {
                (self.p.f.font_close)(h);
            }
            (self.p.f.close_document)(self.h);
        }
    }
}

/// SAFETY: `annot` must be an open annotation.
unsafe fn annot_rect(f: &Fns, annot: Handle) -> Rect {
    let mut r = ffi::RectF::default();
    unsafe { (f.annot_get_rect)(annot, &mut r) };
    let (b, t) = (r.bottom.min(r.top), r.bottom.max(r.top));
    Rect {
        left: r.left as f64,
        bottom: b as f64,
        right: r.right as f64,
        top: t as f64,
    }
}

fn annot_subtype_name(code: c_int) -> &'static str {
    const NAMES: [&str; 29] = [
        "Unknown",
        "Text",
        "Link",
        "FreeText",
        "Line",
        "Square",
        "Circle",
        "Polygon",
        "PolyLine",
        "Highlight",
        "Underline",
        "Squiggly",
        "StrikeOut",
        "Stamp",
        "Caret",
        "Ink",
        "Popup",
        "FileAttachment",
        "Sound",
        "Movie",
        "Widget",
        "Screen",
        "PrinterMark",
        "TrapNet",
        "Watermark",
        "3D",
        "RichMedia",
        "XFAWidget",
        "Redact",
    ];
    NAMES.get(code as usize).copied().unwrap_or("Unknown")
}

fn object_kind(code: c_int) -> ObjectKind {
    match code {
        1 => ObjectKind::Text,
        2 => ObjectKind::Path,
        3 => ObjectKind::Image,
        4 => ObjectKind::Shading,
        5 => ObjectKind::Form,
        _ => ObjectKind::Unknown,
    }
}

/// An open page. It closes on drop. Call `generate` after an edit, or the edit is lost.
pub struct Page<'d> {
    doc: &'d Document,
    h: Handle,
    index: usize,
}

impl Drop for Page<'_> {
    fn drop(&mut self) {
        // SAFETY: `h` is an open page.
        unsafe { (self.f().close_page)(self.h) }
    }
}

impl<'d> Page<'d> {
    fn f(&self) -> &'d Fns {
        &self.doc.p.f
    }

    pub fn info(&self) -> PageInfo {
        let f = self.f();
        let read_box = |get: unsafe extern "C" fn(
            Handle,
            *mut f32,
            *mut f32,
            *mut f32,
            *mut f32,
        ) -> ffi::Bool| {
            let (mut l, mut b, mut r, mut t) = (0f32, 0f32, 0f32, 0f32);
            // SAFETY: `h` is an open page, and the four pointers are valid.
            let ok = unsafe { get(self.h, &mut l, &mut b, &mut r, &mut t) };
            (ok != 0).then(|| Rect {
                left: l.min(r) as f64,
                bottom: b.min(t) as f64,
                right: l.max(r) as f64,
                top: b.max(t) as f64,
            })
        };
        // SAFETY: `h` is an open page.
        let quarter_turns = unsafe { (f.page_get_rotation)(self.h) }.rem_euclid(4) as u8;
        let media_box = read_box(f.page_get_media_box).unwrap_or_else(|| {
            // An inherited media box is not visible to FPDFPage_GetMediaBox.
            // SAFETY: `h` is an open page.
            let (w, h) = unsafe {
                (
                    (f.page_width)(self.h) as f64,
                    (f.page_height)(self.h) as f64,
                )
            };
            let (w, h) = if quarter_turns % 2 == 1 {
                (h, w)
            } else {
                (w, h)
            };
            Rect {
                left: 0.0,
                bottom: 0.0,
                right: w,
                top: h,
            }
        });
        let crop_box = read_box(f.page_get_crop_box).unwrap_or(media_box);
        PageInfo {
            label: self.doc.page_label(self.index),
            media_box,
            crop_box,
            quarter_turns,
        }
    }

    pub fn set_rotation(&self, quarter_turns: u8) {
        // SAFETY: `h` is an open page.
        unsafe { (self.f().page_set_rotation)(self.h, (quarter_turns % 4) as c_int) }
    }

    pub fn object_count(&self) -> usize {
        // SAFETY: `h` is an open page.
        unsafe { (self.f().page_count_objects)(self.h).max(0) as usize }
    }

    fn object(&self, index: usize) -> Result<Handle, EngineError> {
        if index >= self.object_count() {
            return Err(EngineError::InvalidArgument(format!(
                "object index {index} is out of range"
            )));
        }
        // SAFETY: the index is in range.
        Ok(unsafe { (self.f().page_get_object)(self.h, index as c_int) })
    }

    pub fn objects(&self) -> Vec<ObjectInfo> {
        let f = self.f();
        (0..self.object_count())
            .map(|i| {
                // SAFETY: the index is in range, and the pointers are valid.
                unsafe {
                    let o = (f.page_get_object)(self.h, i as c_int);
                    let (mut l, mut b, mut r, mut t) = (0f32, 0f32, 0f32, 0f32);
                    (f.obj_get_bounds)(o, &mut l, &mut b, &mut r, &mut t);
                    ObjectInfo {
                        kind: object_kind((f.obj_get_type)(o)),
                        bounds: Rect {
                            left: l as f64,
                            bottom: b as f64,
                            right: r as f64,
                            top: t as f64,
                        },
                    }
                }
            })
            .collect()
    }

    /// Opens each annotation. The caller must close each handle.
    fn annot_handles(&self) -> Vec<(usize, Handle)> {
        let f = self.f();
        // SAFETY: `h` is an open page, and the indices are in range.
        unsafe {
            (0..(f.annot_count)(self.h).max(0) as usize)
                .map(|i| (i, (f.annot_get)(self.h, i as c_int)))
                .filter(|(_, a)| !a.is_null())
                .collect()
        }
    }

    pub fn annotation_count(&self) -> usize {
        // SAFETY: `h` is an open page.
        unsafe { (self.f().annot_count)(self.h).max(0) as usize }
    }

    pub fn annotations(&self) -> Vec<AnnotationInfo> {
        let f = self.f();
        let contents = CString::new("Contents").unwrap();
        self.annot_handles()
            .into_iter()
            .map(|(_, a)| {
                // SAFETY: `a` is open until `annot_close`.
                unsafe {
                    let info = AnnotationInfo {
                        subtype: annot_subtype_name((f.annot_get_subtype)(a)).to_string(),
                        bounds: annot_rect(f, a),
                        contents: read_wide(|b, l| {
                            (f.annot_get_string_value)(a, contents.as_ptr(), b, l)
                        }),
                    };
                    (f.annot_close)(a);
                    info
                }
            })
            .collect()
    }

    pub fn remove_annotation(&self, index: usize) -> Result<(), EngineError> {
        if index >= self.annotation_count() {
            return Err(EngineError::InvalidArgument(format!(
                "annotation index {index} is out of range"
            )));
        }
        // SAFETY: the index is in range.
        check(
            unsafe { (self.f().annot_remove)(self.h, index as c_int) },
            "remove annotation",
        )
    }

    /// Inserts a new object at the end of the list and returns its index.
    fn insert(&self, obj: Handle) -> usize {
        // SAFETY: `obj` is a new object. The page takes ownership.
        unsafe { (self.f().page_insert_object)(self.h, obj) };
        self.object_count() - 1
    }

    pub fn add_text(
        &self,
        font: FontRef,
        origin: Point,
        text: &str,
        size: f64,
        color: Rgba,
    ) -> Result<usize, EngineError> {
        let f = self.f();
        let w = wide(text);
        // SAFETY: the font belongs to this document. On failure, the object is destroyed.
        unsafe {
            let obj = (f.text_obj_create)(self.doc.h, font.0, size as f32);
            if obj.is_null() {
                return Err(EngineError::Internal(
                    "PDFium cannot make a text object".into(),
                ));
            }
            if (f.text_set_text)(obj, w.as_ptr()) == 0 {
                (f.obj_destroy)(obj);
                return Err(EngineError::Internal("PDFium cannot set the text".into()));
            }
            (f.obj_transform)(obj, 1.0, 0.0, 0.0, 1.0, origin.x, origin.y);
            let (r, g, b, a) = rgba_args(color);
            (f.obj_set_fill_color)(obj, r, g, b, a);
            Ok(self.insert(obj))
        }
    }

    pub fn add_image(&self, bounds: Rect, data: &ImageData) -> Result<usize, EngineError> {
        let f = self.f();
        // SAFETY: the image object is new. On failure, it is destroyed.
        unsafe {
            let obj = (f.image_new)(self.doc.h);
            if obj.is_null() {
                return Err(EngineError::Internal(
                    "PDFium cannot make an image object".into(),
                ));
            }
            let mut page = self.h;
            let ok = match data {
                ImageData::Jpeg(bytes) => {
                    extern "C" fn get_block(
                        param: *mut c_void,
                        pos: c_ulong,
                        buf: *mut u8,
                        size: c_ulong,
                    ) -> c_int {
                        // SAFETY: `param` is the live byte slice. PDFium keeps pos + size inside its length.
                        unsafe {
                            let data = &*(param as *const &[u8]);
                            let src = &data[pos as usize..(pos + size) as usize];
                            std::ptr::copy_nonoverlapping(src.as_ptr(), buf, size as usize);
                        }
                        1
                    }
                    let slice: &[u8] = bytes;
                    let mut access = ffi::FileAccess {
                        len: slice.len() as c_ulong,
                        get_block,
                        param: &slice as *const &[u8] as *mut c_void,
                    };
                    // The inline variant reads all data before it returns.
                    (f.image_load_jpeg_inline)(&mut page, 1, obj, &mut access)
                }
                ImageData::Png(bytes) => match decode_png(bytes) {
                    Ok((width, height, mut bgra)) => {
                        let bmp = (f.bitmap_create_ex)(
                            width as c_int,
                            height as c_int,
                            ffi::FPDFBITMAP_BGRA,
                            bgra.as_mut_ptr() as *mut c_void,
                            (width * 4) as c_int,
                        );
                        let ok =
                            !bmp.is_null() && (f.image_set_bitmap)(&mut page, 1, obj, bmp) != 0;
                        if !bmp.is_null() {
                            (f.bitmap_destroy)(bmp);
                        }
                        ok as ffi::Bool
                    }
                    Err(e) => {
                        (f.obj_destroy)(obj);
                        return Err(e);
                    }
                },
            };
            if ok == 0 {
                (f.obj_destroy)(obj);
                return Err(EngineError::UnsupportedImage(
                    "PDFium cannot read the image".into(),
                ));
            }
            (f.image_set_matrix)(
                obj,
                bounds.width(),
                0.0,
                0.0,
                bounds.height(),
                bounds.left,
                bounds.bottom,
            );
            Ok(self.insert(obj))
        }
    }

    pub fn add_path(
        &self,
        segments: &[PathSegment],
        stroke: Option<&Stroke>,
        fill: Option<Rgba>,
    ) -> Result<usize, EngineError> {
        let f = self.f();
        let Some(PathSegment::MoveTo(start)) = segments.first() else {
            return Err(EngineError::InvalidArgument(
                "a path must start with MoveTo".into(),
            ));
        };
        // SAFETY: the path object is new. The page takes it in `insert`.
        unsafe {
            let obj = (f.path_create)(start.x as f32, start.y as f32);
            if obj.is_null() {
                return Err(EngineError::Internal("PDFium cannot make a path".into()));
            }
            for s in &segments[1..] {
                match *s {
                    PathSegment::MoveTo(p) => (f.path_move_to)(obj, p.x as f32, p.y as f32),
                    PathSegment::LineTo(p) => (f.path_line_to)(obj, p.x as f32, p.y as f32),
                    PathSegment::CubicTo(a, b, c) => (f.path_bezier_to)(
                        obj, a.x as f32, a.y as f32, b.x as f32, b.y as f32, c.x as f32, c.y as f32,
                    ),
                    PathSegment::Close => (f.path_close)(obj),
                };
            }
            if let Some(s) = stroke {
                let (r, g, b, a) = rgba_args(s.color);
                (f.obj_set_stroke_color)(obj, r, g, b, a);
                (f.obj_set_stroke_width)(obj, s.width as f32);
                if !s.dash.is_empty() {
                    let dash: Vec<f32> = s.dash.iter().map(|&d| d as f32).collect();
                    (f.obj_set_dash_array)(obj, dash.as_ptr(), dash.len(), 0.0);
                }
            }
            if let Some(c) = fill {
                let (r, g, b, a) = rgba_args(c);
                (f.obj_set_fill_color)(obj, r, g, b, a);
            }
            let mode = if fill.is_some() {
                ffi::FPDF_FILLMODE_WINDING
            } else {
                ffi::FPDF_FILLMODE_NONE
            };
            (f.path_set_draw_mode)(obj, mode, stroke.is_some() as ffi::Bool);
            Ok(self.insert(obj))
        }
    }

    pub fn remove_object(&self, index: usize) -> Result<(), EngineError> {
        let obj = self.object(index)?;
        // SAFETY: `obj` belongs to this page. After removal, we own it and destroy it.
        unsafe {
            check((self.f().page_remove_object)(self.h, obj), "remove object")?;
            (self.f().obj_destroy)(obj);
        }
        Ok(())
    }

    /// Writes the page content. Edits are lost if the page closes without this call.
    pub fn generate(&self) -> Result<(), EngineError> {
        // SAFETY: `h` is an open page.
        check(
            unsafe { (self.f().page_generate_content)(self.h) },
            "generate content",
        )
    }

    fn with_text_page<T>(&self, run: impl FnOnce(Handle) -> T) -> Result<T, EngineError> {
        // SAFETY: `h` is an open page. The text page closes before this returns.
        unsafe {
            let tp = (self.f().text_load_page)(self.h);
            if tp.is_null() {
                return Err(EngineError::Internal(
                    "PDFium cannot load the page text".into(),
                ));
            }
            let out = run(tp);
            (self.f().text_close_page)(tp);
            Ok(out)
        }
    }

    /// Object kinds in order, with the text of each text object (ADR 0015).
    pub fn fingerprint(&self) -> Result<Vec<(ObjectKind, String)>, EngineError> {
        let f = self.f();
        self.with_text_page(|tp| {
            (0..self.object_count())
                .map(|i| {
                    // SAFETY: the index is in range, and `tp` is open.
                    unsafe {
                        let o = (f.page_get_object)(self.h, i as c_int);
                        let kind = object_kind((f.obj_get_type)(o));
                        let text = if kind == ObjectKind::Text {
                            read_wide(|b, l| (f.text_obj_get_text)(o, tp, b, l)).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        (kind, text)
                    }
                })
                .collect()
        })
    }

    pub fn text(&self) -> Result<String, EngineError> {
        let f = self.f();
        self.with_text_page(|tp| {
            // SAFETY: the buffer has room for `n` characters and a NUL.
            unsafe {
                let n = (f.text_count_chars)(tp).max(0);
                let mut buf = vec![0u16; n as usize + 1];
                let written = (f.text_get_text)(tp, 0, n, buf.as_mut_ptr()).max(1) as usize - 1;
                String::from_utf16_lossy(&buf[..written])
            }
        })
    }

    pub fn text_runs(&self) -> Result<Vec<TextRun>, EngineError> {
        let f = self.f();
        let index_of: HashMap<usize, usize> = (0..self.object_count())
            // SAFETY: the index is in range.
            .map(|i| {
                (
                    unsafe { (f.page_get_object)(self.h, i as c_int) } as usize,
                    i,
                )
            })
            .collect();
        self.with_text_page(|tp| {
            let mut runs: Vec<(usize, TextRun)> = Vec::new();
            // SAFETY: `tp` is open, and each index is below the char count.
            unsafe {
                for i in 0..(f.text_count_chars)(tp).max(0) {
                    let obj = (f.text_get_text_object)(tp, i);
                    if obj.is_null() {
                        continue; // A space or line end that PDFium added.
                    }
                    let c = char::from_u32((f.text_get_unicode)(tp, i)).unwrap_or('\u{FFFD}');
                    let (mut l, mut r, mut b, mut t) = (0.0, 0.0, 0.0, 0.0);
                    (f.text_get_char_box)(tp, i, &mut l, &mut r, &mut b, &mut t);
                    let bounds = Rect {
                        left: l,
                        bottom: b,
                        right: r,
                        top: t,
                    };
                    let key = obj as usize;
                    if runs.last().map(|(k, _)| *k) != Some(key) {
                        let mut size = 0f32;
                        (f.text_obj_get_font_size)(obj, &mut size);
                        let font = (f.text_obj_get_font)(obj);
                        let mut name = vec![0u8; 256];
                        let n = if font.is_null() {
                            0
                        } else {
                            (f.font_get_base_name)(font, name.as_mut_ptr() as *mut _, name.len())
                        };
                        let font_name =
                            String::from_utf8_lossy(&name[..n.saturating_sub(1).min(name.len())])
                                .into_owned();
                        runs.push((
                            key,
                            TextRun {
                                object: index_of.get(&key).copied(),
                                text: String::new(),
                                font_name,
                                font_size: size as f64,
                                bounds,
                                chars: Vec::new(),
                            },
                        ));
                    }
                    let run = &mut runs.last_mut().unwrap().1;
                    run.text.push(c);
                    run.bounds = union(run.bounds, bounds);
                    run.chars.push(CharInfo { c, bounds });
                }
            }
            runs.into_iter().map(|(_, r)| r).collect()
        })
    }

    /// Returns each hit as its text and its boxes, at most `max` hits.
    pub fn search(
        &self,
        query: &str,
        match_case: bool,
        whole_word: bool,
        max: usize,
    ) -> Result<Vec<(String, Vec<Rect>)>, EngineError> {
        let f = self.f();
        let q = wide(query);
        let flags = if match_case { ffi::FPDF_MATCHCASE } else { 0 }
            | if whole_word {
                ffi::FPDF_MATCHWHOLEWORD
            } else {
                0
            };
        self.with_text_page(|tp| {
            let mut hits = Vec::new();
            // SAFETY: `tp` is open. The search handle closes before return.
            unsafe {
                let sch = (f.text_find_start)(tp, q.as_ptr(), flags, 0);
                if sch.is_null() {
                    return hits;
                }
                while hits.len() < max && (f.text_find_next)(sch) != 0 {
                    let start = (f.text_get_sch_result_index)(sch);
                    let count = (f.text_get_sch_count)(sch);
                    let mut buf = vec![0u16; count.max(0) as usize + 1];
                    let n =
                        (f.text_get_text)(tp, start, count, buf.as_mut_ptr()).max(1) as usize - 1;
                    let mut boxes = Vec::new();
                    for r in 0..(f.text_count_rects)(tp, start, count) {
                        let (mut l, mut t, mut rt, mut b) = (0.0, 0.0, 0.0, 0.0);
                        if (f.text_get_rect)(tp, r, &mut l, &mut t, &mut rt, &mut b) != 0 {
                            boxes.push(Rect {
                                left: l,
                                bottom: b.min(t),
                                right: rt,
                                top: b.max(t),
                            });
                        }
                    }
                    hits.push((String::from_utf16_lossy(&buf[..n]), boxes));
                }
                (f.text_find_close)(sch);
            }
            hits
        })
    }

    /// Renders with the page rotation and annotations. The pause callback reads `cancel`.
    pub fn render(
        &self,
        scale: f64,
        clip: Option<Clip>,
        cancel: &AtomicBool,
    ) -> Result<Bitmap, EngineError> {
        let f = self.f();
        // SAFETY: `h` is an open page.
        let (pw, ph) = unsafe {
            (
                (f.page_width)(self.h) as f64,
                (f.page_height)(self.h) as f64,
            )
        };
        let (full_w, full_h) = ((pw * scale).round() as i32, (ph * scale).round() as i32);
        let clip = clip.unwrap_or(Clip {
            x: 0,
            y: 0,
            width: full_w.max(1) as u32,
            height: full_h.max(1) as u32,
        });
        if clip.width == 0 || clip.height == 0 || clip.width > 16384 || clip.height > 16384 {
            return Err(EngineError::InvalidArgument(
                "render size is out of range".into(),
            ));
        }
        let stride = clip.width * 4;
        let mut bgra = vec![0u8; (stride * clip.height) as usize];

        extern "C" fn need_to_pause(this: *mut ffi::Pause) -> ffi::Bool {
            // SAFETY: `user` points to the cancel flag, which outlives the render.
            unsafe { (*((*this).user as *const AtomicBool)).load(Ordering::Relaxed) as ffi::Bool }
        }
        let mut pause = ffi::Pause {
            version: 1,
            need_to_pause_now: need_to_pause,
            user: cancel as *const AtomicBool as *mut c_void,
        };
        // SAFETY: the buffer outlives the bitmap. The render closes before return.
        unsafe {
            let bmp = (f.bitmap_create_ex)(
                clip.width as c_int,
                clip.height as c_int,
                ffi::FPDFBITMAP_BGRA,
                bgra.as_mut_ptr() as *mut c_void,
                stride as c_int,
            );
            if bmp.is_null() {
                return Err(EngineError::Internal("PDFium cannot make a bitmap".into()));
            }
            (f.bitmap_fill_rect)(
                bmp,
                0,
                0,
                clip.width as c_int,
                clip.height as c_int,
                0xFFFF_FFFF,
            );
            let mut status = (f.render_start)(
                bmp,
                self.h,
                -clip.x,
                -clip.y,
                full_w,
                full_h,
                0,
                ffi::FPDF_ANNOT,
                &mut pause,
            );
            while status == ffi::FPDF_RENDER_TOBECONTINUED && !cancel.load(Ordering::Relaxed) {
                status = (f.render_continue)(self.h, &mut pause);
            }
            (f.render_close)(self.h);
            (f.bitmap_destroy)(bmp);
            if cancel.load(Ordering::Relaxed) && status != ffi::FPDF_RENDER_DONE {
                return Err(EngineError::Cancelled);
            }
            if status != ffi::FPDF_RENDER_DONE {
                return Err(EngineError::Internal(format!("render status {status}")));
            }
        }
        Ok(Bitmap {
            width: clip.width,
            height: clip.height,
            stride,
            bgra,
        })
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        left: a.left.min(b.left),
        bottom: a.bottom.min(b.bottom),
        right: a.right.max(b.right),
        top: a.top.max(b.top),
    }
}

/// Largest image that the engine decodes: 100 megapixels.
const MAX_PIXELS: u64 = 100_000_000;

/// Decodes a PNG to BGRA with straight alpha.
fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), EngineError> {
    let bad = |e: &dyn std::fmt::Display| EngineError::UnsupportedImage(format!("PNG: {e}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| bad(&e))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w as u64 * h as u64 > MAX_PIXELS {
        return Err(EngineError::UnsupportedImage(
            "the image has more than 100 megapixels".into(),
        ));
    }
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| bad(&"size overflow"))?
    ];
    let frame = reader.next_frame(&mut buf).map_err(|e| bad(&e))?;
    let px = &buf[..frame.buffer_size()];
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    match frame.color_type {
        png::ColorType::Rgba => {
            for [r, g, b, a] in px.as_chunks::<4>().0 {
                out.extend_from_slice(&[*b, *g, *r, *a]);
            }
        }
        png::ColorType::Rgb => {
            for [r, g, b] in px.as_chunks::<3>().0 {
                out.extend_from_slice(&[*b, *g, *r, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for [v, a] in px.as_chunks::<2>().0 {
                out.extend_from_slice(&[*v, *v, *v, *a]);
            }
        }
        png::ColorType::Grayscale => px
            .iter()
            .for_each(|&g| out.extend_from_slice(&[g, g, g, 255])),
        other => return Err(bad(&format!("color type {other:?}"))),
    }
    Ok((w, h, out))
}
