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
    let (w, h) = doc.dsc.page_size();

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
    let (page_w, page_h) = doc.dsc.page_size();

    let mut interp = Interpreter::with_page_size(page_w, page_h, width, height);

    if !doc.dsc.pages.is_empty() && page_index < doc.dsc.pages.len() {
        if let Some((start, end)) = doc.dsc.preamble_range {
            interp.execute_bytes(&doc.data[start..end]).ok();
        }
        let page = &doc.dsc.pages[page_index];
        execute_page_with_embedded_recovery(
            &mut interp,
            &doc.data[page.start_byte_offset..page.end_byte_offset],
        );
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

pub fn execute_page_with_embedded_recovery(interp: &mut Interpreter, page_bytes: &[u8]) {
    let page_bytes = page_bytes
        .split_inclusive(|byte| *byte == b'\n')
        .scan(0usize, |offset, line| {
            *offset += line.len();
            Some((*offset, line))
        })
        .find_map(|(offset, line)| {
            (line.trim_ascii() == b"showpage").then_some(offset)
        })
        .map(|end| &page_bytes[..end])
        .unwrap_or(page_bytes);

    let mut segment_start = 0;
    let mut embedded_depth = 0usize;
    let mut offset = 0;

    for line in page_bytes.split_inclusive(|byte| *byte == b'\n') {
        let trimmed = line.trim_ascii();
        let begins_document = trimmed.starts_with(b"%%BeginDocument");
        let ends_document = trimmed.starts_with(b"%%EndDocument");

        if begins_document {
            if embedded_depth == 0 {
                if let Err(e) = interp.execute_bytes(&page_bytes[segment_start..offset]) {
                    eprintln!("Error executing page segment before %%BeginDocument: {}", e);
                }
            }
            embedded_depth += 1;
        } else if ends_document && embedded_depth > 0 {
            embedded_depth -= 1;
            if embedded_depth == 0 {
                segment_start = offset + line.len();
            }
        }

        offset += line.len();
    }

    if embedded_depth == 0 && segment_start < page_bytes.len() {
        if let Err(e) = interp.execute_bytes(&page_bytes[segment_start..]) {
            eprintln!("Error executing page segment: {}", e);
        }
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

pub struct PsInterpHandle {
    interp: Interpreter,
    page_h: f64,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_interp_create(
    page_w: f64,
    page_h: f64,
    pixel_w: u32,
    pixel_h: u32,
) -> *mut PsInterpHandle {
    if pixel_w == 0 || pixel_h == 0 {
        return std::ptr::null_mut();
    }
    let interp = Interpreter::with_page_size(page_w, page_h, pixel_w, pixel_h);
    let handle = Box::new(PsInterpHandle {
        interp,
        page_h,
    });
    Box::into_raw(handle)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_interp_execute(
    handle: *mut PsInterpHandle,
    data: *const u8,
    len: usize,
) -> i32 {
    if handle.is_null() || data.is_null() || len == 0 {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let slice = unsafe { slice::from_raw_parts(data, len) };
    if let Err(e) = handle.interp.execute_bytes(slice) {
        eprintln!("ps_interp_execute error: {}", e);
        return -1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_interp_set_current_point(
    handle: *mut PsInterpHandle,
    x: f64,
    y: f64,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    // Set current point in user coordinate space
    handle.interp.set_current_point_user(x, y);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_interp_render_rgba(
    handle: *mut PsInterpHandle,
    out_rgba_buffer: *mut u8,
    width: u32,
    height: u32,
) -> i32 {
    if handle.is_null() || out_rgba_buffer.is_null() || width == 0 || height == 0 {
        return -1;
    }
    let handle = unsafe { &mut *handle };
    let target = handle.interp.pages_rendered.last().unwrap_or(&handle.interp.render_target);
    // Render with transparent background (alpha = 0) so it overlays correctly on the page
    let transparent = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    match target.render_to_pixmap(transparent) {
        Ok(pixmap) => {
            let pixels = pixmap.data();
            let byte_count = (width * height * 4) as usize;
            if pixels.len() != byte_count {
                return -1;
            }
            let dest = unsafe { slice::from_raw_parts_mut(out_rgba_buffer, byte_count) };
            dest.copy_from_slice(pixels);
            0
        }
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_interp_free(handle: *mut PsInterpHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}
