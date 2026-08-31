use crate::error::{PsError, PsResult};
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Value(Value),
    LeftBrace,   // {
    RightBrace,  // }
    LeftBracket, // [
    RightBracket,// ]
    LeftDict,    // <<
    RightDict,   // >>
}

pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn remaining_bytes(&self) -> &'a [u8] {
        if self.pos < self.input.len() {
            &self.input[self.pos..]
        } else {
            &[]
        }
    }

    pub fn read_hex_bytes(&mut self, count: usize) -> (Vec<u8>, bool) {
        let mut result = Vec::with_capacity(count);
        let mut first_nibble = None;

        while self.pos < self.input.len() && result.len() < count {
            let b = self.input[self.pos];
            self.pos += 1;

            if b.is_ascii_whitespace() {
                continue;
            }
            if b == b'%' {
                while self.pos < self.input.len() && self.input[self.pos] != b'\n' && self.input[self.pos] != b'\r' {
                    self.pos += 1;
                }
                continue;
            }

            let hex_val = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => {
                    self.pos -= 1;
                    break;
                }
            };

            if let Some(first) = first_nibble.take() {
                result.push((first << 4) | hex_val);
            } else {
                first_nibble = Some(hex_val);
            }
        }

        if let Some(first) = first_nibble {
            if result.len() < count {
                result.push(first << 4);
            }
        }

        let is_not_eof = self.pos < self.input.len();
        (result, is_not_eof)
    }

    pub fn next_token(&mut self) -> PsResult<Option<Token>> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.input.len() {
            return Ok(None);
        }

        let b = self.input[self.pos];
        match b {
            b'{' => {
                self.pos += 1;
                Ok(Some(Token::LeftBrace))
            }
            b'}' => {
                self.pos += 1;
                Ok(Some(Token::RightBrace))
            }
            b'[' => {
                self.pos += 1;
                Ok(Some(Token::LeftBracket))
            }
            b']' => {
                self.pos += 1;
                Ok(Some(Token::RightBracket))
            }
            b'<' => {
                if self.pos + 1 < self.input.len() && self.input[self.pos + 1] == b'<' {
                    self.pos += 2;
                    Ok(Some(Token::LeftDict))
                } else if self.pos + 1 < self.input.len() && self.input[self.pos + 1] == b'~' {
                    self.read_ascii85_string()
                } else {
                    self.read_hex_string()
                }
            }
            b'>' => {
                if self.pos + 1 < self.input.len() && self.input[self.pos + 1] == b'>' {
                    self.pos += 2;
                    Ok(Some(Token::RightDict))
                } else {
                    self.pos += 1;
                    Err(PsError::SyntaxError("unexpected '>' delimiter".to_string()))
                }
            }
            b'(' => self.read_parenthesized_string(),
            b'/' => {
                self.pos += 1;
                let is_immediate = if self.pos < self.input.len() && self.input[self.pos] == b'/' {
                    self.pos += 1;
                    true
                } else {
                    false
                };
                let name = self.read_name_string()?;
                if is_immediate {
                    Ok(Some(Token::Value(Value::ImmediateName(name))))
                } else {
                    Ok(Some(Token::Value(Value::LiteralName(name))))
                }
            }
            _ => {
                let s = self.read_name_string()?;
                if let Some(val) = Self::parse_number(&s) {
                    Ok(Some(Token::Value(val)))
                } else if s == "true" {
                    Ok(Some(Token::Value(Value::Bool(true))))
                } else if s == "false" {
                    Ok(Some(Token::Value(Value::Bool(false))))
                } else if s == "null" {
                    Ok(Some(Token::Value(Value::Null)))
                } else {
                    Ok(Some(Token::Value(Value::Name(s))))
                }
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == 0 || b == 0x0C {
                self.pos += 1;
            } else if b == b'%' {
                // Skip comment to end of line
                while self.pos < self.input.len() && self.input[self.pos] != b'\r' && self.input[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn is_delimiter(b: u8) -> bool {
        matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
    }

    fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0 | 0x0C)
    }

    fn read_name_string(&mut self) -> PsResult<String> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if Self::is_whitespace(b) || Self::is_delimiter(b) {
                break;
            }
            self.pos += 1;
        }
        if start == self.pos {
            return Err(PsError::SyntaxError("empty name".to_string()));
        }
        Ok(String::from_utf8_lossy(&self.input[start..self.pos]).to_string())
    }

    fn read_parenthesized_string(&mut self) -> PsResult<Option<Token>> {
        self.pos += 1; // skip '('
        let mut depth = 1;
        let mut bytes = Vec::new();

        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            self.pos += 1;
            if b == b'\\' {
                if self.pos >= self.input.len() {
                    break;
                }
                let next = self.input[self.pos];
                self.pos += 1;
                match next {
                    b'n' => bytes.push(b'\n'),
                    b'r' => bytes.push(b'\r'),
                    b't' => bytes.push(b'\t'),
                    b'b' => bytes.push(0x08),
                    b'f' => bytes.push(0x0C),
                    b'\\' => bytes.push(b'\\'),
                    b'(' => bytes.push(b'('),
                    b')' => bytes.push(b')'),
                    b'\r' | b'\n' => {
                        // Escaped newline ignored
                        if next == b'\r' && self.pos < self.input.len() && self.input[self.pos] == b'\n' {
                            self.pos += 1;
                        }
                    }
                    b'0'..=b'7' => {
                        // Octal escape up to 3 digits
                        let mut oct_val = (next - b'0') as u32;
                        for _ in 0..2 {
                            if self.pos < self.input.len() && matches!(self.input[self.pos], b'0'..=b'7') {
                                oct_val = (oct_val << 3) + (self.input[self.pos] - b'0') as u32;
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                        bytes.push((oct_val & 0xFF) as u8);
                    }
                    other => bytes.push(other),
                }
            } else if b == b'(' {
                depth += 1;
                bytes.push(b'(');
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(Token::Value(Value::String(bytes))));
                }
                bytes.push(b')');
            } else {
                bytes.push(b);
            }
        }
        Err(PsError::SyntaxError("unterminated string".to_string()))
    }

    fn read_hex_string(&mut self) -> PsResult<Option<Token>> {
        self.pos += 1; // skip '<'
        let mut hex_digits = Vec::new();
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if Self::is_whitespace(b) {
                continue;
            }
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(PsError::SyntaxError(format!("invalid hex character: {:x}", b))),
            };
            hex_digits.push(digit);
        }

        let mut bytes = Vec::with_capacity((hex_digits.len() + 1) / 2);
        for chunk in hex_digits.chunks(2) {
            if chunk.len() == 2 {
                bytes.push((chunk[0] << 4) | chunk[1]);
            } else {
                bytes.push(chunk[0] << 4);
            }
        }
        Ok(Some(Token::Value(Value::String(bytes))))
    }

    fn read_ascii85_string(&mut self) -> PsResult<Option<Token>> {
        self.pos += 2; // skip '<~'
        let mut bytes = Vec::new();
        let mut tuple: u32 = 0;
        let mut count = 0;

        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            self.pos += 1;
            if b == b'~' && self.pos < self.input.len() && self.input[self.pos] == b'>' {
                self.pos += 1;
                if count > 0 {
                    for _ in count..5 {
                        tuple = tuple * 85 + 84;
                    }
                    for i in 0..(count - 1) {
                        bytes.push(((tuple >> ((3 - i) * 8)) & 0xFF) as u8);
                    }
                }
                return Ok(Some(Token::Value(Value::String(bytes))));
            }
            if Self::is_whitespace(b) {
                continue;
            }
            if b == b'z' && count == 0 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            if !(b'!'..=b'u').contains(&b) {
                return Err(PsError::SyntaxError("invalid ASCII85 char".to_string()));
            }
            tuple = tuple * 85 + (b - b'!') as u32;
            count += 1;
            if count == 5 {
                bytes.push(((tuple >> 24) & 0xFF) as u8);
                bytes.push(((tuple >> 16) & 0xFF) as u8);
                bytes.push(((tuple >> 8) & 0xFF) as u8);
                bytes.push((tuple & 0xFF) as u8);
                tuple = 0;
                count = 0;
            }
        }
        Err(PsError::SyntaxError("unterminated ASCII85 string".to_string()))
    }

    fn parse_number(s: &str) -> Option<Value> {
        if s.is_empty() {
            return None;
        }

        // Radix numbers: 16#FF, 8#77, 2#1010
        if let Some((radix_str, val_str)) = s.split_once('#') {
            if let Ok(radix) = radix_str.parse::<u32>() {
                if (2..=36).contains(&radix) {
                    if let Ok(val) = i64::from_str_radix(val_str, radix) {
                        return Some(Value::Integer(val));
                    }
                }
            }
        }

        // Integer
        if let Ok(i) = s.parse::<i64>() {
            return Some(Value::Integer(i));
        }

        // Float / Real
        if let Ok(f) = s.parse::<f64>() {
            if f.is_finite() {
                return Some(Value::Real(f));
            }
        }

        None
    }
}
