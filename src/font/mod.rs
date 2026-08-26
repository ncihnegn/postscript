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
        let glyph = self.charstrings.get(glyph_name)
            .or_else(|| {
                let alias = match glyph_name {
                    "0" => "zero", "1" => "one", "2" => "two", "3" => "three", "4" => "four",
                    "5" => "five", "6" => "six", "7" => "seven", "8" => "eight", "9" => "nine",
                    "." => "period", "," => "comma", "-" => "hyphen", ":" => "colon", ";" => "semicolon",
                    "!" => "exclam", "?" => "question", "(" => "parenleft", ")" => "parenright",
                    "[" => "bracketleft", "]" => "bracketright", "{" => "braceleft", "}" => "braceright",
                    "/" => "slash", "\\" => "backslash", "=" => "equal", "+" => "plus", "*" => "asterisk",
                    " " => "space", "_" => "underscore", "@" => "at", "'" => "quoteright", "`" => "quoteleft",
                    "\"" => "quotedbl", "<" => "less", ">" => "greater", "#" => "numbersign",
                    "$" => "dollar", "%" => "percent", "&" => "ampersand", "^" => "asciicircum",
                    "~" => "asciitilde", "|" => "bar",
                    "zero" => "0", "one" => "1", "two" => "2", "three" => "3", "four" => "4",
                    "five" => "5", "six" => "6", "seven" => "7", "eight" => "8", "nine" => "9",
                    "period" => ".", "comma" => ",", "hyphen" => "-", "minus" => "-", "colon" => ":",
                    "semicolon" => ";", "exclam" => "!", "question" => "?",
                    "parenleft" => "(", "parenright" => ")",
                    "bracketleft" => "[", "bracketright" => "]",
                    "braceleft" => "{", "braceright" => "}",
                    "slash" => "/", "backslash" => "\\", "equal" => "=", "plus" => "+",
                    "asterisk" => "*", "space" => " ", "underscore" => "_",
                    _ => return None,
                };
                self.charstrings.get(alias)
            });

        if let Some(g) = glyph {
            let transformed_path = g.path.transform(&self.matrix);
            let (scaled_width, _) = self.matrix.transform_vector(g.width, 0.0);
            Some((transformed_path, scaled_width.abs()))
        } else {
            None
        }
    }
}

pub fn load_font_by_name(name: &str) -> Option<FontFace> {
    let filename = map_font_name_to_filename(name);
    let path = find_font_file(&filename)?;
    let data = std::fs::read(&path).ok()?;
    if let Ok((_font_name, parsed_encoding, subrs, charstrings)) = Type1Parser::parse_pfb(&data) {
        let lower = name.to_lowercase();
        let encoding: Vec<String> = if lower.ends_with("8r") {
            TEX_BASE1_ENCODING.iter().map(|s| s.to_string()).collect()
        } else if lower.ends_with("8t") {
            CORK_T1_ENCODING.iter().map(|s| s.to_string()).collect()
        } else {
            parsed_encoding
        };

        Some(FontFace {
            name: name.to_string(),
            matrix: Matrix2D::scale(0.001, 0.001),
            encoding,
            charstrings,
            subrs,
        })
    } else {
        None
    }
}

pub const TEX_BASE1_ENCODING: [&str; 256] = [
    ".notdef", "dotaccent", "fi", "fl", "fraction", "hungarumlaut", "Lslash", "lslash",
    "ogonek", "ring", ".notdef", "breve", "minus", ".notdef", "Zcaron", "zcaron",
    "caron", "dotlessi", "dotlessj", "ff", "ffi", "ffl", "notequal", "infinity",
    "lessequal", "greaterequal", "partialdiff", "summation", "product", "pi", "grave", "quotesingle",
    "space", "exclam", "quotedbl", "numbersign", "dollar", "percent", "ampersand", "quoteright",
    "parenleft", "parenright", "asterisk", "plus", "comma", "hyphen", "period", "slash",
    "zero", "one", "two", "three", "four", "five", "six", "seven",
    "eight", "nine", "colon", "semicolon", "less", "equal", "greater", "question",
    "at", "A", "B", "C", "D", "E", "F", "G",
    "H", "I", "J", "K", "L", "M", "N", "O",
    "P", "Q", "R", "S", "T", "U", "V", "W",
    "X", "Y", "Z", "bracketleft", "backslash", "bracketright", "asciicircum", "underscore",
    "quoteleft", "a", "b", "c", "d", "e", "f", "g",
    "h", "i", "j", "k", "l", "m", "n", "o",
    "p", "q", "r", "s", "t", "u", "v", "w",
    "x", "y", "z", "braceleft", "bar", "braceright", "asciitilde", ".notdef",
    "Euro", "integral", "quotesinglbase", "florin", "quotedblbase", "ellipsis", "dagger", "daggerdbl",
    "circumflex", "perthousand", "Scaron", "guilsinglleft", "OE", "Omega", "radical", "approxequal",
    ".notdef", ".notdef", ".notdef", "quotedblleft", "quotedblright", "bullet", "endash", "emdash",
    "tilde", "trademark", "scaron", "guilsinglright", "oe", "Delta", "lozenge", "Ydieresis",
    ".notdef", "exclamdown", "cent", "sterling", "currency", "yen", "brokenbar", "section",
    "dieresis", "copyright", "ordfeminine", "guillemotleft", "logicalnot", "hyphen", "registered", "macron",
    "degree", "plusminus", "twosuperior", "threesuperior", "acute", "mu", "paragraph", "periodcentered",
    "cedilla", "onesuperior", "ordmasculine", "guillemotright", "onequarter", "onehalf", "threequarters", "questiondown",
    "Agrave", "Aacute", "Acircumflex", "Atilde", "Adieresis", "Aring", "AE", "Ccedilla",
    "Egrave", "Eacute", "Ecircumflex", "Edieresis", "Igrave", "Iacute", "Icircumflex", "Idieresis",
    "Eth", "Ntilde", "Ograve", "Oacute", "Ocircumflex", "Otilde", "Odieresis", "multiply",
    "Oslash", "Ugrave", "Uacute", "Ucircumflex", "Udieresis", "Yacute", "Thorn", "germandbls",
    "agrave", "aacute", "acircumflex", "atilde", "adieresis", "aring", "ae", "ccedilla",
    "egrave", "eacute", "ecircumflex", "edieresis", "igrave", "iacute", "icircumflex", "idieresis",
    "eth", "ntilde", "ograve", "oacute", "ocircumflex", "otilde", "odieresis", "divide",
    "oslash", "ugrave", "uacute", "ucircumflex", "udieresis", "yacute", "thorn", "ydieresis",
];

pub const CORK_T1_ENCODING: [&str; 256] = [
    ".notdef", "grave", "acute", "circumflex", "tilde", "dieresis", "hungarumlaut", "ring",
    "caron", "breve", "macron", "dotaccent", "cedilla", "ogonek", "quotesinglbase", "guilsinglleft",
    "guilsinglright", "quotedblleft", "quotedblright", "quotedblbase", "guillemotleft", "guillemotright", "endash", "emdash",
    "cwm", "perthousand", "dotlessi", "dotlessj", "ff", "fi", "fl", "ffi",
    "ffl", "space", "exclam", "quotedbl", "numbersign", "dollar", "percent", "ampersand",
    "quoteright", "parenleft", "parenright", "asterisk", "plus", "comma", "hyphen", "period",
    "slash", "zero", "one", "two", "three", "four", "five", "six",
    "seven", "eight", "nine", "colon", "semicolon", "less", "equal", "greater",
    "question", "at", "A", "B", "C", "D", "E", "F",
    "G", "H", "I", "J", "K", "L", "M", "N",
    "O", "P", "Q", "R", "S", "T", "U", "V",
    "W", "X", "Y", "Z", "bracketleft", "backslash", "bracketright", "asciicircum",
    "underscore", "quoteleft", "a", "b", "c", "d", "e", "f",
    "g", "h", "i", "j", "k", "l", "m", "n",
    "o", "p", "q", "r", "s", "t", "u", "v",
    "w", "x", "y", "z", "braceleft", "bar", "braceright", "asciitilde",
    "sfthyphen", "Abreve", "Aogonek", "Cacute", "Ccaron", "Dcaron", "Ecaron", "Eogonek",
    "Gbreve", "Lacute", "Lcaron", "Lslash", "Nacute", "Ncaron", "Eng", "Ohungarumlaut",
    "Racute", "Rcaron", "Sacute", "Scaron", "Scedilla", "Tcaron", "Tcedilla", "Uhungarumlaut",
    "Uring", "Ydieresis", "Zacute", "Zcaron", "Zdotaccent", "IJ", "Idotaccent", "dcroat",
    "section", "abreve", "aogonek", "cacute", "ccaron", "dcaron", "ecaron", "eogonek",
    "gbreve", "lacute", "lcaron", "lslash", "nacute", "ncaron", "eng", "ohungarumlaut",
    "racute", "rcaron", "sacute", "scaron", "scedilla", "tcaron", "tcedilla", "uhungarumlaut",
    "uring", "ydieresis", "zacute", "zcaron", "zdotaccent", "ij", "exclamdown", "questiondown",
    "sterling", "Agrave", "Aacute", "Acircumflex", "Atilde", "Adieresis", "Aring", "AE",
    "Ccedilla", "Egrave", "Eacute", "Ecircumflex", "Edieresis", "Igrave", "Iacute", "Icircumflex",
    "Idieresis", "Eth", "Ntilde", "Ograve", "Oacute", "Ocircumflex", "Otilde", "Odieresis",
    "OE", "Oslash", "Ugrave", "Uacute", "Ucircumflex", "Udieresis", "Yacute", "Thorn",
    "SS", "agrave", "aacute", "acircumflex", "atilde", "adieresis", "aring", "ae",
    "ccedilla", "egrave", "eacute", "ecircumflex", "edieresis", "igrave", "iacute", "icircumflex",
    "idieresis", "eth", "ntilde", "ograve", "oacute", "ocircumflex", "otilde", "odieresis",
    "oe", "oslash", "ugrave", "uacute", "ucircumflex", "udieresis", "yacute", "thorn",
];

fn map_font_name_to_filename(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.starts_with("phvbo") || lower.starts_with("helvetica-boldoblique") {
        "uhvbo8a.pfb".to_string()
    } else if lower.starts_with("phvro") || lower.starts_with("helvetica-oblique") {
        "uhvro8a.pfb".to_string()
    } else if lower.starts_with("phvb") || lower.starts_with("helvetica-bold") {
        "uhvb8a.pfb".to_string()
    } else if lower.starts_with("phv") || lower.starts_with("helvetica") {
        "uhvr8a.pfb".to_string()
    } else if lower.starts_with("ptmbi") || lower.starts_with("times-bolditalic") {
        "utmbi8a.pfb".to_string()
    } else if lower.starts_with("ptmri") || lower.starts_with("times-italic") {
        "utmri8a.pfb".to_string()
    } else if lower.starts_with("ptmb") || lower.starts_with("times-bold") {
        "utmb8a.pfb".to_string()
    } else if lower.starts_with("ptm") || lower.starts_with("times") {
        "utmr8a.pfb".to_string()
    } else if lower.starts_with("pcrb") || lower.starts_with("courier-bold") {
        "ucrb8a.pfb".to_string()
    } else if lower.starts_with("pcr") || lower.starts_with("courier") {
        "ucrr8a.pfb".to_string()
    } else if lower.starts_with("pzdr") || lower.starts_with("zapfdingbats") {
        "uzdr.pfb".to_string()
    } else if lower.starts_with("psyr") || lower.starts_with("symbol") {
        "usyr.pfb".to_string()
    } else if lower.ends_with(".pfb") {
        lower
    } else {
        format!("{}.pfb", lower)
    }
}

fn find_font_file(filename: &str) -> Option<std::path::PathBuf> {
    let search_dirs = [
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/urw/helvetic",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/urw/times",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/urw/courier",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/urw/dingbats",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/urw/symbol",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/public/amsfonts/cm",
        "/usr/local/texlive/2026/texmf-dist/fonts/type1/public/amsfonts/symbols",
        "/usr/local/texlive/2025/texmf-dist/fonts/type1/urw/helvetic",
        "/usr/local/texlive/2025/texmf-dist/fonts/type1/urw/times",
        "/Library/TeX/Root/texmf-dist/fonts/type1/urw/helvetic",
        "/Library/TeX/Root/texmf-dist/fonts/type1/urw/times",
    ];

    for dir in &search_dirs {
        let p = std::path::Path::new(dir).join(filename);
        if p.exists() {
            return Some(p);
        }
    }

    // Fallback using kpsewhich
    if let Ok(output) = std::process::Command::new("kpsewhich")
        .arg(filename)
        .output()
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !out_str.is_empty() {
                let p = std::path::PathBuf::from(out_str);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    None
}
