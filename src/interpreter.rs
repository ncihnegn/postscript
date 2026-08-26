use crate::error::{PsError, PsResult};
use crate::font::FontFace;
use crate::font::eexec::Type1Cipher;
use crate::gstate::{Color, GraphicsState, LineCap, LineJoin};
use crate::lexer::{Lexer, Token};
use crate::matrix::Matrix2D;
use crate::render::RenderTarget;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Interpreter {
    pub operand_stack: Vec<Value>,
    pub dict_stack: Vec<Rc<RefCell<HashMap<String, Value>>>>,
    pub gstate_stack: Vec<GraphicsState>,
    pub current_gstate: GraphicsState,
    pub font_directory: HashMap<String, FontFace>,
    pub render_target: RenderTarget,
    pub pages_rendered: Vec<RenderTarget>,
    pub initial_ctm: Matrix2D,
}

impl Interpreter {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_page_size(width as f64, height as f64, width, height)
    }

    pub fn with_page_size(page_w: f64, page_h: f64, pixel_w: u32, pixel_h: u32) -> Self {
        let sx = if page_w > 0.0 { (pixel_w as f64) / page_w } else { 1.0 };
        let sy = if page_h > 0.0 { (pixel_h as f64) / page_h } else { 1.0 };
        let initial_ctm = Matrix2D::new(sx, 0.0, 0.0, -sy, 0.0, pixel_h as f64);

        let mut current_gstate = GraphicsState::new();
        current_gstate.ctm = initial_ctm;

        let mut interp = Self {
            operand_stack: Vec::with_capacity(128),
            dict_stack: Vec::with_capacity(16),
            gstate_stack: Vec::with_capacity(16),
            current_gstate,
            font_directory: HashMap::new(),
            render_target: RenderTarget::new(pixel_w, pixel_h),
            pages_rendered: Vec::new(),
            initial_ctm,
        };

        // Initialize systemdict and userdict
        let systemdict = Rc::new(RefCell::new(HashMap::new()));
        let userdict = Rc::new(RefCell::new(HashMap::new()));
        interp.dict_stack.push(systemdict);
        interp.dict_stack.push(userdict);

        // Preload common Computer Modern and standard fonts
        for font_name in &[
            "CMR10", "CMR12", "CMR17", "CMR9", "CMR8", "CMR7", "CMR6", "CMR5",
            "CMBX10", "CMBX12", "CMBX9", "CMBX8", "CMBX7", "CMBX6", "CMBX5",
            "CMSY10", "CMSY9", "CMSY8", "CMSY7", "CMSY6", "CMSY5",
            "CMMI10", "CMMI12", "CMMI9", "CMMI8", "CMMI7", "CMMI6", "CMMI5",
            "CMTI10", "CMTI12", "CMTI9", "CMTI8", "CMTI7",
            "CMTT10", "CMTT12", "CMTT9", "CMTT8",
            "CMEX10", "MSAM10", "MSBM10",
            "Times-Roman", "Times-Bold", "Times-Italic", "Helvetica", "Courier"
        ] {
            interp.font_directory.insert(font_name.to_string(), FontFace::new(font_name));
        }

        interp
    }

    pub fn execute_str(&mut self, ps_code: &str) -> PsResult<()> {
        self.execute_bytes(ps_code.as_bytes())
    }

    pub fn execute_bytes(&mut self, ps_bytes: &[u8]) -> PsResult<()> {
        let mut lexer = Lexer::new(ps_bytes);
        self.run_lexer(&mut lexer)
    }

    pub fn run_lexer(&mut self, lexer: &mut Lexer) -> PsResult<()> {
        while let Some(token) = lexer.next_token()? {
            match token {
                Token::Value(val) => {
                    self.eval_value(val, lexer)?;
                }
                Token::LeftBrace => {
                    let proc = self.read_procedure(lexer)?;
                    self.operand_stack.push(proc);
                }
                Token::RightBrace => {
                    return Err(PsError::SyntaxError("unexpected '}'".to_string()));
                }
                Token::LeftBracket => {
                    self.operand_stack.push(Value::Mark);
                }
                Token::RightBracket => {
                    self.op_close_bracket()?;
                }
                Token::LeftDict => {
                    self.operand_stack.push(Value::Mark);
                }
                Token::RightDict => {
                    self.op_close_dict()?;
                }
            }
        }
        Ok(())
    }

    fn read_procedure(&mut self, lexer: &mut Lexer) -> PsResult<Value> {
        let mut items = Vec::new();

        while let Some(token) = lexer.next_token()? {
            match token {
                Token::LeftBrace => {
                    let nested = self.read_procedure(lexer)?;
                    items.push(nested);
                }
                Token::RightBrace => {
                    return Ok(Value::new_proc(items));
                }
                Token::Value(val) => {
                    items.push(val);
                }
                Token::LeftBracket => {
                    items.push(Value::Name("[".to_string()));
                }
                Token::RightBracket => {
                    items.push(Value::Name("]".to_string()));
                }
                Token::LeftDict => {
                    items.push(Value::Name("<<".to_string()));
                }
                Token::RightDict => {
                    items.push(Value::Name(">>".to_string()));
                }
            }
        }

        Err(PsError::SyntaxError("unterminated procedure".to_string()))
    }

    pub fn eval_value(&mut self, val: Value, lexer: &mut Lexer) -> PsResult<()> {
        match val {
            Value::Name(name) => {
                self.execute_name(&name, lexer)?;
            }
            other => {
                self.operand_stack.push(other);
            }
        }
        Ok(())
    }

    pub fn execute_name(&mut self, name: &str, lexer: &mut Lexer) -> PsResult<()> {
        // Built-in operators dispatch
        match self.dispatch_builtin(name, lexer) {
            Ok(true) => return Ok(()),
            Err(e) => return Err(PsError::SyntaxError(format!("operator '{}' failed: {}", name, e))),
            Ok(false) => {}
        }

        // Look up name in dictionary stack
        if let Some(val) = self.lookup_dict(name) {
            match val {
                Value::ExecutableArray(proc) => {
                    let items = proc.clone();
                    for item in items.iter() {
                        self.eval_value(item.clone(), lexer)?;
                    }
                }
                other => {
                    self.operand_stack.push(other);
                }
            }
            return Ok(());
        }

        // Ignore unknown pdfmark/BDC or DSC directives safely
        if name.ends_with("mark") || name == "BDC" || name == "EMC" || name == "pdfmark" {
            // pdfmark takes a mark and pops to mark
            self.op_cleartomark().ok();
            return Ok(());
        }

        // Return error for unknown identifier
        Err(PsError::Undefined(name.to_string()))
    }

    pub fn lookup_dict(&self, key: &str) -> Option<Value> {
        for dict in self.dict_stack.iter().rev() {
            if let Some(val) = dict.borrow().get(key) {
                return Some(val.clone());
            }
        }
        None
    }

    pub fn def_in_current_dict(&mut self, key: String, val: Value) -> PsResult<()> {
        if let Some(dict) = self.dict_stack.last() {
            dict.borrow_mut().insert(key, val);
            Ok(())
        } else {
            Err(PsError::LimitCheck("empty dict stack".to_string()))
        }
    }

    fn dispatch_builtin(&mut self, name: &str, lexer: &mut Lexer) -> PsResult<bool> {
        match name {
            // Stack manipulation
            "pop" => self.op_pop()?,
            "dup" => self.op_dup()?,
            "exch" => self.op_exch()?,
            "copy" => self.op_copy()?,
            "index" => self.op_index()?,
            "roll" => self.op_roll()?,
            "clear" => self.operand_stack.clear(),
            "count" => {
                let len = self.operand_stack.len() as i64;
                self.operand_stack.push(Value::Integer(len));
            }
            "mark" | "[" => self.operand_stack.push(Value::Mark),
            "cleartomark" => self.op_cleartomark()?,
            "]" => self.op_close_bracket()?,

            // Arithmetic & Math
            "add" => self.op_bin_num(|a, b| Ok(a + b))?,
            "sub" => self.op_bin_num(|a, b| Ok(a - b))?,
            "mul" => self.op_bin_num(|a, b| Ok(a * b))?,
            "div" => self.op_bin_num(|a, b| {
                if b == 0.0 { Err(PsError::UndefinedResult) } else { Ok(a / b) }
            })?,
            "idiv" => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 { return Err(PsError::UndefinedResult); }
                self.operand_stack.push(Value::Integer(a / b));
            }
            "mod" => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 { return Err(PsError::UndefinedResult); }
                self.operand_stack.push(Value::Integer(a % b));
            }
            "neg" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(-val));
            }
            "abs" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.abs()));
            }
            "sin" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.to_radians().sin()));
            }
            "cos" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.to_radians().cos()));
            }
            "atan" => {
                let den = self.pop_num()?;
                let num = self.pop_num()?;
                self.operand_stack.push(Value::Real(num.atan2(den).to_degrees()));
            }
            "sqrt" => {
                let val = self.pop_num()?;
                if val < 0.0 { return Err(PsError::RangeCheck("sqrt of negative number".to_string())); }
                self.operand_stack.push(Value::Real(val.sqrt()));
            }
            "ln" => {
                let val = self.pop_num()?;
                if val <= 0.0 { return Err(PsError::RangeCheck("ln of non-positive number".to_string())); }
                self.operand_stack.push(Value::Real(val.ln()));
            }
            "exp" => {
                let exp = self.pop_num()?;
                let base = self.pop_num()?;
                self.operand_stack.push(Value::Real(base.powf(exp)));
            }
            "truncate" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.trunc()));
            }
            "round" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.round()));
            }
            "floor" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.floor()));
            }
            "ceiling" => {
                let val = self.pop_num()?;
                self.operand_stack.push(Value::Real(val.ceil()));
            }

            // Comparison & Boolean
            "eq" => {
                let b = self.pop_value()?;
                let a = self.pop_value()?;
                self.operand_stack.push(Value::Bool(a == b));
            }
            "ne" => {
                let b = self.pop_value()?;
                let a = self.pop_value()?;
                self.operand_stack.push(Value::Bool(a != b));
            }
            "ge" => self.op_bin_bool(|a, b| a >= b)?,
            "gt" => self.op_bin_bool(|a, b| a > b)?,
            "le" => self.op_bin_bool(|a, b| a <= b)?,
            "lt" => self.op_bin_bool(|a, b| a < b)?,
            "and" => {
                let b = self.pop_value()?;
                let a = self.pop_value()?;
                match (a, b) {
                    (Value::Bool(v1), Value::Bool(v2)) => self.operand_stack.push(Value::Bool(v1 && v2)),
                    (Value::Integer(i1), Value::Integer(i2)) => self.operand_stack.push(Value::Integer(i1 & i2)),
                    _ => return Err(PsError::TypeCheck { expected: "bool or int", got: "other".to_string() }),
                }
            }
            "or" => {
                let b = self.pop_value()?;
                let a = self.pop_value()?;
                match (a, b) {
                    (Value::Bool(v1), Value::Bool(v2)) => self.operand_stack.push(Value::Bool(v1 || v2)),
                    (Value::Integer(i1), Value::Integer(i2)) => self.operand_stack.push(Value::Integer(i1 | i2)),
                    _ => return Err(PsError::TypeCheck { expected: "bool or int", got: "other".to_string() }),
                }
            }
            "not" => {
                let a = self.pop_value()?;
                match a {
                    Value::Bool(b) => self.operand_stack.push(Value::Bool(!b)),
                    Value::Integer(i) => self.operand_stack.push(Value::Integer(!i)),
                    _ => return Err(PsError::TypeCheck { expected: "bool or int", got: a.type_name().to_string() }),
                }
            }

            // Dictionary operators
            "dict" => {
                let _capacity = self.pop_i64()?;
                self.operand_stack.push(Value::new_dict());
            }
            "begin" => {
                let dict = self.pop_dict()?;
                self.dict_stack.push(dict);
            }
            "end" => {
                if self.dict_stack.len() > 2 {
                    self.dict_stack.pop();
                }
            }
            "def" => {
                let val = self.pop_value()?;
                let key = self.pop_key_name()?;
                self.def_in_current_dict(key, val)?;
            }
            "known" => {
                let key = self.pop_key_name()?;
                let dict = self.pop_dict()?;
                let is_known = dict.borrow().contains_key(&key);
                self.operand_stack.push(Value::Bool(is_known));
            }
            "get" => {
                let key_or_index = self.pop_value()?;
                let container = self.pop_value()?;
                match container {
                    Value::Dict(d) => {
                        let k = match key_or_index {
                            Value::LiteralName(n) | Value::Name(n) => n,
                            Value::String(s) => String::from_utf8_lossy(&s).to_string(),
                            _ => return Err(PsError::TypeCheck { expected: "name key", got: key_or_index.type_name().to_string() }),
                        };
                        let val = d.borrow().get(&k).cloned().unwrap_or(Value::Null);
                        self.operand_stack.push(val);
                    }
                    Value::Array(a) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let arr = a.borrow();
                        let val = arr.get(idx).cloned().ok_or_else(|| PsError::RangeCheck("array index out of bounds".to_string()))?;
                        self.operand_stack.push(val);
                    }
                    _ => return Err(PsError::TypeCheck { expected: "dict or array", got: container.type_name().to_string() }),
                }
            }
            "put" => {
                let val = self.pop_value()?;
                let key_or_index = self.pop_value()?;
                let container = self.pop_value()?;
                match container {
                    Value::Dict(d) => {
                        let k = match key_or_index {
                            Value::LiteralName(n) | Value::Name(n) => n,
                            Value::String(s) => String::from_utf8_lossy(&s).to_string(),
                            _ => return Err(PsError::TypeCheck { expected: "name key", got: key_or_index.type_name().to_string() }),
                        };
                        d.borrow_mut().insert(k, val);
                    }
                    Value::Array(a) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let mut arr = a.borrow_mut();
                        if idx < arr.len() {
                            arr[idx] = val;
                        } else if idx == arr.len() {
                            arr.push(val);
                        } else {
                            return Err(PsError::RangeCheck("array index out of bounds".to_string()));
                        }
                    }
                    _ => return Err(PsError::TypeCheck { expected: "dict or array", got: container.type_name().to_string() }),
                }
            }
            "currentdict" => {
                if let Some(d) = self.dict_stack.last() {
                    self.operand_stack.push(Value::Dict(d.clone()));
                }
            }
            "systemdict" => {
                if let Some(d) = self.dict_stack.first() {
                    self.operand_stack.push(Value::Dict(d.clone()));
                }
            }
            "userdict" => {
                if self.dict_stack.len() > 1 {
                    self.operand_stack.push(Value::Dict(self.dict_stack[1].clone()));
                }
            }
            "FontDirectory" => {
                let dict = Rc::new(RefCell::new(HashMap::new()));
                for (name, _) in &self.font_directory {
                    dict.borrow_mut().insert(name.clone(), Value::new_dict());
                }
                self.operand_stack.push(Value::Dict(dict));
            }

            // Array operators
            "array" => {
                let len = self.pop_i64()? as usize;
                let items = vec![Value::Null; len];
                self.operand_stack.push(Value::new_array(items));
            }
            "length" => {
                let val = self.pop_value()?;
                let len = match val {
                    Value::Array(a) => a.borrow().len(),
                    Value::ExecutableArray(a) => a.len(),
                    Value::Dict(d) => d.borrow().len(),
                    Value::String(s) => s.len(),
                    _ => return Err(PsError::TypeCheck { expected: "array, dict or string", got: val.type_name().to_string() }),
                };
                self.operand_stack.push(Value::Integer(len as i64));
            }
            "aload" => {
                let val = self.pop_value()?;
                if let Value::Array(a) = val.clone() {
                    let items = a.borrow().clone();
                    for item in items {
                        self.operand_stack.push(item);
                    }
                    self.operand_stack.push(val);
                } else {
                    return Err(PsError::TypeCheck { expected: "array", got: val.type_name().to_string() });
                }
            }
            "astore" => {
                let val = self.pop_value()?;
                if let Value::Array(a) = val.clone() {
                    let len = a.borrow().len();
                    let mut items = Vec::with_capacity(len);
                    for _ in 0..len {
                        items.push(self.pop_value()?);
                    }
                    items.reverse();
                    *a.borrow_mut() = items;
                    self.operand_stack.push(val);
                } else {
                    return Err(PsError::TypeCheck { expected: "array", got: val.type_name().to_string() });
                }
            }

            // Control flow
            "if" => {
                let proc = self.pop_value()?;
                if let Ok(cond) = self.pop_bool() {
                    if cond {
                        self.execute_proc_value(proc, lexer)?;
                    }
                }
            }
            "ifelse" => {
                let proc_false = self.pop_value()?;
                let proc_true = self.pop_value()?;
                if let Ok(cond) = self.pop_bool() {
                    if cond {
                        self.execute_proc_value(proc_true, lexer)?;
                    } else {
                        self.execute_proc_value(proc_false, lexer)?;
                    }
                }
            }
            "for" => {
                let proc = self.pop_value()?;
                let limit = self.pop_num()?;
                let step = self.pop_num()?;
                let init = self.pop_num()?;

                let mut current = init;
                if step > 0.0 {
                    while current <= limit {
                        self.operand_stack.push(Value::Real(current));
                        self.execute_proc_value(proc.clone(), lexer)?;
                        current += step;
                    }
                } else if step < 0.0 {
                    while current >= limit {
                        self.operand_stack.push(Value::Real(current));
                        self.execute_proc_value(proc.clone(), lexer)?;
                        current += step;
                    }
                }
            }
            "repeat" => {
                let proc = self.pop_value()?;
                let count = self.pop_i64()?;
                for _ in 0..count.max(0) {
                    self.execute_proc_value(proc.clone(), lexer)?;
                }
            }
            "loop" => {
                let proc = self.pop_value()?;
                loop {
                    match self.execute_proc_value(proc.clone(), lexer) {
                        Ok(()) => {}
                        Err(PsError::Exit) => break,
                        Err(e) => return Err(e),
                    }
                }
            }
            "exit" => return Err(PsError::Exit),
            "exec" => {
                let val = self.pop_value()?;
                self.execute_proc_value(val, lexer)?;
            }
            "bind" => {
                // Optimization / no-op in interpreter
            }
            "save" => {
                self.operand_stack.push(Value::Integer(1));
            }
            "restore" => {
                self.pop_value().ok();
            }
            "readonly" | "executeonly" | "noaccess" => {
                // Return same object unchanged
            }

            // Graphics State
            "gsave" => {
                self.gstate_stack.push(self.current_gstate.clone());
            }
            "grestore" => {
                if let Some(saved) = self.gstate_stack.pop() {
                    self.current_gstate = saved;
                }
            }
            "newpath" => {
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
            }
            "moveto" => {
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                self.current_gstate.current_path.move_to(x, y);
                self.current_gstate.current_point = Some((x, y));
            }
            "rmoveto" => {
                let dy = self.pop_num()?;
                let dx = self.pop_num()?;
                let (cx, cy) = self.current_gstate.current_point.unwrap_or((0.0, 0.0));
                let nx = cx + dx;
                let ny = cy + dy;
                self.current_gstate.current_path.move_to(nx, ny);
                self.current_gstate.current_point = Some((nx, ny));
            }
            "lineto" => {
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                self.current_gstate.current_path.line_to(x, y);
                self.current_gstate.current_point = Some((x, y));
            }
            "rlineto" => {
                let dy = self.pop_num()?;
                let dx = self.pop_num()?;
                let (cx, cy) = self.current_gstate.current_point.unwrap_or((0.0, 0.0));
                let nx = cx + dx;
                let ny = cy + dy;
                self.current_gstate.current_path.line_to(nx, ny);
                self.current_gstate.current_point = Some((nx, ny));
            }
            "curveto" => {
                let y3 = self.pop_num()?;
                let x3 = self.pop_num()?;
                let y2 = self.pop_num()?;
                let x2 = self.pop_num()?;
                let y1 = self.pop_num()?;
                let x1 = self.pop_num()?;
                self.current_gstate.current_path.curve_to(x1, y1, x2, y2, x3, y3);
                self.current_gstate.current_point = Some((x3, y3));
            }
            "rcurveto" => {
                let dy3 = self.pop_num()?;
                let dx3 = self.pop_num()?;
                let dy2 = self.pop_num()?;
                let dx2 = self.pop_num()?;
                let dy1 = self.pop_num()?;
                let dx1 = self.pop_num()?;
                let (cx, cy) = self.current_gstate.current_point.unwrap_or((0.0, 0.0));
                let nx1 = cx + dx1;
                let ny1 = cy + dy1;
                let nx2 = nx1 + dx2;
                let ny2 = ny1 + dy2;
                let nx3 = nx2 + dx3;
                let ny3 = ny2 + dy3;
                self.current_gstate.current_path.curve_to(nx1, ny1, nx2, ny2, nx3, ny3);
                self.current_gstate.current_point = Some((nx3, ny3));
            }
            "arc" => {
                let angle2 = self.pop_num()?;
                let angle1 = self.pop_num()?;
                let r = self.pop_num()?;
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                self.current_gstate.current_path.arc(x, y, r, angle1, angle2, false);
                let end_x = x + r * angle2.to_radians().cos();
                let end_y = y + r * angle2.to_radians().sin();
                self.current_gstate.current_point = Some((end_x, end_y));
            }
            "arcn" => {
                let angle2 = self.pop_num()?;
                let angle1 = self.pop_num()?;
                let r = self.pop_num()?;
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                self.current_gstate.current_path.arc(x, y, r, angle1, angle2, true);
                let end_x = x + r * angle2.to_radians().cos();
                let end_y = y + r * angle2.to_radians().sin();
                self.current_gstate.current_point = Some((end_x, end_y));
            }
            "closepath" => {
                self.current_gstate.current_path.close_path();
            }
            "currentpoint" => {
                if let Some((x, y)) = self.current_gstate.current_point {
                    self.operand_stack.push(Value::Real(x));
                    self.operand_stack.push(Value::Real(y));
                } else {
                    return Err(PsError::LimitCheck("currentpoint is undefined".to_string()));
                }
            }

            // Painting
            "fill" => {
                let transformed_path = self.current_gstate.current_path.transform(&self.current_gstate.ctm);
                self.render_target.push_fill(transformed_path, self.current_gstate.color, false);
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
            }
            "eofill" => {
                let transformed_path = self.current_gstate.current_path.transform(&self.current_gstate.ctm);
                self.render_target.push_fill(transformed_path, self.current_gstate.color, true);
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
            }
            "stroke" => {
                let transformed_path = self.current_gstate.current_path.transform(&self.current_gstate.ctm);
                let (scaled_width, _) = self.current_gstate.ctm.transform_vector(self.current_gstate.line_width, 0.0);
                self.render_target.push_stroke(
                    transformed_path,
                    self.current_gstate.color,
                    scaled_width.abs(),
                    self.current_gstate.line_cap,
                    self.current_gstate.line_join,
                    self.current_gstate.miter_limit,
                );
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
            }
            "showpage" => {
                let w = self.render_target.width;
                let h = self.render_target.height;
                let page = std::mem::replace(
                    &mut self.render_target,
                    RenderTarget::new(w, h),
                );
                self.pages_rendered.push(page);
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
            }

            // Style & Color
            "setlinewidth" => {
                let w = self.pop_num()?;
                self.current_gstate.line_width = w;
            }
            "setgray" => {
                let g = self.pop_num()?;
                self.current_gstate.color = Color::gray(g);
            }
            "setrgbcolor" => {
                let b = self.pop_num()?;
                let g = self.pop_num()?;
                let r = self.pop_num()?;
                self.current_gstate.color = Color::rgb(r, g, b);
            }
            "setcmykcolor" => {
                let k = self.pop_num()?;
                let y = self.pop_num()?;
                let m = self.pop_num()?;
                let c = self.pop_num()?;
                self.current_gstate.color = Color::cmyk(c, m, y, k);
            }
            "setlinecap" => {
                let cap = self.pop_i64()?;
                self.current_gstate.line_cap = match cap {
                    1 => LineCap::Round,
                    2 => LineCap::Square,
                    _ => LineCap::Butt,
                };
            }
            "setlinejoin" => {
                let join = self.pop_i64()?;
                self.current_gstate.line_join = match join {
                    1 => LineJoin::Round,
                    2 => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                };
            }
            "setmiterlimit" => {
                let limit = self.pop_num()?;
                self.current_gstate.miter_limit = limit;
            }

            // Coordinate Transforms
            "translate" => {
                let ty = self.pop_num()?;
                let tx = self.pop_num()?;
                let t_mat = Matrix2D::translate(tx, ty);
                self.current_gstate.ctm = t_mat.concat(&self.current_gstate.ctm);
            }
            "scale" => {
                let sy = self.pop_num()?;
                let sx = self.pop_num()?;
                let s_mat = Matrix2D::scale(sx, sy);
                self.current_gstate.ctm = s_mat.concat(&self.current_gstate.ctm);
            }
            "rotate" => {
                let deg = self.pop_num()?;
                let r_mat = Matrix2D::rotate(deg);
                self.current_gstate.ctm = r_mat.concat(&self.current_gstate.ctm);
            }
            "concat" => {
                let mat_val = self.pop_value()?;
                let mat = self.val_to_matrix(mat_val)?;
                self.current_gstate.ctm = mat.concat(&self.current_gstate.ctm);
            }
            "matrix" => {
                self.operand_stack.push(self.matrix_to_val(Matrix2D::identity()));
            }
            "currentmatrix" => {
                let _target = self.pop_value().ok();
                self.operand_stack.push(self.matrix_to_val(self.current_gstate.ctm));
            }
            "initmatrix" => {
                self.current_gstate.ctm = self.initial_ctm;
            }
            "setmatrix" => {
                let mat_val = self.pop_value()?;
                self.current_gstate.ctm = self.val_to_matrix(mat_val)?;
            }
            "transform" => {
                let val = self.pop_value()?;
                let (x, y, mat) = if let Ok(m) = self.val_to_matrix(val.clone()) {
                    let y = self.pop_num()?;
                    let x = self.pop_num()?;
                    (x, y, m)
                } else {
                    let y = val.as_f64()?;
                    let x = self.pop_num()?;
                    (x, y, self.current_gstate.ctm)
                };
                let (tx, ty) = mat.transform_point(x, y);
                self.operand_stack.push(Value::Real(tx));
                self.operand_stack.push(Value::Real(ty));
            }
            "itransform" => {
                let val = self.pop_value()?;
                let (x, y, mat) = if let Ok(m) = self.val_to_matrix(val.clone()) {
                    let y = self.pop_num()?;
                    let x = self.pop_num()?;
                    (x, y, m)
                } else {
                    let y = val.as_f64()?;
                    let x = self.pop_num()?;
                    (x, y, self.current_gstate.ctm)
                };
                let inv = mat.inverse().unwrap_or(Matrix2D::identity());
                let (tx, ty) = inv.transform_point(x, y);
                self.operand_stack.push(Value::Real(tx));
                self.operand_stack.push(Value::Real(ty));
            }
            "dtransform" => {
                let val = self.pop_value()?;
                let (dx, dy, mat) = if let Ok(m) = self.val_to_matrix(val.clone()) {
                    let dy = self.pop_num()?;
                    let dx = self.pop_num()?;
                    (dx, dy, m)
                } else {
                    let dy = val.as_f64()?;
                    let dx = self.pop_num()?;
                    (dx, dy, self.current_gstate.ctm)
                };
                let (tdx, tdy) = mat.transform_vector(dx, dy);
                self.operand_stack.push(Value::Real(tdx));
                self.operand_stack.push(Value::Real(tdy));
            }
            "idtransform" => {
                let val = self.pop_value()?;
                let (dx, dy, mat) = if let Ok(m) = self.val_to_matrix(val.clone()) {
                    let dy = self.pop_num()?;
                    let dx = self.pop_num()?;
                    (dx, dy, m)
                } else {
                    let dy = val.as_f64()?;
                    let dx = self.pop_num()?;
                    (dx, dy, self.current_gstate.ctm)
                };
                let inv = mat.inverse().unwrap_or(Matrix2D::identity());
                let (tdx, tdy) = inv.transform_vector(dx, dy);
                self.operand_stack.push(Value::Real(tdx));
                self.operand_stack.push(Value::Real(tdy));
            }
            "setdash" => {
                let _offset = self.pop_num()?;
                let _array = self.pop_value()?;
            }
            "currentdash" => {
                self.operand_stack.push(Value::new_array(vec![]));
                self.operand_stack.push(Value::Integer(0));
            }

            // Font operators
            "findfont" => {
                let name = self.pop_key_name()?;
                let _font = self.font_directory.get(&name).cloned().unwrap_or_else(|| FontFace::new(&name));
                let dict = Rc::new(RefCell::new(HashMap::new()));
                dict.borrow_mut().insert("FontName".to_string(), Value::LiteralName(name));
                self.operand_stack.push(Value::Dict(dict));
            }
            "scalefont" => {
                let scale = self.pop_num()?;
                let font_dict = self.pop_dict()?;
                let font_name = font_dict.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default();
                let font = self.font_directory.get(&font_name).cloned().unwrap_or_else(|| FontFace::new(&font_name));
                let scaled_font = font.scalefont(scale);
                self.font_directory.insert(font_name.clone(), scaled_font);
                self.operand_stack.push(Value::Dict(font_dict));
            }
            "makefont" => {
                let mat_val = self.pop_value()?;
                let mat = self.val_to_matrix(mat_val)?;
                let font_dict = self.pop_dict()?;
                let font_name = font_dict.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default();
                let font = self.font_directory.get(&font_name).cloned().unwrap_or_else(|| FontFace::new(&font_name));
                let made_font = font.makefont(mat);
                self.font_directory.insert(font_name.clone(), made_font);
                self.operand_stack.push(Value::Dict(font_dict));
            }
            "setfont" => {
                let font_val = self.pop_value()?;
                let font_name = match font_val {
                    Value::Dict(d) => d.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default(),
                    Value::LiteralName(n) | Value::Name(n) => n,
                    _ => "".to_string(),
                };
                let font = self.font_directory.get(&font_name).cloned().unwrap_or_else(|| FontFace::new(&font_name));
                self.current_gstate.font = Some(font);
            }
            "charpath" => {
                let _stroke_bool = self.pop_bool()?;
                let str_val = self.pop_value()?;
                let bytes = match str_val {
                    Value::String(s) => s,
                    Value::Name(n) | Value::LiteralName(n) => n.into_bytes(),
                    _ => return Err(PsError::TypeCheck { expected: "string", got: str_val.type_name().to_string() }),
                };

                let (mut cx, cy) = self.current_gstate.current_point.unwrap_or((0.0, 0.0));
                for &b in &bytes {
                    let glyph_name = if let Some(f) = &self.current_gstate.font {
                        f.encoding.get(b as usize).cloned().unwrap_or_else(|| ".notdef".to_string())
                    } else {
                        (b as char).to_string()
                    };

                    if let Some(f) = &self.current_gstate.font {
                        if let Some((glyph_path, width)) = f.get_glyph_path(&glyph_name) {
                            let placed = glyph_path.transform(&Matrix2D::translate(cx, cy));
                            self.current_gstate.current_path.append(&placed);
                            cx += width;
                            continue;
                        }
                    }

                    // Fallback advance for standard characters
                    cx += 6.0;
                }
                self.current_gstate.current_point = Some((cx, cy));
            }
            "show" => {
                let str_val = self.pop_value()?;
                let bytes = match str_val {
                    Value::String(s) => s,
                    Value::Name(n) | Value::LiteralName(n) => n.into_bytes(),
                    _ => return Err(PsError::TypeCheck { expected: "string", got: str_val.type_name().to_string() }),
                };

                let (mut cx, cy) = self.current_gstate.current_point.unwrap_or((0.0, 0.0));
                for &b in &bytes {
                    let glyph_name = if let Some(f) = &self.current_gstate.font {
                        f.encoding.get(b as usize).cloned().unwrap_or_else(|| ".notdef".to_string())
                    } else {
                        (b as char).to_string()
                    };

                    if let Some(f) = &self.current_gstate.font {
                        if let Some((glyph_path, width)) = f.get_glyph_path(&glyph_name) {
                            let placed = glyph_path.transform(&Matrix2D::translate(cx, cy));
                            let transformed = placed.transform(&self.current_gstate.ctm);
                            self.render_target.push_fill(transformed, self.current_gstate.color, false);
                            cx += width;
                            continue;
                        }
                    }
                    cx += 6.0;
                }
                self.current_gstate.current_point = Some((cx, cy));
            }

            "currentfile" => {
                self.operand_stack.push(Value::LiteralName("currentfile".to_string()));
            }

            // eexec handling
            "eexec" => {
                self.pop_value().ok(); // pop currentfile operand
                let remaining = lexer.remaining_bytes();
                let end_offset = if let Some(idx) = remaining.windows(16).position(|w| w == b"0000000000000000") {
                    idx
                } else if let Some(idx) = remaining.windows(11).position(|w| w == b"cleartomark") {
                    idx
                } else {
                    remaining.len()
                };
                let eexec_data = &remaining[..end_offset];
                let decrypted = Type1Cipher::decrypt_eexec(eexec_data);
                self.parse_eexec_font_data(&decrypted)?;
                lexer.set_position(lexer.position() + end_offset);
            }

            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_eexec_font_data(&mut self, decrypted: &[u8]) -> PsResult<()> {
        let mut font_dict_opt = None;
        for val in self.operand_stack.iter().rev() {
            if let Value::Dict(d) = val {
                if d.borrow().contains_key("FontName") {
                    font_dict_opt = Some(d.clone());
                    break;
                }
            }
        }

        let font_name = if let Some(d) = &font_dict_opt {
            d.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default()
        } else {
            self.lookup_dict("FontName").map(|v| v.as_str_lossy()).unwrap_or_default()
        };

        let encoding_arr = if let Some(d) = &font_dict_opt {
            d.borrow().get("Encoding").and_then(|enc_val| {
                if let Value::Array(arr) = enc_val {
                    let strings: Vec<String> = arr.borrow().iter().map(|v| v.as_str_lossy()).collect();
                    Some(strings)
                } else {
                    None
                }
            })
        } else {
            self.lookup_dict("Encoding").and_then(|enc_val| {
                if let Value::Array(arr) = enc_val {
                    let strings: Vec<String> = arr.borrow().iter().map(|v| v.as_str_lossy()).collect();
                    Some(strings)
                } else {
                    None
                }
            })
        };

        if !font_name.is_empty() {
            if let Ok((subrs, charstrings)) = crate::font::Type1Parser::parse_eexec_data(decrypted, &font_name) {
                let font = self.font_directory.entry(font_name.clone()).or_insert_with(|| FontFace::new(&font_name));
                font.subrs = subrs;
                font.charstrings.extend(charstrings);

                if let Some(enc_strings) = encoding_arr {
                    for (i, v) in enc_strings.into_iter().enumerate() {
                        if i < font.encoding.len() {
                            font.encoding[i] = v;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn execute_proc_value(&mut self, val: Value, lexer: &mut Lexer) -> PsResult<()> {
        match val {
            Value::ExecutableArray(proc) => {
                let items = proc.clone();
                for item in items.iter() {
                    self.eval_value(item.clone(), lexer)?;
                }
            }
            Value::Name(n) => self.execute_name(&n, lexer)?,
            other => self.operand_stack.push(other),
        }
        Ok(())
    }

    fn pop_value(&mut self) -> PsResult<Value> {
        self.operand_stack.pop().ok_or(PsError::StackUnderflow)
    }

    fn pop_num(&mut self) -> PsResult<f64> {
        let val = self.pop_value()?;
        val.as_f64()
    }

    fn pop_i64(&mut self) -> PsResult<i64> {
        let val = self.pop_value()?;
        val.as_i64()
    }

    fn pop_bool(&mut self) -> PsResult<bool> {
        let val = self.pop_value()?;
        val.as_bool()
    }

    fn pop_dict(&mut self) -> PsResult<Rc<RefCell<HashMap<String, Value>>>> {
        let val = self.pop_value()?;
        match val {
            Value::Dict(d) => Ok(d),
            _ => Err(PsError::TypeCheck { expected: "dict", got: val.type_name().to_string() }),
        }
    }

    fn pop_key_name(&mut self) -> PsResult<String> {
        let val = self.pop_value()?;
        match val {
            Value::LiteralName(n) | Value::Name(n) => Ok(n),
            Value::String(s) => Ok(String::from_utf8_lossy(&s).to_string()),
            _ => Err(PsError::TypeCheck { expected: "name", got: val.type_name().to_string() }),
        }
    }

    fn op_pop(&mut self) -> PsResult<()> {
        self.pop_value()?;
        Ok(())
    }

    fn op_dup(&mut self) -> PsResult<()> {
        let val = self.operand_stack.last().cloned().ok_or(PsError::StackUnderflow)?;
        self.operand_stack.push(val);
        Ok(())
    }

    fn op_exch(&mut self) -> PsResult<()> {
        let len = self.operand_stack.len();
        if len < 2 {
            return Err(PsError::StackUnderflow);
        }
        self.operand_stack.swap(len - 1, len - 2);
        Ok(())
    }

    fn op_copy(&mut self) -> PsResult<()> {
        let count = self.pop_i64()? as usize;
        let len = self.operand_stack.len();
        if len < count {
            return Err(PsError::StackUnderflow);
        }
        let items = self.operand_stack[len - count..].to_vec();
        self.operand_stack.extend(items);
        Ok(())
    }

    fn op_index(&mut self) -> PsResult<()> {
        let idx = self.pop_i64()? as usize;
        let len = self.operand_stack.len();
        if idx >= len {
            return Err(PsError::StackUnderflow);
        }
        let val = self.operand_stack[len - 1 - idx].clone();
        self.operand_stack.push(val);
        Ok(())
    }

    fn op_roll(&mut self) -> PsResult<()> {
        let shift = self.pop_i64()?;
        let n = self.pop_i64()? as usize;
        let len = self.operand_stack.len();
        if len < n || n == 0 {
            return Err(PsError::StackUnderflow);
        }

        let slice = &mut self.operand_stack[len - n..];
        let normalized = (shift % n as i64 + n as i64) as usize % n;
        slice.rotate_right(normalized);
        Ok(())
    }

    fn op_cleartomark(&mut self) -> PsResult<()> {
        while let Some(val) = self.operand_stack.pop() {
            if matches!(val, Value::Mark) {
                return Ok(());
            }
        }
        self.operand_stack.clear();
        Ok(())
    }

    fn op_close_bracket(&mut self) -> PsResult<()> {
        let mut items = Vec::new();
        while let Some(val) = self.operand_stack.pop() {
            if matches!(val, Value::Mark) {
                items.reverse();
                self.operand_stack.push(Value::new_array(items));
                return Ok(());
            }
            items.push(val);
        }
        Err(PsError::StackUnderflow)
    }

    fn op_close_dict(&mut self) -> PsResult<()> {
        let mut pairs: Vec<Value> = Vec::new();
        while let Some(val) = self.operand_stack.pop() {
            if matches!(val, Value::Mark) {
                if pairs.len() % 2 != 0 {
                    return Err(PsError::SyntaxError("odd number of key/value elements in dict".to_string()));
                }
                let dict = Value::new_dict();
                if let Value::Dict(d) = &dict {
                    let mut d_mut = d.borrow_mut();
                    for chunk in pairs.chunks(2).rev() {
                        let key = chunk[1].as_str_lossy();
                        let val = chunk[0].clone();
                        d_mut.insert(key, val);
                    }
                }
                self.operand_stack.push(dict);
                return Ok(());
            }
            pairs.push(val);
        }
        Err(PsError::StackUnderflow)
    }

    fn op_bin_num<F>(&mut self, op: F) -> PsResult<()>
    where
        F: FnOnce(f64, f64) -> PsResult<f64>,
    {
        let b = self.pop_num()?;
        let a = self.pop_num()?;
        let res = op(a, b)?;
        self.operand_stack.push(Value::Real(res));
        Ok(())
    }

    fn op_bin_bool<F>(&mut self, op: F) -> PsResult<()>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let b = self.pop_num()?;
        let a = self.pop_num()?;
        self.operand_stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    fn val_to_matrix(&self, val: Value) -> PsResult<Matrix2D> {
        if let Value::Array(a) = val {
            let arr = a.borrow();
            if arr.len() >= 6 {
                return Ok(Matrix2D::new(
                    arr[0].as_f64()?,
                    arr[1].as_f64()?,
                    arr[2].as_f64()?,
                    arr[3].as_f64()?,
                    arr[4].as_f64()?,
                    arr[5].as_f64()?,
                ));
            }
        }
        Err(PsError::TypeCheck { expected: "6-element array matrix", got: "other".to_string() })
    }

    fn matrix_to_val(&self, m: Matrix2D) -> Value {
        Value::new_array(vec![
            Value::Real(m.a),
            Value::Real(m.b),
            Value::Real(m.c),
            Value::Real(m.d),
            Value::Real(m.tx),
            Value::Real(m.ty),
        ])
    }
}
