//! Declarations of the PDFium C functions that the engine uses (ADR 0013).
//! Types follow the headers of the pinned build in `.pdfium/<platform>/include`.

use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_ushort, c_void};

pub type Handle = *mut c_void;
pub type Bool = c_int;
pub type WChar = c_ushort;

pub const FPDF_ERR_PASSWORD: c_ulong = 4;
pub const FPDF_NO_INCREMENTAL: c_ulong = 1 << 1;
pub const FPDF_ANNOT: c_int = 0x01;
pub const FPDF_FONT_TRUETYPE: c_int = 2;
pub const FPDF_MATCHCASE: c_ulong = 1;
pub const FPDF_MATCHWHOLEWORD: c_ulong = 2;
pub const FPDF_RENDER_TOBECONTINUED: c_int = 1;
pub const FPDF_RENDER_DONE: c_int = 2;
pub const FPDF_FORMFLAG_READONLY: c_int = 1;
pub const FPDF_FILLMODE_NONE: c_int = 0;
pub const FPDF_FILLMODE_WINDING: c_int = 2;
pub const FPDFBITMAP_BGRA: c_int = 4;
pub const PDFACTION_GOTO: c_ulong = 1;

#[repr(C)]
pub struct FileWrite {
    pub version: c_int,
    pub write_block: extern "C" fn(*mut FileWrite, *const c_void, c_ulong) -> c_int,
}

#[repr(C)]
pub struct FileAccess {
    pub len: c_ulong,
    pub get_block: extern "C" fn(*mut c_void, c_ulong, *mut u8, c_ulong) -> c_int,
    pub param: *mut c_void,
}

#[repr(C)]
pub struct Pause {
    pub version: c_int,
    pub need_to_pause_now: extern "C" fn(*mut Pause) -> Bool,
    pub user: *mut c_void,
}

/// `FS_RECTF`. Note the order: left, top, right, bottom.
#[repr(C)]
#[derive(Default)]
pub struct RectF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// `FPDF_FORMFILLINFO` version 1, with no callbacks. Larger than the real struct, all zero.
#[repr(C)]
pub struct FormFillInfo {
    pub version: c_int,
    pub rest: [u64; 128],
}

macro_rules! pdfium_fns {
    ($($field:ident = $sym:literal : fn($($arg:ty),*) $(-> $ret:ty)?;)+) => {
        pub struct Fns {
            $(pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)+
        }

        impl Fns {
            /// SAFETY: `lib` must be a PDFium build with the C API of these declarations.
            /// The function pointers are valid only while `lib` stays loaded.
            pub unsafe fn load(lib: &Library) -> Result<Self, libloading::Error> {
                unsafe {
                    Ok(Self {
                        $($field: *lib.get(concat!($sym, "\0").as_bytes())?,)+
                    })
                }
            }
        }
    };
}

pdfium_fns! {
    init_library = "FPDF_InitLibrary": fn();
    destroy_library = "FPDF_DestroyLibrary": fn();
    get_last_error = "FPDF_GetLastError": fn() -> c_ulong;

    load_mem_document = "FPDF_LoadMemDocument64": fn(*const c_void, usize, *const c_char) -> Handle;
    close_document = "FPDF_CloseDocument": fn(Handle);
    get_page_count = "FPDF_GetPageCount": fn(Handle) -> c_int;
    save_as_copy = "FPDF_SaveAsCopy": fn(Handle, *mut FileWrite, c_ulong) -> Bool;
    get_meta_text = "FPDF_GetMetaText": fn(Handle, *const c_char, *mut c_void, c_ulong) -> c_ulong;
    get_page_label = "FPDF_GetPageLabel": fn(Handle, c_int, *mut c_void, c_ulong) -> c_ulong;
    get_signature_count = "FPDF_GetSignatureCount": fn(Handle) -> c_int;

    load_page = "FPDF_LoadPage": fn(Handle, c_int) -> Handle;
    close_page = "FPDF_ClosePage": fn(Handle);
    page_new = "FPDFPage_New": fn(Handle, c_int, f64, f64) -> Handle;
    page_delete = "FPDFPage_Delete": fn(Handle, c_int);
    move_pages = "FPDF_MovePages": fn(Handle, *const c_int, c_ulong, c_int) -> Bool;
    page_get_rotation = "FPDFPage_GetRotation": fn(Handle) -> c_int;
    page_set_rotation = "FPDFPage_SetRotation": fn(Handle, c_int);
    page_get_media_box = "FPDFPage_GetMediaBox": fn(Handle, *mut f32, *mut f32, *mut f32, *mut f32) -> Bool;
    page_get_crop_box = "FPDFPage_GetCropBox": fn(Handle, *mut f32, *mut f32, *mut f32, *mut f32) -> Bool;
    page_width = "FPDF_GetPageWidthF": fn(Handle) -> f32;
    page_height = "FPDF_GetPageHeightF": fn(Handle) -> f32;
    page_generate_content = "FPDFPage_GenerateContent": fn(Handle) -> Bool;

    page_count_objects = "FPDFPage_CountObjects": fn(Handle) -> c_int;
    page_get_object = "FPDFPage_GetObject": fn(Handle, c_int) -> Handle;
    page_insert_object = "FPDFPage_InsertObject": fn(Handle, Handle);
    page_remove_object = "FPDFPage_RemoveObject": fn(Handle, Handle) -> Bool;
    obj_destroy = "FPDFPageObj_Destroy": fn(Handle);
    obj_get_type = "FPDFPageObj_GetType": fn(Handle) -> c_int;
    obj_get_bounds = "FPDFPageObj_GetBounds": fn(Handle, *mut f32, *mut f32, *mut f32, *mut f32) -> Bool;
    obj_transform = "FPDFPageObj_Transform": fn(Handle, f64, f64, f64, f64, f64, f64);
    obj_set_fill_color = "FPDFPageObj_SetFillColor": fn(Handle, c_uint, c_uint, c_uint, c_uint) -> Bool;
    obj_set_stroke_color = "FPDFPageObj_SetStrokeColor": fn(Handle, c_uint, c_uint, c_uint, c_uint) -> Bool;
    obj_set_stroke_width = "FPDFPageObj_SetStrokeWidth": fn(Handle, f32) -> Bool;
    obj_set_dash_array = "FPDFPageObj_SetDashArray": fn(Handle, *const f32, usize, f32) -> Bool;

    path_create = "FPDFPageObj_CreateNewPath": fn(f32, f32) -> Handle;
    path_move_to = "FPDFPath_MoveTo": fn(Handle, f32, f32) -> Bool;
    path_line_to = "FPDFPath_LineTo": fn(Handle, f32, f32) -> Bool;
    path_bezier_to = "FPDFPath_BezierTo": fn(Handle, f32, f32, f32, f32, f32, f32) -> Bool;
    path_close = "FPDFPath_Close": fn(Handle) -> Bool;
    path_set_draw_mode = "FPDFPath_SetDrawMode": fn(Handle, c_int, Bool) -> Bool;

    image_new = "FPDFPageObj_NewImageObj": fn(Handle) -> Handle;
    image_set_bitmap = "FPDFImageObj_SetBitmap": fn(*mut Handle, c_int, Handle, Handle) -> Bool;
    image_load_jpeg_inline = "FPDFImageObj_LoadJpegFileInline": fn(*mut Handle, c_int, Handle, *mut FileAccess) -> Bool;
    image_set_matrix = "FPDFImageObj_SetMatrix": fn(Handle, f64, f64, f64, f64, f64, f64) -> Bool;

    text_load_font = "FPDFText_LoadFont": fn(Handle, *const u8, u32, c_int, Bool) -> Handle;
    font_close = "FPDFFont_Close": fn(Handle);
    font_get_base_name = "FPDFFont_GetBaseFontName": fn(Handle, *mut c_char, usize) -> usize;
    text_obj_create = "FPDFPageObj_CreateTextObj": fn(Handle, Handle, f32) -> Handle;
    text_set_text = "FPDFText_SetText": fn(Handle, *const WChar) -> Bool;
    text_obj_get_font_size = "FPDFTextObj_GetFontSize": fn(Handle, *mut f32) -> Bool;
    text_obj_get_font = "FPDFTextObj_GetFont": fn(Handle) -> Handle;
    text_obj_get_text = "FPDFTextObj_GetText": fn(Handle, Handle, *mut WChar, c_ulong) -> c_ulong;

    text_load_page = "FPDFText_LoadPage": fn(Handle) -> Handle;
    text_close_page = "FPDFText_ClosePage": fn(Handle);
    text_count_chars = "FPDFText_CountChars": fn(Handle) -> c_int;
    text_get_unicode = "FPDFText_GetUnicode": fn(Handle, c_int) -> c_uint;
    text_get_char_box = "FPDFText_GetCharBox": fn(Handle, c_int, *mut f64, *mut f64, *mut f64, *mut f64) -> Bool;
    text_get_text_object = "FPDFText_GetTextObject": fn(Handle, c_int) -> Handle;
    text_get_text = "FPDFText_GetText": fn(Handle, c_int, c_int, *mut WChar) -> c_int;
    text_find_start = "FPDFText_FindStart": fn(Handle, *const WChar, c_ulong, c_int) -> Handle;
    text_find_next = "FPDFText_FindNext": fn(Handle) -> Bool;
    text_get_sch_result_index = "FPDFText_GetSchResultIndex": fn(Handle) -> c_int;
    text_get_sch_count = "FPDFText_GetSchCount": fn(Handle) -> c_int;
    text_find_close = "FPDFText_FindClose": fn(Handle);
    text_count_rects = "FPDFText_CountRects": fn(Handle, c_int, c_int) -> c_int;
    text_get_rect = "FPDFText_GetRect": fn(Handle, c_int, *mut f64, *mut f64, *mut f64, *mut f64) -> Bool;

    annot_count = "FPDFPage_GetAnnotCount": fn(Handle) -> c_int;
    annot_get = "FPDFPage_GetAnnot": fn(Handle, c_int) -> Handle;
    annot_close = "FPDFPage_CloseAnnot": fn(Handle);
    annot_remove = "FPDFPage_RemoveAnnot": fn(Handle, c_int) -> Bool;
    annot_get_subtype = "FPDFAnnot_GetSubtype": fn(Handle) -> c_int;
    annot_get_rect = "FPDFAnnot_GetRect": fn(Handle, *mut RectF) -> Bool;
    annot_get_string_value = "FPDFAnnot_GetStringValue": fn(Handle, *const c_char, *mut WChar, c_ulong) -> c_ulong;
    annot_get_form_field_type = "FPDFAnnot_GetFormFieldType": fn(Handle, Handle) -> c_int;
    annot_get_form_field_name = "FPDFAnnot_GetFormFieldName": fn(Handle, Handle, *mut WChar, c_ulong) -> c_ulong;
    annot_get_form_field_value = "FPDFAnnot_GetFormFieldValue": fn(Handle, Handle, *mut WChar, c_ulong) -> c_ulong;
    annot_get_form_field_flags = "FPDFAnnot_GetFormFieldFlags": fn(Handle, Handle) -> c_int;
    form_init = "FPDFDOC_InitFormFillEnvironment": fn(Handle, *mut FormFillInfo) -> Handle;
    form_exit = "FPDFDOC_ExitFormFillEnvironment": fn(Handle);

    bookmark_first_child = "FPDFBookmark_GetFirstChild": fn(Handle, Handle) -> Handle;
    bookmark_next_sibling = "FPDFBookmark_GetNextSibling": fn(Handle, Handle) -> Handle;
    bookmark_get_title = "FPDFBookmark_GetTitle": fn(Handle, *mut c_void, c_ulong) -> c_ulong;
    bookmark_get_dest = "FPDFBookmark_GetDest": fn(Handle, Handle) -> Handle;
    bookmark_get_action = "FPDFBookmark_GetAction": fn(Handle) -> Handle;
    action_get_type = "FPDFAction_GetType": fn(Handle) -> c_ulong;
    action_get_dest = "FPDFAction_GetDest": fn(Handle, Handle) -> Handle;
    dest_get_page_index = "FPDFDest_GetDestPageIndex": fn(Handle, Handle) -> c_int;

    bitmap_create_ex = "FPDFBitmap_CreateEx": fn(c_int, c_int, c_int, *mut c_void, c_int) -> Handle;
    bitmap_fill_rect = "FPDFBitmap_FillRect": fn(Handle, c_int, c_int, c_int, c_int, c_ulong) -> Bool;
    bitmap_destroy = "FPDFBitmap_Destroy": fn(Handle);
    render_start = "FPDF_RenderPageBitmap_Start": fn(Handle, Handle, c_int, c_int, c_int, c_int, c_int, c_int, *mut Pause) -> c_int;
    render_continue = "FPDF_RenderPage_Continue": fn(Handle, *mut Pause) -> c_int;
    render_close = "FPDF_RenderPage_Close": fn(Handle);
}
