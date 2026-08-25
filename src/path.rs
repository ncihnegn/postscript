use crate::matrix::Matrix2D;

#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    CurveTo(f64, f64, f64, f64, f64, f64), // cp1, cp2, endpoint
    ClosePath,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub segments: Vec<PathSegment>,
}

impl Path {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn clear(&mut self) {
        self.segments.clear();
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        if let Some(PathSegment::MoveTo(..)) = self.segments.last() {
            *self.segments.last_mut().unwrap() = PathSegment::MoveTo(x, y);
        } else {
            if let Some(last) = self.segments.last() {
                if *last != PathSegment::ClosePath {
                    self.segments.push(PathSegment::ClosePath);
                }
            }
            self.segments.push(PathSegment::MoveTo(x, y));
        }
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.segments.push(PathSegment::LineTo(x, y));
    }

    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.segments
            .push(PathSegment::CurveTo(x1, y1, x2, y2, x3, y3));
    }

    pub fn close_path(&mut self) {
        self.segments.push(PathSegment::ClosePath);
    }

    pub fn transform(&self, matrix: &Matrix2D) -> Self {
        let mut new_path = Path::new();
        for seg in &self.segments {
            match *seg {
                PathSegment::MoveTo(x, y) => {
                    let (tx, ty) = matrix.transform_point(x, y);
                    new_path.move_to(tx, ty);
                }
                PathSegment::LineTo(x, y) => {
                    let (tx, ty) = matrix.transform_point(x, y);
                    new_path.line_to(tx, ty);
                }
                PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => {
                    let (tx1, ty1) = matrix.transform_point(x1, y1);
                    let (tx2, ty2) = matrix.transform_point(x2, y2);
                    let (tx3, ty3) = matrix.transform_point(x3, y3);
                    new_path.curve_to(tx1, ty1, tx2, ty2, tx3, ty3);
                }
                PathSegment::ClosePath => {
                    new_path.close_path();
                }
            }
        }
        new_path
    }

    pub fn append(&mut self, other: &Path) {
        self.segments.extend_from_slice(&other.segments);
    }

    pub fn arc(&mut self, cx: f64, cy: f64, r: f64, angle1: f64, angle2: f64, clockwise: bool) {
        // Approximate circular arc with cubic Bézier splines
        let a1 = angle1.to_radians();
        let mut a2 = angle2.to_radians();

        if !clockwise && a2 < a1 {
            while a2 < a1 {
                a2 += std::f64::consts::TAU;
            }
        } else if clockwise && a2 > a1 {
            while a2 > a1 {
                a2 -= std::f64::consts::TAU;
            }
        }

        let total_sweep = a2 - a1;
        let num_segments = (total_sweep.abs() / (std::f64::consts::PI / 2.0)).ceil().max(1.0) as usize;
        let step = total_sweep / (num_segments as f64);

        let start_x = cx + r * a1.cos();
        let start_y = cy + r * a1.sin();

        if self.segments.is_empty() {
            self.move_to(start_x, start_y);
        } else {
            self.line_to(start_x, start_y);
        }

        let mut current_angle = a1;
        for _ in 0..num_segments {
            let next_angle = current_angle + step;
            let half_step = step / 2.0;
            let k = (4.0 / 3.0) * ((half_step / 2.0).tan());

            let p0x = cx + r * current_angle.cos();
            let p0y = cy + r * current_angle.sin();
            let p3x = cx + r * next_angle.cos();
            let p3y = cy + r * next_angle.sin();

            let dx0 = -r * current_angle.sin();
            let dy0 = r * current_angle.cos();
            let dx3 = -r * next_angle.sin();
            let dy3 = r * next_angle.cos();

            let p1x = p0x + k * dx0;
            let p1y = p0y + k * dy0;
            let p2x = p3x - k * dx3;
            let p2y = p3y - k * dy3;

            self.curve_to(p1x, p1y, p2x, p2y, p3x, p3y);
            current_angle = next_angle;
        }
    }
}
