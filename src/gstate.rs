use crate::font::FontFace;
use crate::matrix::Matrix2D;
use crate::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    pub fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn gray(g: f64) -> Self {
        Self { r: g, g: g, b: g, a: 1.0 }
    }

    pub fn cmyk(c: f64, m: f64, y: f64, k: f64) -> Self {
        let r = (1.0 - c) * (1.0 - k);
        let g = (1.0 - m) * (1.0 - k);
        let b = (1.0 - y) * (1.0 - k);
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipPath {
    pub path: Path,
    pub even_odd: bool,
}

#[derive(Debug, Clone)]
pub struct GraphicsState {
    pub ctm: Matrix2D,
    pub current_path: Path,
    pub current_point: Option<(f64, f64)>,
    pub subpath_start: Option<(f64, f64)>,
    pub line_width: f64,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
    pub color: Color,
    pub font: Option<FontFace>,
    pub clip_paths: Vec<ClipPath>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix2D::identity(),
            current_path: Path::new(),
            current_point: None,
            subpath_start: None,
            line_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 10.0,
            dash_pattern: Vec::new(),
            dash_offset: 0.0,
            color: Color::BLACK,
            font: None,
            clip_paths: Vec::new(),
        }
    }
}

impl GraphicsState {
    pub fn new() -> Self {
        Self::default()
    }
}
