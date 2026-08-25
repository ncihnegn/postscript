use crate::dsc::DscDocument;
use crate::gstate::Color;
use crate::interpreter::Interpreter;
use std::slice;

pub struct PsDocumentHandle {
    data: Vec<u8>,
    dsc: DscDocument,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_document_open(data: *const u8, len: usize) -> *mut PsDocumentHandle {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { slice::from_raw_parts(data, len) };
    let dsc = DscDocument::parse(slice);
    let handle = Box::new(PsDocumentHandle {
        data: slice.to_vec(),
        dsc,
    });
    Box::into_raw(handle)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_document_page_count(handle: *const PsDocumentHandle) -> usize {
    if handle.is_null() {
        return 0;
    }
    let doc = unsafe { &*handle };
    if doc.dsc.pages.is_empty() {
        1
    } else {
        doc.dsc.pages.len()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_document_get_page_size(
    handle: *const PsDocumentHandle,
    _page_index: usize,
    out_width: *mut f64,
    out_height: *mut f64,
) -> i32 {
    if handle.is_null() || out_width.is_null() || out_height.is_null() {
        return -1;
    }
    let doc = unsafe { &*handle };
    let (w, h) = if let Some(bbox) = &doc.dsc.bounding_box {
        (bbox.width(), bbox.height())
    } else if let Some(bbox) = &doc.dsc.hires_bounding_box {
        (bbox.width(), bbox.height())
    } else {
        (612.0, 792.0) // Standard US Letter default
    };

    unsafe {
        *out_width = if w > 0.0 { w } else { 612.0 };
        *out_height = if h > 0.0 { h } else { 792.0 };
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_document_render_page_rgba(
    handle: *const PsDocumentHandle,
    page_index: usize,
    width: u32,
    height: u32,
    out_rgba_buffer: *mut u8,
) -> i32 {
    if handle.is_null() || out_rgba_buffer.is_null() || width == 0 || height == 0 {
        return -1;
    }
    let doc = unsafe { &*handle };
    let (page_w, page_h) = if let Some(bbox) = &doc.dsc.bounding_box {
        (bbox.width(), bbox.height())
    } else if let Some(bbox) = &doc.dsc.hires_bounding_box {
        (bbox.width(), bbox.height())
    } else {
        (612.0, 792.0)
    };

    let mut interp = Interpreter::with_page_size(page_w, page_h, width, height);

    if !doc.dsc.pages.is_empty() && page_index < doc.dsc.pages.len() {
        if let Some((start, end)) = doc.dsc.preamble_range {
            interp.execute_bytes(&doc.data[start..end]).ok();
        }
        let page = &doc.dsc.pages[page_index];
        if let Err(_) = interp.execute_bytes(&doc.data[page.start_byte_offset..page.end_byte_offset]) {
            // Execution warning, proceed with recorded commands
        }
    } else {
        if let Err(_) = interp.execute_bytes(&doc.data) {
            // Execution warning, proceed with recorded commands
        }
    }

    let target = interp.pages_rendered.last().unwrap_or(&interp.render_target);
    match target.render_to_pixmap(Color::WHITE) {
        Ok(pixmap) => {
            let pixels = pixmap.data();
            let dest = unsafe { slice::from_raw_parts_mut(out_rgba_buffer, (width * height * 4) as usize) };
            dest.copy_from_slice(pixels);
            0
        }
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_document_free(handle: *mut PsDocumentHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}
