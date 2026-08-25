use crate::error::PsResult;
use crate::font::charstring::{CharStringInterpreter, GlyphOutline};
use std::collections::HashMap;

pub struct Type1Parser;

impl Type1Parser {
    pub fn parse_eexec_data(
        data: &[u8],
        _font_name: &str,
    ) -> PsResult<(Vec<Vec<u8>>, HashMap<String, GlyphOutline>)> {
        let mut subrs: Vec<Vec<u8>> = Vec::new();
        let mut charstrings: HashMap<String, GlyphOutline> = HashMap::new();

        // 1. Scan for Subrs
        if let Some(subrs_pos) = Self::find_bytes(data, b"/Subrs") {
            let mut pos = subrs_pos + 6;
            while pos < data.len() {
                if data[pos..].starts_with(b"/CharStrings") || data[pos..].starts_with(b"end") {
                    break;
                }
                if data[pos..].starts_with(b"dup") {
                    pos += 3;
                    // Read index
                    let (idx, new_pos) = Self::read_int(data, pos);
                    pos = new_pos;
                    // Read length
                    let (len, new_pos) = Self::read_int(data, pos);
                    pos = new_pos;
                    // Skip RD / -| / |
                    pos = Self::skip_rd(data, pos);
                    if pos + len <= data.len() {
                        let subr_bytes = data[pos..pos + len].to_vec();
                        if idx >= subrs.len() {
                            subrs.resize(idx + 1, Vec::new());
                        }
                        subrs[idx] = subr_bytes;
                        pos += len;
                    }
                } else {
                    pos += 1;
                }
            }
        }

        // 2. Scan for CharStrings
        let interpreter = CharStringInterpreter::new(&subrs, 4);

        if let Some(cs_pos) = Self::find_bytes(data, b"/CharStrings") {
            let mut pos = cs_pos + 12;
            while pos < data.len() {
                if data[pos..].starts_with(b"end") {
                    break;
                }
                if data[pos] == b'/' {
                    pos += 1;
                    let glyph_name = Self::read_name(data, &mut pos);
                    let (len, new_pos) = Self::read_int(data, pos);
                    pos = new_pos;
                    pos = Self::skip_rd(data, pos);
                    if pos + len <= data.len() {
                        let cs_bytes = &data[pos..pos + len];
                        if let Ok(outline) = interpreter.interpret(cs_bytes) {
                            charstrings.insert(glyph_name, outline);
                        }
                        pos += len;
                    }
                } else {
                    pos += 1;
                }
            }
        }

        Ok((subrs, charstrings))
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn read_name(data: &[u8], pos: &mut usize) -> String {
        let start = *pos;
        while *pos < data.len() {
            let b = data[*pos];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'/' || b == b'{' || b == b'}' {
                break;
            }
            *pos += 1;
        }
        String::from_utf8_lossy(&data[start..*pos]).to_string()
    }

    fn read_int(data: &[u8], mut pos: usize) -> (usize, usize) {
        while pos < data.len() && (data[pos] == b' ' || data[pos] == b'\t' || data[pos] == b'\r' || data[pos] == b'\n') {
            pos += 1;
        }
        let mut val = 0;
        while pos < data.len() && data[pos].is_ascii_digit() {
            val = val * 10 + (data[pos] - b'0') as usize;
            pos += 1;
        }
        (val, pos)
    }

    fn skip_rd(data: &[u8], mut pos: usize) -> usize {
        while pos < data.len() && (data[pos] == b' ' || data[pos] == b'\t' || data[pos] == b'\r' || data[pos] == b'\n') {
            pos += 1;
        }
        if pos + 2 <= data.len() && (data[pos..pos + 2] == *b"RD" || data[pos..pos + 2] == *b"-|") {
            pos += 2;
        } else if pos < data.len() && data[pos] == b'|' {
            pos += 1;
        }
        // Skip exactly 1 whitespace after RD delimiter
        if pos < data.len() && (data[pos] == b' ' || data[pos] == b'\t' || data[pos] == b'\r' || data[pos] == b'\n') {
            pos += 1;
        }
        pos
    }
}
