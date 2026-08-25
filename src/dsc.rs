#[derive(Debug, Clone, PartialEq)]
pub struct BoundingBox {
    pub llx: f64,
    pub lly: f64,
    pub urx: f64,
    pub ury: f64,
}

impl BoundingBox {
    pub fn width(&self) -> f64 {
        (self.urx - self.llx).abs()
    }

    pub fn height(&self) -> f64 {
        (self.ury - self.lly).abs()
    }
}

#[derive(Debug, Clone)]
pub struct DscPage {
    pub label: String,
    pub ordinal: usize,
    pub start_byte_offset: usize,
    pub end_byte_offset: usize,
}

#[derive(Debug, Clone)]
pub struct DscDocument {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub bounding_box: Option<BoundingBox>,
    pub hires_bounding_box: Option<BoundingBox>,
    pub total_pages: Option<usize>,
    pub prolog_range: Option<(usize, usize)>,
    pub setup_range: Option<(usize, usize)>,
    pub preamble_range: Option<(usize, usize)>,
    pub pages: Vec<DscPage>,
}

impl DscDocument {
    pub fn parse(input: &[u8]) -> Self {
        let text = String::from_utf8_lossy(input);
        let mut title = None;
        let mut creator = None;
        let mut bounding_box = None;
        let mut hires_bounding_box = None;
        let mut total_pages = None;
        let mut pages = Vec::new();

        let mut prolog_start = None;
        let mut prolog_end = None;
        let mut setup_start = None;
        let mut setup_end = None;

        let mut current_page: Option<(String, usize, usize)> = None;

        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let line_len = line.len();
            let trimmed = line.trim();

            if trimmed.starts_with("%%Title:") {
                title = Some(trimmed["%%Title:".len()..].trim().to_string());
            } else if trimmed.starts_with("%%Creator:") {
                creator = Some(trimmed["%%Creator:".len()..].trim().to_string());
            } else if trimmed.starts_with("%%BoundingBox:") {
                let parts: Vec<&str> = trimmed["%%BoundingBox:".len()..].split_whitespace().collect();
                if parts.len() == 4 {
                    if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (
                        parts[0].parse::<f64>(),
                        parts[1].parse::<f64>(),
                        parts[2].parse::<f64>(),
                        parts[3].parse::<f64>(),
                    ) {
                        bounding_box = Some(BoundingBox { llx: x1, lly: y1, urx: x2, ury: y2 });
                    }
                }
            } else if trimmed.starts_with("%%HiResBoundingBox:") {
                let parts: Vec<&str> = trimmed["%%HiResBoundingBox:".len()..].split_whitespace().collect();
                if parts.len() == 4 {
                    if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (
                        parts[0].parse::<f64>(),
                        parts[1].parse::<f64>(),
                        parts[2].parse::<f64>(),
                        parts[3].parse::<f64>(),
                    ) {
                        hires_bounding_box = Some(BoundingBox { llx: x1, lly: y1, urx: x2, ury: y2 });
                    }
                }
            } else if trimmed.starts_with("%%Pages:") {
                let p = trimmed["%%Pages:".len()..].trim();
                if let Ok(n) = p.parse::<usize>() {
                    total_pages = Some(n);
                }
            } else if trimmed.starts_with("%%BeginProlog") {
                prolog_start = Some(offset);
            } else if trimmed.starts_with("%%EndProlog") {
                prolog_end = Some(offset + line_len);
            } else if trimmed.starts_with("%%BeginSetup") {
                setup_start = Some(offset);
            } else if trimmed.starts_with("%%EndSetup") {
                setup_end = Some(offset + line_len);
            } else if trimmed.starts_with("%%Page:") {
                if let Some((label, ord, start)) = current_page.take() {
                    pages.push(DscPage {
                        label,
                        ordinal: ord,
                        start_byte_offset: start,
                        end_byte_offset: offset,
                    });
                }

                let parts: Vec<&str> = trimmed["%%Page:".len()..].split_whitespace().collect();
                let label = parts.first().unwrap_or(&"?").to_string();
                let ord = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(pages.len() + 1);
                current_page = Some((label, ord, offset));
            } else if trimmed.starts_with("%%Trailer") || trimmed.starts_with("%%EOF") {
                if let Some((label, ord, start)) = current_page.take() {
                    pages.push(DscPage {
                        label,
                        ordinal: ord,
                        start_byte_offset: start,
                        end_byte_offset: offset,
                    });
                }
            }

            offset += line_len;
        }

        if let Some((label, ord, start)) = current_page.take() {
            pages.push(DscPage {
                label,
                ordinal: ord,
                start_byte_offset: start,
                end_byte_offset: input.len(),
            });
        }

        let prolog_range = match (prolog_start, prolog_end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        };

        let setup_range = match (setup_start, setup_end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        };

        let preamble_range = pages.first().map(|p| (0, p.start_byte_offset));

        Self {
            title,
            creator,
            bounding_box,
            hires_bounding_box,
            total_pages,
            prolog_range,
            setup_range,
            preamble_range,
            pages,
        }
    }
}
