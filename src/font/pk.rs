//! PK bitmap fonts.
//!
//! TeX documents converted to PostScript often reference bitmap-only fonts such
//! as `manfnt` by name alone. Those fonts have no embedded Type 1 program, so
//! the only glyph source on a TeX system is the `.pk` file. This module parses
//! PK files and turns their glyph rasters into paths so the regular fill
//! pipeline can draw them.

use crate::path::Path;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const FIX: f64 = 1048576.0; // 1 << 20

#[derive(Debug, Clone)]
pub struct PkGlyph {
    pub character_code: u32,
    /// Character width from the TFM file, in fix units of the design size.
    pub tfm_width: u32,
    pub width: u32,
    pub height: u32,
    pub hoff: i32,
    pub voff: i32,
    /// Rows packed most-significant-bit first, `(width + 7) / 8` bytes per row,
    /// top row first.
    pub bitmap: Vec<u8>,
}

pub struct PkFont {
    pub design_size: f64,
    pub pixels_per_point: f64,
    pub checksum: u32,
    pub glyphs: HashMap<u32, PkGlyph>,
}

impl std::fmt::Debug for PkFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkFont")
            .field("design_size", &self.design_size)
            .field("pixels_per_point", &self.pixels_per_point)
            .field("glyphs", &self.glyphs.len())
            .finish()
    }
}

impl PkFont {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut reader = Cursor::new(data);
        if reader.byte()? != 247 {
            return None; // pre
        }
        if reader.byte()? != 89 {
            return None; // PK identifier
        }
        let comment_length = reader.byte()? as usize;
        reader.skip(comment_length)?;

        let design_size = reader.unsigned(4)? as f64 / FIX;
        let checksum = reader.unsigned(4)?;
        let pixels_per_point = reader.unsigned(4)? as f64 / 65536.0;
        let _vertical_pixels_per_point = reader.unsigned(4)?;
        if !(design_size > 0.0) || !(pixels_per_point > 0.0) {
            return None;
        }

        let mut glyphs: HashMap<u32, PkGlyph> = HashMap::new();

        loop {
            let flag = match reader.byte() {
                Some(flag) => flag,
                None => break,
            };

            if flag >= 240 {
                match flag {
                    240..=243 => {
                        let length_bytes = (flag - 239) as usize;
                        let Some(length) = reader.unsigned(length_bytes) else {
                            break;
                        };
                        if reader.skip(length as usize).is_none() {
                            break;
                        }
                    }
                    244 => {
                        if reader.skip(4).is_none() {
                            break;
                        }
                    }
                    245 => break, // postamble
                    246 => continue,
                    _ => break,
                }
                continue;
            }

            let kind = flag & 0x07;
            let dynamic = (flag >> 4) as usize;
            let black_first = (flag & 0x08) != 0;

            // Each packet announces its own length; the raster is everything
            // after the fixed header that is not the packet's length field.
            let pl: u32;
            let header_delta: usize;
            let character_code: u32;
            let tfm_width: u32;
            let width: u32;
            let height: u32;
            let hoff: i32;
            let voff: i32;

            match kind {
                0..=3 => {
                    pl = ((kind as u32 & 0x03) << 8) | reader.unsigned(1)?;
                    header_delta = 8;
                    character_code = reader.unsigned(1)?;
                    tfm_width = reader.unsigned(3)?;
                    let _dm = reader.byte()?;
                    width = reader.unsigned(1)?;
                    height = reader.unsigned(1)?;
                    hoff = reader.signed(1)?;
                    voff = reader.signed(1)?;
                }
                4..=6 => {
                    pl = ((kind as u32 & 0x03) << 16) | reader.unsigned(2)?;
                    header_delta = 13;
                    character_code = reader.unsigned(1)?;
                    tfm_width = reader.unsigned(3)?;
                    let _dm = reader.unsigned(2)?;
                    width = reader.unsigned(2)?;
                    height = reader.unsigned(2)?;
                    hoff = reader.signed(2)?;
                    voff = reader.signed(2)?;
                }
                _ => {
                    pl = reader.unsigned(4)?;
                    header_delta = 28;
                    character_code = reader.unsigned(4)?;
                    tfm_width = reader.unsigned(4)?;
                    let _dx = reader.unsigned(4)?;
                    let _dy = reader.unsigned(4)?;
                    width = reader.unsigned(4)?;
                    height = reader.unsigned(4)?;
                    hoff = reader.signed(4)?;
                    voff = reader.signed(4)?;
                }
            }

            let Some(raster_length) = (pl as usize).checked_sub(header_delta) else {
                break;
            };
            let Some(raster) = reader.take(raster_length) else {
                break;
            };

            let bitmap = if width == 0 || height == 0 {
                Vec::new()
            } else if dynamic == 14 {
                decode_raw(raster, width, height)
            } else {
                decode_packed(raster, dynamic, black_first, width, height)
            };

            glyphs.insert(
                character_code,
                PkGlyph {
                    character_code,
                    tfm_width,
                    width,
                    height,
                    hoff,
                    voff,
                    bitmap,
                },
            );
        }

        if glyphs.is_empty() {
            return None;
        }

        Some(Self {
            design_size,
            pixels_per_point,
            checksum,
            glyphs,
        })
    }

    /// Size of one bitmap pixel in glyph space, where 1000 units make an em.
    pub fn units_per_pixel(&self) -> f64 {
        1000.0 / (self.pixels_per_point * self.design_size)
    }

    /// Glyph outline (in glyph space) and advance for a character code.
    pub fn glyph_path(&self, character_code: u32) -> Option<(Path, f64)> {
        let glyph = self.glyphs.get(&character_code)?;
        let unit = self.units_per_pixel();
        if !unit.is_finite() || unit <= 0.0 {
            return None;
        }

        let advance = glyph.tfm_width as f64 / FIX * 1000.0;
        let mut path = Path::new();

        let width = glyph.width as usize;
        let height = glyph.height as usize;
        let row_bytes = (width + 7) / 8;
        if width > 0 && height > 0 && glyph.bitmap.len() >= row_bytes * height {
            // PK offsets place the bitmap's upper-left corner relative to the
            // character origin, positive y upwards.
            let left = -(glyph.hoff as f64) * unit;
            let top = glyph.voff as f64 * unit;

            for row in 0..height {
                let row_start = row * row_bytes;
                let top_y = top - row as f64 * unit;
                let bottom_y = top_y - unit;
                let mut column = 0;
                while column < width {
                    if !bit_is_set(&glyph.bitmap, row_start, column) {
                        column += 1;
                        continue;
                    }
                    let run_start = column;
                    while column < width && bit_is_set(&glyph.bitmap, row_start, column) {
                        column += 1;
                    }
                    let left_x = left + run_start as f64 * unit;
                    let right_x = left + column as f64 * unit;
                    path.move_to(left_x, bottom_y);
                    path.line_to(right_x, bottom_y);
                    path.line_to(right_x, top_y);
                    path.line_to(left_x, top_y);
                    path.close_path();
                }
            }
        }

        Some((path, advance))
    }
}

fn bit_is_set(bitmap: &[u8], row_start: usize, column: usize) -> bool {
    let byte = bitmap[row_start + column / 8];
    (byte >> (7 - (column % 8))) & 1 != 0
}

/// PK font for a font name, cached per name (including misses).
pub fn load_pk_font(name: &str) -> Option<Arc<PkFont>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<PkFont>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = name.to_lowercase();

    if let Ok(guard) = cache.lock() {
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }

    let font = locate_pk(name).and_then(|path| {
        let data = std::fs::read(&path).ok()?;
        PkFont::parse(&data).map(Arc::new)
    });

    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, font.clone());
    }
    font
}

fn locate_pk(name: &str) -> Option<PathBuf> {
    let mut base_names = vec![name.to_string()];
    let lower = name.to_lowercase();
    if lower != name {
        base_names.push(lower);
    }

    for base in base_names {
        for extension in ["600pk", "pk", "1200pk", "300pk", "2400pk"] {
            if let Some(path) = find_with_kpsewhich(&format!("{base}.{extension}")) {
                return Some(path);
            }
        }
    }
    None
}

fn find_with_kpsewhich(filename: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("kpsewhich")
        .arg("--format=pk")
        .arg(filename)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    path.exists().then_some(path)
}

fn decode_raw(raster: &[u8], width: u32, height: u32) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let row_bytes = (width + 7) / 8;
    let mut bitmap = vec![0u8; row_bytes * height];
    let total_bits = width * height;

    for bit in 0..total_bits {
        let byte = match raster.get(bit / 8) {
            Some(byte) => *byte,
            None => break,
        };
        if (byte >> (7 - bit % 8)) & 1 == 0 {
            continue;
        }
        let row = bit / width;
        let column = bit % width;
        bitmap[row * row_bytes + column / 8] |= 1 << (7 - column % 8);
    }
    bitmap
}

fn decode_packed(
    raster: &[u8],
    dynamic: usize,
    black_first: bool,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let row_bytes = (width + 7) / 8;
    let mut bitmap = vec![0u8; row_bytes * height];

    let mut nybbles = Nybbles::new(raster);
    let mut black = black_first;
    let mut row_bits = vec![false; width];
    let mut column = 0;
    let mut row = 0;
    let mut repeat = 0;

    while row < height {
        let Some(first) = nybbles.read() else {
            break;
        };
        if first == 14 {
            let Some(count) = decode_number(&mut nybbles, dynamic) else {
                break;
            };
            repeat = count;
            continue;
        }
        if first == 15 {
            repeat = 1;
            continue;
        }
        let Some(count) = decode_number_from(first, &mut nybbles, dynamic) else {
            break;
        };

        let mut remaining = count;
        while remaining > 0 && row < height {
            let put = remaining.min(width - column);
            if black {
                for index in column..column + put {
                    row_bits[index] = true;
                }
            }
            column += put;
            remaining -= put;
            if column >= width {
                flush_row(
                    &mut bitmap,
                    row_bytes,
                    &row_bits,
                    &mut row,
                    height,
                    &mut repeat,
                );
                row_bits.fill(false);
                column = 0;
            }
        }
        black = !black;
    }

    bitmap
}

fn flush_row(
    bitmap: &mut [u8],
    row_bytes: usize,
    row_bits: &[bool],
    row: &mut usize,
    height: usize,
    repeat: &mut usize,
) {
    for _ in 0..*repeat + 1 {
        if *row >= height {
            break;
        }
        for (column, is_black) in row_bits.iter().enumerate() {
            if *is_black {
                bitmap[*row * row_bytes + column / 8] |= 1 << (7 - column % 8);
            }
        }
        *row += 1;
    }
    *repeat = 0;
}

fn decode_number_from(first: u8, nybbles: &mut Nybbles, dynamic: usize) -> Option<usize> {
    let value = first as i64;
    if value == 0 {
        let mut leading = 1i64;
        let mut next = nybbles.read()? as i64;
        while next == 0 {
            leading += 1;
            next = nybbles.read()? as i64;
        }
        let mut number = next;
        for _ in 0..leading {
            number = (number << 4) + nybbles.read()? as i64;
        }
        let decoded = number - 15 + (13 - dynamic as i64) * 16 + dynamic as i64;
        return (decoded >= 0).then_some(decoded as usize);
    }
    if value <= dynamic as i64 {
        return Some(value as usize);
    }
    if value < 14 {
        let extra = nybbles.read()? as i64;
        return Some(((value - dynamic as i64 - 1) * 16 + extra + dynamic as i64 + 1).max(0) as usize);
    }
    None
}

fn decode_number(nybbles: &mut Nybbles, dynamic: usize) -> Option<usize> {
    let first = nybbles.read()?;
    decode_number_from(first, nybbles, dynamic)
}

struct Nybbles<'a> {
    data: &'a [u8],
    index: usize,
    high: bool,
}

impl<'a> Nybbles<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            index: 0,
            high: true,
        }
    }

    fn read(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.index)?;
        if self.high {
            self.high = false;
            Some(byte >> 4)
        } else {
            self.high = true;
            self.index += 1;
            Some(byte & 0x0F)
        }
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.position)?;
        self.position += 1;
        Some(byte)
    }

    fn unsigned(&mut self, count: usize) -> Option<u32> {
        let end = self.position.checked_add(count)?;
        let bytes = self.data.get(self.position..end)?;
        self.position = end;
        let mut value = 0u32;
        for byte in bytes {
            value = (value << 8) | *byte as u32;
        }
        Some(value)
    }

    fn signed(&mut self, count: usize) -> Option<i32> {
        let value = self.unsigned(count)? as i64;
        let shift = 64 - count * 8;
        Some(((value << shift) >> shift) as i32)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.take(count).map(|_| ())
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(count)?;
        let bytes = self.data.get(self.position..end)?;
        self.position = end;
        Some(bytes)
    }
}
