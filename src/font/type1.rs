use crate::error::{PsError, PsResult};
use crate::font::charstring::{CharStringInterpreter, GlyphOutline};
use crate::font::eexec::Type1Cipher;
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

    pub fn parse_pfb(data: &[u8]) -> PsResult<(String, Vec<String>, Vec<Vec<u8>>, HashMap<String, GlyphOutline>)> {
        let mut pos = 0;
        let mut ascii_header = Vec::new();
        let mut binary_eexec = Vec::new();

        while pos + 6 <= data.len() && data[pos] == 0x80 {
            let seg_type = data[pos + 1];
            if seg_type == 3 {
                break;
            }
            let seg_len = (data[pos + 2] as usize)
                | ((data[pos + 3] as usize) << 8)
                | ((data[pos + 4] as usize) << 16)
                | ((data[pos + 5] as usize) << 24);
            pos += 6;
            if pos + seg_len > data.len() {
                break;
            }
            let seg_data = &data[pos..pos + seg_len];
            if seg_type == 1 {
                ascii_header.extend_from_slice(seg_data);
            } else if seg_type == 2 {
                binary_eexec.extend_from_slice(seg_data);
            }
            pos += seg_len;
        }

        if binary_eexec.is_empty() {
            return Err(PsError::SyntaxError("No eexec segment in PFB".to_string()));
        }

        let mut cipher = Type1Cipher::new_eexec();
        let decrypted = cipher.decrypt(&binary_eexec, 4);
        let (subrs, charstrings) = Self::parse_eexec_data(&decrypted, "")?;

        let font_name = if let Some(fn_pos) = Self::find_bytes(&ascii_header, b"/FontName") {
            let mut p = fn_pos + 9;
            while p < ascii_header.len() && (ascii_header[p] == b' ' || ascii_header[p] == b'/') {
                p += 1;
            }
            Self::read_name(&ascii_header, &mut p)
        } else {
            "unnamed".to_string()
        };

        let mut encoding = vec![".notdef".to_string(); 256];
        for i in 32..=126 {
            encoding[i] = (i as u8 as char).to_string();
        }

        Ok((font_name, encoding, subrs, charstrings))
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
