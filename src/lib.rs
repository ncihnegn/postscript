pub mod dsc;
pub mod error;
pub mod ffi;
pub mod font;
pub mod gstate;
pub mod interpreter;
pub mod lexer;
pub mod matrix;
pub mod path;
pub mod render;
pub mod value;

pub use dsc::{BoundingBox, DscDocument, DscPage};
pub use error::{PsError, PsResult};
pub use font::{FontFace, GlyphOutline};
pub use gstate::{Color, GraphicsState, LineCap, LineJoin};
pub use interpreter::Interpreter;
pub use lexer::{Lexer, Token};
pub use matrix::Matrix2D;
pub use path::{Path, PathSegment};
pub use render::{DrawCommand, RenderTarget};
pub use value::Value;

/// Renders a specific page or the whole PostScript stream to a `tiny_skia::Pixmap`.
pub fn render_ps_to_pixmap(ps_bytes: &[u8], page_index: usize, width: u32, height: u32) -> PsResult<tiny_skia::Pixmap> {
    let dsc = DscDocument::parse(ps_bytes);
    let (page_w, page_h) = if let Some(bbox) = &dsc.bounding_box {
        (bbox.width(), bbox.height())
    } else if let Some(bbox) = &dsc.hires_bounding_box {
        (bbox.width(), bbox.height())
    } else {
        (612.0, 792.0)
    };
    let mut interp = Interpreter::with_page_size(page_w, page_h, width, height);

    if !dsc.pages.is_empty() && page_index < dsc.pages.len() {
        // Execute Document Preamble / Setup before first page
        if let Some((start, end)) = dsc.preamble_range {
            interp.execute_bytes(&ps_bytes[start..end]).ok();
        }

        // Execute specific page
        let page = &dsc.pages[page_index];
        interp.execute_bytes(&ps_bytes[page.start_byte_offset..page.end_byte_offset])?;

        if !interp.pages_rendered.is_empty() {
            return interp.pages_rendered.last().unwrap().render_to_pixmap(Color::WHITE);
        }
    } else {
        // Run continuous stream
        interp.execute_bytes(ps_bytes)?;
        if !interp.pages_rendered.is_empty() {
            return interp.pages_rendered.last().unwrap().render_to_pixmap(Color::WHITE);
        }
    }

    interp.render_target.render_to_pixmap(Color::WHITE)
}

/// Renders a PostScript page directly to PNG encoded bytes.
pub fn render_ps_to_png(ps_bytes: &[u8], page_index: usize, width: u32, height: u32) -> PsResult<Vec<u8>> {
    let pixmap = render_ps_to_pixmap(ps_bytes, page_index, width, height)?;
    pixmap.encode_png().map_err(|e| PsError::IoError(e.to_string()))
}
