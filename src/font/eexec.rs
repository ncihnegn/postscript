/// PostScript eexec and CharString cipher implementation (Adobe Type 1 Font Format)

pub struct Type1Cipher {
    r: u16,
    c1: u16,
    c2: u16,
}

impl Type1Cipher {
    /// Standard eexec cipher parameters (R = 55665, c1 = 52845, c2 = 22719)
    pub fn new_eexec() -> Self {
        Self {
            r: 55665,
            c1: 52845,
            c2: 22719,
        }
    }

    /// Standard CharStrings cipher parameters (R = 4330, c1 = 52845, c2 = 22719)
    pub fn new_charstring() -> Self {
        Self {
            r: 4330,
            c1: 52845,
            c2: 22719,
        }
    }

    pub fn decrypt_byte(&mut self, cipher: u8) -> u8 {
        let plain = cipher ^ ((self.r >> 8) as u8);
        self.r = ((cipher as u16).wrapping_add(self.r).wrapping_mul(self.c1)).wrapping_add(self.c2);
        plain
    }

    /// Decrypts a stream of bytes, discarding the first `lenIV` (typically 4) random bytes.
    pub fn decrypt(&mut self, data: &[u8], len_iv: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(data.len().saturating_sub(len_iv));
        for (i, &b) in data.iter().enumerate() {
            let plain = self.decrypt_byte(b);
            if i >= len_iv {
                output.push(plain);
            }
        }
        output
    }

    /// Helper to decrypt hex-encoded or binary eexec data
    pub fn decrypt_eexec(input: &[u8]) -> Vec<u8> {
        let mut cipher = Self::new_eexec();
        // Check if data is ASCII hex encoded
        let is_hex = input.iter().take(32).all(|&b| {
            b.is_ascii_hexdigit() || b == b' ' || b == b'\t' || b == b'\r' || b == b'\n'
        });

        if is_hex {
            let mut hex_bytes = Vec::new();
            let mut high_nibble: Option<u8> = None;
            for &b in input {
                let nibble = match b {
                    b'0'..=b'9' => b - b'0',
                    b'a'..=b'f' => b - b'a' + 10,
                    b'A'..=b'F' => b - b'A' + 10,
                    _ => continue,
                };
                if let Some(h) = high_nibble.take() {
                    hex_bytes.push((h << 4) | nibble);
                } else {
                    high_nibble = Some(nibble);
                }
            }
            cipher.decrypt(&hex_bytes, 4)
        } else {
            cipher.decrypt(input, 4)
        }
    }
}
