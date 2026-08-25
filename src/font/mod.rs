pub mod charstring;
pub mod eexec;
pub mod type1;

use std::collections::HashMap;
use crate::matrix::Matrix2D;
use crate::path::Path;
pub use charstring::{CharStringInterpreter, GlyphOutline};
pub use type1::Type1Parser;

#[derive(Debug, Clone)]
pub struct FontFace {
    pub name: String,
    pub matrix: Matrix2D,
    pub encoding: Vec<String>,
    pub charstrings: HashMap<String, GlyphOutline>,
    pub subrs: Vec<Vec<u8>>,
}

impl FontFace {
    pub fn new(name: &str) -> Self {
        let mut encoding = vec![".notdef".to_string(); 256];
        // Default standard encoding printable ASCII
        for i in 32..=126 {
            let ch = i as u8 as char;
            encoding[i] = ch.to_string();
        }

        Self {
            name: name.to_string(),
            matrix: Matrix2D::scale(0.001, 0.001),
            encoding,
            charstrings: HashMap::new(),
            subrs: Vec::new(),
        }
    }

    pub fn scalefont(&self, scale: f64) -> Self {
        let mut f = self.clone();
        f.matrix = Matrix2D::scale(scale * 0.001, scale * 0.001);
        f
    }

    pub fn makefont(&self, matrix: Matrix2D) -> Self {
        let mut f = self.clone();
        f.matrix = matrix.concat(&Matrix2D::scale(0.001, 0.001));
        f
    }

    pub fn get_glyph_path(&self, glyph_name: &str) -> Option<(Path, f64)> {
        if let Some(glyph) = self.charstrings.get(glyph_name) {
            let transformed_path = glyph.path.transform(&self.matrix);
            let (scaled_width, _) = self.matrix.transform_vector(glyph.width, 0.0);
            Some((transformed_path, scaled_width.abs()))
        } else {
            None
        }
    }
}
