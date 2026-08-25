use crate::error::{PsError, PsResult};
use crate::font::eexec::Type1Cipher;
use crate::path::Path;

pub struct CharStringInterpreter<'a> {
    subrs: &'a [Vec<u8>],
    len_iv: usize,
}

#[derive(Debug, Clone)]
pub struct GlyphOutline {
    pub path: Path,
    pub width: f64,
    pub lsb: f64,
}

impl<'a> CharStringInterpreter<'a> {
    pub fn new(subrs: &'a [Vec<u8>], len_iv: usize) -> Self {
        Self { subrs, len_iv }
    }

    pub fn interpret(&self, raw_bytes: &[u8]) -> PsResult<GlyphOutline> {
        let decrypted = if self.len_iv > 0 {
            let mut cipher = Type1Cipher::new_charstring();
            cipher.decrypt(raw_bytes, self.len_iv)
        } else {
            raw_bytes.to_vec()
        };

        let mut path = Path::new();
        let mut stack: Vec<f64> = Vec::new();
        let mut othersubr_results: Vec<f64> = Vec::new();
        let mut flex_points: Vec<(f64, f64)> = Vec::new();
        let mut in_flex = false;
        let mut cur_x = 0.0;
        let mut cur_y = 0.0;
        let mut width = 0.0;
        let mut lsb = 0.0;

        self.execute_bytecode(
            &decrypted,
            &mut stack,
            &mut othersubr_results,
            &mut flex_points,
            &mut in_flex,
            &mut path,
            &mut cur_x,
            &mut cur_y,
            &mut width,
            &mut lsb,
            0,
        )?;

        Ok(GlyphOutline {
            path,
            width,
            lsb,
        })
    }

    fn execute_bytecode(
        &self,
        bytecode: &[u8],
        stack: &mut Vec<f64>,
        othersubr_results: &mut Vec<f64>,
        flex_points: &mut Vec<(f64, f64)>,
        in_flex: &mut bool,
        path: &mut Path,
        cur_x: &mut f64,
        cur_y: &mut f64,
        width: &mut f64,
        lsb: &mut f64,
        call_depth: usize,
    ) -> PsResult<()> {
        if call_depth > 20 {
            return Err(PsError::LimitCheck("subroutine call depth exceeded".to_string()));
        }

        let mut pos = 0;
        while pos < bytecode.len() {
            let b0 = bytecode[pos];
            pos += 1;

            match b0 {
                // hstem (1), vstem (3)
                1 | 3 => {
                    stack.clear();
                }
                // vmoveto (4)
                4 => {
                    if let Some(dy) = stack.pop() {
                        *cur_y += dy;
                        path.move_to(*cur_x, *cur_y);
                    }
                    stack.clear();
                }
                // rlineto (5)
                5 => {
                    let mut i = 0;
                    while i + 1 < stack.len() {
                        let dx = stack[i];
                        let dy = stack[i + 1];
                        *cur_x += dx;
                        *cur_y += dy;
                        path.line_to(*cur_x, *cur_y);
                        i += 2;
                    }
                    stack.clear();
                }
                // hlineto (6)
                6 => {
                    let mut horiz = true;
                    for &d in stack.iter() {
                        if horiz {
                            *cur_x += d;
                        } else {
                            *cur_y += d;
                        }
                        path.line_to(*cur_x, *cur_y);
                        horiz = !horiz;
                    }
                    stack.clear();
                }
                // vlineto (7)
                7 => {
                    let mut vert = true;
                    for &d in stack.iter() {
                        if vert {
                            *cur_y += d;
                        } else {
                            *cur_x += d;
                        }
                        path.line_to(*cur_x, *cur_y);
                        vert = !vert;
                    }
                    stack.clear();
                }
                // rrcurveto (8)
                8 => {
                    let mut i = 0;
                    while i + 5 < stack.len() {
                        let dx1 = stack[i];
                        let dy1 = stack[i + 1];
                        let dx2 = stack[i + 2];
                        let dy2 = stack[i + 3];
                        let dx3 = stack[i + 4];
                        let dy3 = stack[i + 5];

                        let cp1x = *cur_x + dx1;
                        let cp1y = *cur_y + dy1;
                        let cp2x = cp1x + dx2;
                        let cp2y = cp1y + dy2;
                        *cur_x = cp2x + dx3;
                        *cur_y = cp2y + dy3;

                        path.curve_to(cp1x, cp1y, cp2x, cp2y, *cur_x, *cur_y);
                        i += 6;
                    }
                    stack.clear();
                }
                // closepath (9)
                9 => {
                    path.close_path();
                    stack.clear();
                }
                // callsubr (10)
                10 => {
                    if let Some(subr_num) = stack.pop() {
                        let idx = subr_num as isize;
                        if idx >= 0 && (idx as usize) < self.subrs.len() {
                            let subr_bytes = &self.subrs[idx as usize];
                            let decrypted = if self.len_iv > 0 {
                                let mut cipher = Type1Cipher::new_charstring();
                                cipher.decrypt(subr_bytes, self.len_iv)
                            } else {
                                subr_bytes.clone()
                            };
                            self.execute_bytecode(
                                &decrypted,
                                stack,
                                othersubr_results,
                                flex_points,
                                in_flex,
                                path,
                                cur_x,
                                cur_y,
                                width,
                                lsb,
                                call_depth + 1,
                            )?;
                        }
                    }
                }
                // return (11)
                11 => {
                    return Ok(());
                }
                // escape (12 xx)
                12 => {
                    if pos < bytecode.len() {
                        let b1 = bytecode[pos];
                        pos += 1;
                        match b1 {
                            0 => {} // dotsection
                            6 => {
                                // seac: asb, adx, ady, bchar, achar
                                stack.clear();
                            }
                            7 => {
                                // sbw: sbx, sby, wx, wy
                                if stack.len() >= 4 {
                                    *lsb = stack[0];
                                    *width = stack[2];
                                    *cur_x = stack[0];
                                    *cur_y = stack[1];
                                }
                                stack.clear();
                            }
                            12 => {
                                // div
                                if stack.len() >= 2 {
                                    let num2 = stack.pop().unwrap();
                                    let num1 = stack.pop().unwrap();
                                    if num2 != 0.0 {
                                        stack.push(num1 / num2);
                                    }
                                }
                            }
                            16 => {
                                // callothersubr: arg1..argN, n, othersubr#
                                if stack.len() >= 2 {
                                    let othersubr_num = stack.pop().unwrap() as usize;
                                    let _n = stack.pop().unwrap() as usize;
                                    match othersubr_num {
                                        0 => {
                                            *in_flex = false;
                                            if flex_points.len() >= 8 {
                                                let p2 = flex_points[2];
                                                let p3 = flex_points[3];
                                                let p4 = flex_points[4];
                                                let p5 = flex_points[5];
                                                let p6 = flex_points[6];
                                                let p7 = flex_points[7];
                                                path.curve_to(p2.0, p2.1, p3.0, p3.1, p4.0, p4.1);
                                                path.curve_to(p5.0, p5.1, p6.0, p6.1, p7.0, p7.1);
                                                *cur_x = p7.0;
                                                *cur_y = p7.1;
                                            }
                                            // setcurrentpoint pops y then x, so push y first then x
                                            othersubr_results.push(*cur_y);
                                            othersubr_results.push(*cur_x);
                                            stack.clear();
                                        }
                                        1 => {
                                            *in_flex = true;
                                            flex_points.clear();
                                            flex_points.push((*cur_x, *cur_y));
                                            stack.clear();
                                        }
                                        2 => {
                                            stack.clear();
                                        }
                                        3 => {
                                            // Hint replacement
                                            othersubr_results.push(3.0);
                                            stack.clear();
                                        }
                                        _ => {
                                            stack.clear();
                                        }
                                    }
                                }
                            }
                            17 => {
                                // pop (pops result from othersubr to Type 1 stack)
                                if let Some(val) = othersubr_results.pop() {
                                    stack.push(val);
                                }
                            }
                            33 => {
                                // setcurrentpoint
                                if stack.len() >= 2 {
                                    let y = stack.pop().unwrap();
                                    let x = stack.pop().unwrap();
                                    *cur_x = x;
                                    *cur_y = y;
                                }
                                stack.clear();
                            }
                            _ => {
                                stack.clear();
                            }
                        }
                    }
                }
                // hsbw: sbx, wx
                13 => {
                    if stack.len() >= 2 {
                        *lsb = stack[0];
                        *width = stack[1];
                        *cur_x = stack[0];
                        *cur_y = 0.0;
                    }
                    stack.clear();
                }
                // endchar
                14 => {
                    path.close_path();
                    stack.clear();
                    return Ok(());
                }
                // rmoveto (21)
                21 => {
                    if stack.len() >= 2 {
                        let dy = stack.pop().unwrap();
                        let dx = stack.pop().unwrap();
                        *cur_x += dx;
                        *cur_y += dy;
                        if *in_flex {
                            flex_points.push((*cur_x, *cur_y));
                        } else {
                            path.move_to(*cur_x, *cur_y);
                        }
                    }
                    stack.clear();
                }
                // hmoveto (22)
                22 => {
                    if let Some(dx) = stack.pop() {
                        *cur_x += dx;
                        path.move_to(*cur_x, *cur_y);
                    }
                    stack.clear();
                }
                // vhcurveto (30)
                30 => {
                    let mut i = 0;
                    let mut v_first = true;
                    while i + 3 < stack.len() {
                        if v_first {
                            let dy1 = stack[i];
                            let dx2 = stack[i + 1];
                            let dy2 = stack[i + 2];
                            let dx3 = stack[i + 3];
                            let dy4 = if i + 5 == stack.len() { stack[i + 4] } else { 0.0 };

                            let cp1x = *cur_x;
                            let cp1y = *cur_y + dy1;
                            let cp2x = cp1x + dx2;
                            let cp2y = cp1y + dy2;
                            *cur_x = cp2x + dx3;
                            *cur_y = cp2y + dy4;

                            path.curve_to(cp1x, cp1y, cp2x, cp2y, *cur_x, *cur_y);
                            i += 4;
                            if dy4 != 0.0 { i += 1; }
                        } else {
                            let dx1 = stack[i];
                            let dx2 = stack[i + 1];
                            let dy2 = stack[i + 2];
                            let dy3 = stack[i + 3];
                            let dx4 = if i + 5 == stack.len() { stack[i + 4] } else { 0.0 };

                            let cp1x = *cur_x + dx1;
                            let cp1y = *cur_y;
                            let cp2x = cp1x + dx2;
                            let cp2y = cp1y + dy2;
                            *cur_x = cp2x + dx4;
                            *cur_y = cp2y + dy3;

                            path.curve_to(cp1x, cp1y, cp2x, cp2y, *cur_x, *cur_y);
                            i += 4;
                            if dx4 != 0.0 { i += 1; }
                        }
                        v_first = !v_first;
                    }
                    stack.clear();
                }
                // hvcurveto (31)
                31 => {
                    let mut i = 0;
                    let mut h_first = true;
                    while i + 3 < stack.len() {
                        if h_first {
                            let dx1 = stack[i];
                            let dx2 = stack[i + 1];
                            let dy2 = stack[i + 2];
                            let dy3 = stack[i + 3];
                            let dx4 = if i + 5 == stack.len() { stack[i + 4] } else { 0.0 };

                            let cp1x = *cur_x + dx1;
                            let cp1y = *cur_y;
                            let cp2x = cp1x + dx2;
                            let cp2y = cp1y + dy2;
                            *cur_x = cp2x + dx4;
                            *cur_y = cp2y + dy3;

                            path.curve_to(cp1x, cp1y, cp2x, cp2y, *cur_x, *cur_y);
                            i += 4;
                            if dx4 != 0.0 { i += 1; }
                        } else {
                            let dy1 = stack[i];
                            let dx2 = stack[i + 1];
                            let dy2 = stack[i + 2];
                            let dx3 = stack[i + 3];
                            let dy4 = if i + 5 == stack.len() { stack[i + 4] } else { 0.0 };

                            let cp1x = *cur_x;
                            let cp1y = *cur_y + dy1;
                            let cp2x = cp1x + dx2;
                            let cp2y = cp1y + dy2;
                            *cur_x = cp2x + dx3;
                            *cur_y = cp2y + dy4;

                            path.curve_to(cp1x, cp1y, cp2x, cp2y, *cur_x, *cur_y);
                            i += 4;
                            if dy4 != 0.0 { i += 1; }
                        }
                        h_first = !h_first;
                    }
                    stack.clear();
                }
                // Integers: 32..=246
                32..=246 => {
                    stack.push((b0 as i32 - 139) as f64);
                }
                // Positive integers: 247..=250
                247..=250 => {
                    if pos < bytecode.len() {
                        let b1 = bytecode[pos];
                        pos += 1;
                        let val = ((b0 as i32 - 247) * 256) + b1 as i32 + 108;
                        stack.push(val as f64);
                    }
                }
                // Negative integers: 251..=254
                251..=254 => {
                    if pos < bytecode.len() {
                        let b1 = bytecode[pos];
                        pos += 1;
                        let val = -((b0 as i32 - 251) * 256) - b1 as i32 - 108;
                        stack.push(val as f64);
                    }
                }
                // 32-bit signed integer: 255
                255 => {
                    if pos + 3 < bytecode.len() {
                        let val = ((bytecode[pos] as i32) << 24)
                            | ((bytecode[pos + 1] as i32) << 16)
                            | ((bytecode[pos + 2] as i32) << 8)
                            | (bytecode[pos + 3] as i32);
                        pos += 4;
                        stack.push(val as f64);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
