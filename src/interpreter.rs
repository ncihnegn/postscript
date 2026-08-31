use crate::error::{PsError, PsResult};
use crate::font::FontFace;
use crate::font::eexec::Type1Cipher;
use crate::gstate::{Color, GraphicsState, LineCap, LineJoin};
use crate::lexer::{Lexer, Token};
use crate::matrix::Matrix2D;
use crate::path::{Path, PathSegment};
use crate::render::RenderTarget;
use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone)]
pub struct SaveSnapshot {
    pub id: u64,
    pub dicts: Vec<(Rc<RefCell<HashMap<String, Value>>>, HashMap<String, Value>)>,
    pub current_gstate: GraphicsState,
    pub gstate_stack: Vec<GraphicsState>,
}

pub struct Interpreter {
    pub operand_stack: Vec<Value>,
    pub dict_stack: Vec<Rc<RefCell<HashMap<String, Value>>>>,
    pub gstate_stack: Vec<GraphicsState>,
    pub current_gstate: GraphicsState,
    pub font_directory: HashMap<String, FontFace>,
    pub render_target: RenderTarget,
    pub pages_rendered: Vec<RenderTarget>,
    pub initial_ctm: Matrix2D,
    pub save_stack: Vec<SaveSnapshot>,
    pub next_save_id: u64,
    pub font_instances: Vec<FontFace>,
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
            save_stack: Vec::with_capacity(8),
            next_save_id: 1,
            font_instances: Vec::with_capacity(32),
        };

        // Initialize systemdict, userdict, statusdict, globaldict
        let systemdict = Rc::new(RefCell::new(HashMap::new()));
        let userdict = Rc::new(RefCell::new(HashMap::new()));
        let statusdict = Rc::new(RefCell::new(HashMap::new()));
        let globaldict = Rc::new(RefCell::new(HashMap::new()));
        let fontdir = Rc::new(RefCell::new(HashMap::new()));

        statusdict.borrow_mut().insert("product".to_string(), Value::String(b"macDVI PostScript".to_vec()));
        statusdict.borrow_mut().insert("version".to_string(), Value::String(b"3010".to_vec()));
        statusdict.borrow_mut().insert("revision".to_string(), Value::Integer(1));
        statusdict.borrow_mut().insert("pagecount".to_string(), Value::Integer(0));
        statusdict.borrow_mut().insert("waittimeout".to_string(), Value::Integer(300));
        statusdict.borrow_mut().insert("manualfeed".to_string(), Value::Bool(false));
        statusdict.borrow_mut().insert("jobname".to_string(), Value::String(b"".to_vec()));

        systemdict.borrow_mut().insert("systemdict".to_string(), Value::Dict(systemdict.clone()));
        systemdict.borrow_mut().insert("userdict".to_string(), Value::Dict(userdict.clone()));
        systemdict.borrow_mut().insert("statusdict".to_string(), Value::Dict(statusdict.clone()));
        systemdict.borrow_mut().insert("globaldict".to_string(), Value::Dict(globaldict.clone()));
        systemdict.borrow_mut().insert("FontDirectory".to_string(), Value::Dict(fontdir.clone()));
        systemdict.borrow_mut().insert("product".to_string(), Value::String(b"macDVI PostScript".to_vec()));
        systemdict.borrow_mut().insert("version".to_string(), Value::String(b"3010".to_vec()));
        systemdict.borrow_mut().insert("revision".to_string(), Value::Integer(1));
        systemdict.borrow_mut().insert("languagelevel".to_string(), Value::Integer(2));
        for type_name in [
            "arraytype",
            "booleantype",
            "dicttype",
            "filetype",
            "fonttype",
            "gstatetype",
            "integertype",
            "marktype",
            "nametype",
            "nulltype",
            "operatortype",
            "packedarraytype",
            "realtype",
            "savetype",
            "stringtype",
        ] {
            systemdict
                .borrow_mut()
                .insert(type_name.to_string(), Value::LiteralName(type_name.to_string()));
        }
        systemdict.borrow_mut().insert(
            "StandardEncoding".to_string(),
            Value::new_array(vec![Value::LiteralName(".notdef".to_string()); 256]),
        );

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
            Value::ImmediateName(name) => {
                let value = self
                    .lookup_dict(&name)
                    .ok_or_else(|| PsError::Undefined(name.clone()))?;
                self.operand_stack.push(value);
            }
            other => {
                self.operand_stack.push(other);
            }
        }
        Ok(())
    }

    pub fn execute_name(&mut self, name: &str, lexer: &mut Lexer) -> PsResult<()> {
        // 1. Look up name in dictionary stack
        if let Some(val) = self.lookup_dict(name) {
            match val {
                Value::ExecutableArray(proc) => {
                    let items = proc.borrow().clone();
                    for item in items {
                        self.eval_value(item, lexer)?;
                    }
                    return Ok(());
                }
                Value::Name(alias) => {
                    if alias != name {
                        return self.execute_name(&alias, lexer);
                    }
                }
                other => {
                    self.operand_stack.push(other);
                    return Ok(());
                }
            }
        }

        // 2. Built-in operators dispatch
        match self.dispatch_builtin(name, lexer) {
            Ok(true) => return Ok(()),
            Err(e) => {
                let stack_tail = self
                    .operand_stack
                    .iter()
                    .rev()
                    .take(8)
                    .map(|value| format!("{value:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(PsError::SyntaxError(format!(
                    "operator '{}' failed at byte {}: {} (stack top: [{}])",
                    name,
                    lexer.position(),
                    e,
                    stack_tail
                )));
            }
            Ok(false) => {}
        }

        // 3. Ignore unknown pdfmark/BDC or DSC directives safely
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
            "counttomark" => {
                let count = self
                    .operand_stack
                    .iter()
                    .rev()
                    .position(|value| matches!(value, Value::Mark))
                    .ok_or(PsError::StackUnderflow)?;
                self.operand_stack.push(Value::Integer(count as i64));
            }
            "mark" | "[" | "<<" => self.operand_stack.push(Value::Mark),
            "cleartomark" => self.op_cleartomark()?,
            "]" => self.op_close_bracket()?,
            ">>" => self.op_close_dict()?,

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
            "bitshift" => {
                let shift = self.pop_i64()?;
                let value = self.pop_i64()?;
                let shifted = if shift >= 0 {
                    value.wrapping_shl(shift.min(63) as u32)
                } else {
                    value.wrapping_shr((-shift).min(63) as u32)
                };
                self.operand_stack.push(Value::Integer(shifted));
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
                let dict = self.pop_value()?;
                let is_known = match dict {
                    Value::Dict(dict) => dict.borrow().contains_key(&key),
                    Value::Null => false,
                    other => {
                        return Err(PsError::TypeCheck {
                            expected: "dict",
                            got: other.type_name().to_string(),
                        });
                    }
                };
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
                            Value::Integer(i) => i.to_string(),
                            Value::Real(r) => r.to_string(),
                            _ => return Err(PsError::TypeCheck { expected: "key", got: key_or_index.type_name().to_string() }),
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
                    Value::ExecutableArray(a) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let arr = a.borrow();
                        let val = arr.get(idx).cloned().ok_or_else(|| PsError::RangeCheck("array index out of bounds".to_string()))?;
                        self.operand_stack.push(val);
                    }
                    Value::String(s) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let val = s.get(idx).copied().ok_or_else(|| PsError::RangeCheck("string index out of bounds".to_string()))?;
                        self.operand_stack.push(Value::Integer(val as i64));
                    }
                    _ => return Err(PsError::TypeCheck { expected: "dict, array or string", got: container.type_name().to_string() }),
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
                            Value::Integer(i) => i.to_string(),
                            Value::Real(r) => r.to_string(),
                            _ => return Err(PsError::TypeCheck { expected: "key", got: key_or_index.type_name().to_string() }),
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
                    Value::ExecutableArray(a) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let mut arr = a.borrow_mut();
                        if idx < arr.len() {
                            arr[idx] = val;
                        } else {
                            return Err(PsError::RangeCheck("array index out of bounds".to_string()));
                        }
                    }
                    Value::String(mut string) => {
                        let idx = key_or_index.as_i64()? as usize;
                        let byte = val.as_i64()? as u8;
                        if idx >= string.len() {
                            return Err(PsError::RangeCheck("string index out of bounds".to_string()));
                        }

                        let original = string.clone();
                        string[idx] = byte;

                        // Strings are currently represented by value rather
                        // than a shared composite object. Keep duplicated
                        // operand-stack copies synchronized for idioms such
                        // as `dup 0 71 put cvn`.
                        for stack_value in &mut self.operand_stack {
                            if let Value::String(other) = stack_value {
                                if *other == original {
                                    other[idx] = byte;
                                }
                            }
                        }
                    }
                    _ => return Err(PsError::TypeCheck { expected: "dict or array", got: container.type_name().to_string() }),
                }
            }
            "store" => {
                let val = self.pop_value()?;
                let key = self.pop_key_name()?;
                let mut found = false;
                for dict in self.dict_stack.iter().rev() {
                    if dict.borrow().contains_key(&key) {
                        dict.borrow_mut().insert(key.clone(), val.clone());
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.def_in_current_dict(key, val)?;
                }
            }
            "where" => {
                let mut key = self.pop_key_name()?;
                if key.starts_with("//") {
                    key = key.trim_start_matches('/').to_string();
                }
                let mut found_dict = None;
                for dict in self.dict_stack.iter().rev() {
                    if dict.borrow().contains_key(&key) {
                        found_dict = Some(dict.clone());
                        break;
                    }
                }
                if let Some(d) = found_dict {
                    self.operand_stack.push(Value::Dict(d));
                    self.operand_stack.push(Value::Bool(true));
                } else {
                    self.operand_stack.push(Value::Bool(false));
                }
            }
            "load" => {
                let mut key = self.pop_key_name()?;
                if key.starts_with("//") {
                    key = key.trim_start_matches('/').to_string();
                }
                if let Some(val) = self.lookup_dict(&key) {
                    self.operand_stack.push(val);
                } else {
                    self.operand_stack.push(Value::Name(key));
                }
            }
            "countdictstack" => {
                self.operand_stack.push(Value::Integer(self.dict_stack.len() as i64));
            }
            "maxlength" => {
                let dict = self.pop_dict()?;
                let len = dict.borrow().len().max(100);
                self.operand_stack.push(Value::Integer(len as i64));
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
            "statusdict" => {
                if let Some(sys) = self.dict_stack.first() {
                    if let Some(s) = sys.borrow().get("statusdict") {
                        self.operand_stack.push(s.clone());
                    } else {
                        self.operand_stack.push(Value::new_dict());
                    }
                }
            }
            "globaldict" => {
                if let Some(sys) = self.dict_stack.first() {
                    if let Some(g) = sys.borrow().get("globaldict") {
                        self.operand_stack.push(g.clone());
                    } else {
                        self.operand_stack.push(Value::new_dict());
                    }
                }
            }
            "currentglobal" => {
                // The native VM currently uses a single dictionary/memory
                // space. Report local VM mode and accept setglobal below so
                // embedded Ghostscript-generated EPS prologs can initialize.
                self.operand_stack.push(Value::Bool(false));
            }
            "setglobal" => {
                let _global = self.pop_bool()?;
            }
            "gstate" => {
                // Use a lightweight placeholder for PostScript gstate
                // objects. The renderer already models the active graphics
                // state directly, so embedded EPS setup can pass this object
                // through setgstate without requiring a second VM type.
                self.operand_stack.push(Value::new_dict());
            }
            "setgstate" => {
                let _state = self.pop_value()?;
            }
            "currentblackgeneration" | "currentundercolorremoval" | "currentcolortransfer" => {
                self.operand_stack.push(Value::new_proc(Vec::new()));
            }
            "currenthalftone" => {
                self.operand_stack.push(Value::new_dict());
            }
            "currentpagedevice" => {
                self.operand_stack.push(Value::new_dict());
            }
            "defineresource" => {
                let _category = self.pop_key_name()?;
                let value = self.pop_value()?;
                self.operand_stack.push(value);
            }
            "findresource" => {
                let category = self.pop_key_name()?;
                let name = self.pop_key_name()?;
                let value = if category == "Encoding" {
                    self.lookup_dict(&name).unwrap_or_else(|| Value::new_array(vec![
                        Value::LiteralName(".notdef".to_string());
                        256
                    ]))
                } else {
                    Value::new_dict()
                };
                self.operand_stack.push(value);
            }
            "resourcestatus" => {
                let _category = self.pop_key_name()?;
                let _name = self.pop_key_name()?;
                self.operand_stack.push(Value::Bool(false));
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
                    Value::ExecutableArray(a) => a.borrow().len(),
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
            "string" => {
                let len = self.pop_i64()? as usize;
                self.operand_stack.push(Value::String(vec![0; len]));
            }
            "getinterval" => {
                let count = self.pop_i64()? as usize;
                let index = self.pop_i64()? as usize;
                let container = self.pop_value()?;
                match container {
                    Value::Array(a) => {
                        let arr = a.borrow();
                        let sub = arr.iter().skip(index).take(count).cloned().collect();
                        self.operand_stack.push(Value::new_array(sub));
                    }
                    Value::String(s) => {
                        let sub = s.into_iter().skip(index).take(count).collect();
                        self.operand_stack.push(Value::String(sub));
                    }
                    _ => return Err(PsError::TypeCheck { expected: "array or string", got: container.type_name().to_string() }),
                }
            }
            "putinterval" => {
                let sub_val = self.pop_value()?;
                let index = self.pop_i64()? as usize;
                let container = self.pop_value()?;
                match (container, sub_val) {
                    (Value::Array(a), Value::Array(sub)) => {
                        let mut arr = a.borrow_mut();
                        let sub_arr = sub.borrow();
                        for (i, item) in sub_arr.iter().enumerate() {
                            if index + i < arr.len() {
                                arr[index + i] = item.clone();
                            } else {
                                arr.push(item.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            "search" => {
                let seek_val = self.pop_value()?;
                let str_val = self.pop_value()?;
                match (str_val, seek_val) {
                    (Value::String(s), Value::String(seek)) => {
                        if seek.is_empty() {
                            self.operand_stack.push(Value::String(s));
                            self.operand_stack.push(Value::String(Vec::new()));
                            self.operand_stack.push(Value::String(Vec::new()));
                            self.operand_stack.push(Value::Bool(true));
                        } else if let Some(pos) = s.windows(seek.len()).position(|window| window == seek.as_slice()) {
                            let pre = s[..pos].to_vec();
                            let match_str = s[pos..pos + seek.len()].to_vec();
                            let post = s[pos + seek.len()..].to_vec();
                            self.operand_stack.push(Value::String(post));
                            self.operand_stack.push(Value::String(match_str));
                            self.operand_stack.push(Value::String(pre));
                            self.operand_stack.push(Value::Bool(true));
                        } else {
                            self.operand_stack.push(Value::String(s));
                            self.operand_stack.push(Value::Bool(false));
                        }
                    }
                    (s, seek) => {
                        return Err(PsError::TypeCheck {
                            expected: "string and string",
                            got: format!("{} and {}", s.type_name(), seek.type_name()),
                        });
                    }
                }
            }
            "anchorsearch" => {
                let seek_val = self.pop_value()?;
                let str_val = self.pop_value()?;
                match (str_val, seek_val) {
                    (Value::String(s), Value::String(seek)) => {
                        if s.starts_with(&seek) {
                            let match_str = s[..seek.len()].to_vec();
                            let post = s[seek.len()..].to_vec();
                            self.operand_stack.push(Value::String(post));
                            self.operand_stack.push(Value::String(match_str));
                            self.operand_stack.push(Value::Bool(true));
                        } else {
                            self.operand_stack.push(Value::String(s));
                            self.operand_stack.push(Value::Bool(false));
                        }
                    }
                    (s, seek) => {
                        return Err(PsError::TypeCheck {
                            expected: "string and string",
                            got: format!("{} and {}", s.type_name(), seek.type_name()),
                        });
                    }
                }
            }
            "readhexstring" => {
                let str_buf = self.pop_value()?;
                let _file = self.pop_value().ok();
                let len = match &str_buf {
                    Value::String(s) => s.len(),
                    _ => 0,
                };
                let (hex_bytes, not_eof) = lexer.read_hex_bytes(len);
                self.operand_stack.push(Value::String(hex_bytes));
                self.operand_stack.push(Value::Bool(not_eof));
            }
            "filter" => {
                let filter_name = self.pop_key_name()?;
                let source = self.pop_value()?;
                let dict = Rc::new(RefCell::new(HashMap::new()));
                dict.borrow_mut().insert("Type".to_string(), Value::LiteralName("Filter".to_string()));
                dict.borrow_mut().insert("Filter".to_string(), Value::LiteralName(filter_name));
                dict.borrow_mut().insert("Source".to_string(), source);
                self.operand_stack.push(Value::Dict(dict));
            }
            "setcolorspace" => {
                self.pop_value().ok();
            }
            "setcolor" => {
                if let Ok(n1) = self.pop_num() {
                    if let Ok(n2) = self.pop_num() {
                        if let Ok(n3) = self.pop_num() {
                            if let Ok(n4) = self.pop_num() {
                                self.current_gstate.color = Color::cmyk(n4, n3, n2, n1);
                            } else {
                                self.current_gstate.color = Color::rgb(n3, n2, n1);
                            }
                        } else {
                            self.current_gstate.color = Color::gray(n1);
                        }
                    } else {
                        self.current_gstate.color = Color::gray(n1);
                    }
                }
            }
            "image" => {
                let top = self.pop_value()?;
                match top {
                    Value::Dict(d) => {
                        let w = d.borrow().get("Width").and_then(|v| v.as_i64().ok()).unwrap_or(1) as usize;
                        let h = d.borrow().get("Height").and_then(|v| v.as_i64().ok()).unwrap_or(1) as usize;

                        let data_source = d.borrow().get("DataSource").cloned();
                        let raw_bytes = read_image_data(lexer, data_source.as_ref());

                        let mut rgba = Vec::with_capacity(w * h * 4);
                        if let Ok(dyn_img) = image::load_from_memory(&raw_bytes) {
                            let rgba_img = dyn_img.to_rgba8();
                            rgba = rgba_img.into_raw();
                        } else {
                            let decompressed = if raw_bytes.starts_with(&[0x78]) {
                                miniz_oxide::inflate::decompress_to_vec_zlib(&raw_bytes).unwrap_or(raw_bytes)
                            } else {
                                raw_bytes
                            };

                            if decompressed.len() >= w * h * 3 {
                                for chunk in decompressed.chunks_exact(3).take(w * h) {
                                    rgba.push(chunk[0]);
                                    rgba.push(chunk[1]);
                                    rgba.push(chunk[2]);
                                    rgba.push(255);
                                }
                            } else if decompressed.len() >= w * h {
                                for &g in decompressed.iter().take(w * h) {
                                    rgba.push(g);
                                    rgba.push(g);
                                    rgba.push(g);
                                    rgba.push(255);
                                }
                            }
                        }

                        if !rgba.is_empty() {
                            let img_matrix = if let Some(Value::Array(arr)) = d.borrow().get("ImageMatrix") {
                                if arr.borrow().len() >= 6 {
                                    let a = arr.borrow()[0].as_f64().unwrap_or(w as f64);
                                    let b = arr.borrow()[1].as_f64().unwrap_or(0.0);
                                    let c = arr.borrow()[2].as_f64().unwrap_or(0.0);
                                    let d_val = arr.borrow()[3].as_f64().unwrap_or(h as f64);
                                    let tx = arr.borrow()[4].as_f64().unwrap_or(0.0);
                                    let ty = arr.borrow()[5].as_f64().unwrap_or(0.0);
                                    Matrix2D::new(a, b, c, d_val, tx, ty).inverse().unwrap_or_else(|| Matrix2D::scale(1.0 / (w as f64), 1.0 / (h as f64)))
                                } else {
                                    Matrix2D::scale(1.0 / (w as f64), 1.0 / (h as f64))
                                }
                            } else {
                                Matrix2D::scale(1.0 / (w as f64), 1.0 / (h as f64))
                            };
                            let transform = img_matrix.concat(&self.current_gstate.ctm);
                            self.render_target.push_image(
                                w as u32,
                                h as u32,
                                rgba,
                                transform,
                                self.current_gstate.clip_paths.clone(),
                            );
                        }
                    }
                    proc => {
                        let _mat_val = self.pop_value()?;
                        let _bits = self.pop_num()?;
                        let h = self.pop_num().unwrap_or(1.0) as usize;
                        let w = self.pop_num().unwrap_or(1.0) as usize;
                        let total_needed = w * h;
                        let mut bytes_read = 0;
                        while bytes_read < total_needed {
                            self.execute_proc_value(proc.clone(), lexer)?;
                            if let Ok(val) = self.pop_value() {
                                match val {
                                    Value::String(s) => {
                                        if s.is_empty() {
                                            break;
                                        }
                                        bytes_read += s.len();
                                    }
                                    _ => break,
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }

            // Type & Conversion
            "type" => {
                let val = self.pop_value()?;
                let t_name = match val {
                    Value::Integer(_) => "integertype",
                    Value::Real(_) => "realtype",
                    Value::Bool(_) => "booleantype",
                    Value::String(_) => "stringtype",
                    Value::LiteralName(_) | Value::Name(_) | Value::ImmediateName(_) => "nametype",
                    Value::Dict(_) => "dicttype",
                    Value::Array(_) | Value::ExecutableArray(_) => "arraytype",
                    Value::Mark => "marktype",
                    Value::Null => "nulltype",
                };
                self.operand_stack.push(Value::LiteralName(t_name.to_string()));
            }
            "cvn" => {
                let val = self.pop_value()?;
                let name = val.as_str_lossy();
                self.operand_stack.push(Value::LiteralName(name));
            }
            "cvi" => {
                let val = self.pop_value()?;
                let i = val.as_i64()?;
                self.operand_stack.push(Value::Integer(i));
            }
            "cvr" => {
                let val = self.pop_value()?;
                let r = val.as_f64()?;
                self.operand_stack.push(Value::Real(r));
            }
            "cvs" => {
                let _str_buf = self.pop_value()?;
                let val = self.pop_value()?;
                let s = match val {
                    Value::Integer(i) => i.to_string(),
                    Value::Real(r) => r.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::String(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Value::LiteralName(n) | Value::Name(n) => n,
                    _ => "".to_string(),
                };
                self.operand_stack.push(Value::String(s.into_bytes()));
            }
            "cvrs" => {
                let mut str_buf = self.pop_value()?;
                let radix = self.pop_i64()? as u32;
                let num = self.pop_num()?;
                let int_val = num.round() as i64;
                let s = if radix >= 2 && radix <= 36 {
                    format_radix(int_val, radix)
                } else {
                    int_val.to_string()
                };
                let bytes = s.into_bytes();
                let result = if let Value::String(ref mut dest) = str_buf {
                    let len = bytes.len().min(dest.len());
                    dest[..len].copy_from_slice(&bytes[..len]);
                    dest[..len].to_vec()
                } else {
                    bytes
                };
                self.operand_stack.push(Value::String(result));
            }
            "cvx" => {
                let val = self.pop_value()?;
                let exe = match val {
                    Value::LiteralName(n) => Value::Name(n),
                    Value::Array(a) => Value::new_proc(a.borrow().clone()),
                    other => other,
                };
                self.operand_stack.push(exe);
            }
            "cvlit" => {
                let val = self.pop_value()?;
                let lit = match val {
                    Value::Name(n) => Value::LiteralName(n),
                    Value::ExecutableArray(a) => Value::new_array(a.borrow().clone()),
                    other => other,
                };
                self.operand_stack.push(lit);
            }
            "readonly" | "executeonly" | "noaccess" => {
                // Return operand itself
            }
            "gcheck" | "rcheck" | "wcheck" | "xcheck" => {
                let _val = self.pop_value()?;
                self.operand_stack.push(Value::Bool(true));
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
                        match self.execute_proc_value(proc.clone(), lexer) {
                            Ok(()) => {}
                            Err(PsError::Exit) => break,
                            Err(e) => return Err(e),
                        }
                        current += step;
                    }
                } else if step < 0.0 {
                    while current >= limit {
                        self.operand_stack.push(Value::Real(current));
                        match self.execute_proc_value(proc.clone(), lexer) {
                            Ok(()) => {}
                            Err(PsError::Exit) => break,
                            Err(e) => return Err(e),
                        }
                        current += step;
                    }
                }
            }
            "repeat" => {
                let proc = self.pop_value()?;
                let count = self.pop_i64()?;
                for _ in 0..count.max(0) {
                    match self.execute_proc_value(proc.clone(), lexer) {
                        Ok(()) => {}
                        Err(PsError::Exit) => break,
                        Err(e) => return Err(e),
                    }
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
            "forall" => {
                let proc = self.pop_value()?;
                let collection = self.pop_value()?;
                match collection {
                    Value::Array(arr) => {
                        let items = arr.borrow().clone();
                        for item in items {
                            self.operand_stack.push(item);
                            match self.execute_proc_value(proc.clone(), lexer) {
                                Ok(()) => {}
                                Err(PsError::Exit) => break,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    Value::ExecutableArray(arr) => {
                        let items = arr.borrow().clone();
                        for item in items {
                            self.operand_stack.push(item);
                            match self.execute_proc_value(proc.clone(), lexer) {
                                Ok(()) => {}
                                Err(PsError::Exit) => break,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    Value::Dict(d) => {
                        let pairs: Vec<(String, Value)> = d.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                        for (k, v) in pairs {
                            self.operand_stack.push(Value::LiteralName(k));
                            self.operand_stack.push(v);
                            match self.execute_proc_value(proc.clone(), lexer) {
                                Ok(()) => {}
                                Err(PsError::Exit) => break,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    Value::String(s) => {
                        let bytes = s.clone();
                        for b in bytes {
                            self.operand_stack.push(Value::Integer(b as i64));
                            match self.execute_proc_value(proc.clone(), lexer) {
                                Ok(()) => {}
                                Err(PsError::Exit) => break,
                                Err(e) => return Err(e),
                            }
                        }
                    }
                    _ => return Err(PsError::TypeCheck { expected: "array, dict or string", got: collection.type_name().to_string() }),
                }
            }
            "exit" => return Err(PsError::Exit),
            "stop" => return Err(PsError::Stop),
            "stopped" => {
                let proc = self.pop_value()?;
                let operand_stack_len = self.operand_stack.len();
                match self.execute_proc_value(proc, lexer) {
                    Ok(()) => self.operand_stack.push(Value::Bool(false)),
                    Err(PsError::Stop) | Err(_) => {
                        self.operand_stack.truncate(operand_stack_len);
                        self.operand_stack.push(Value::Bool(true));
                    }
                }
            }
            "exec" => {
                let val = self.pop_value()?;
                self.execute_proc_value(val, lexer)?;
            }
            "bind" => {
                // Optimization / no-op in interpreter
            }
            "save" => {
                let id = self.next_save_id;
                self.next_save_id += 1;
                let mut dicts = Vec::new();
                for d in &self.dict_stack {
                    dicts.push((d.clone(), d.borrow().clone()));
                }
                self.save_stack.push(SaveSnapshot {
                    id,
                    dicts,
                    current_gstate: self.current_gstate.clone(),
                    gstate_stack: self.gstate_stack.clone(),
                });
                self.operand_stack.push(Value::Integer(id as i64));
            }
            "restore" => {
                let val = self.pop_value()?;
                let save_id = val.as_i64().unwrap_or(0) as u64;
                if let Some(pos) = self.save_stack.iter().rposition(|s| s.id == save_id) {
                    let snapshot = self.save_stack.remove(pos);
                    self.save_stack.truncate(pos);
                    for (dict_rc, saved_map) in snapshot.dicts {
                        *dict_rc.borrow_mut() = saved_map;
                    }
                    self.current_gstate = snapshot.current_gstate;
                    self.gstate_stack = snapshot.gstate_stack;
                }
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
                self.current_gstate.subpath_start = None;
            }
            "moveto" => {
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                let point = self.current_gstate.ctm.transform_point(x, y);
                self.current_gstate.current_path.move_to(point.0, point.1);
                self.current_gstate.current_point = Some(point);
                self.current_gstate.subpath_start = Some(point);
            }
            "rmoveto" => {
                let dy = self.pop_num()?;
                let dx = self.pop_num()?;
                let (cx, cy) = self.get_current_point_user().unwrap_or((0.0, 0.0));
                let nx = cx + dx;
                let ny = cy + dy;
                let point = self.current_gstate.ctm.transform_point(nx, ny);
                self.current_gstate.current_path.move_to(point.0, point.1);
                self.current_gstate.current_point = Some(point);
                self.current_gstate.subpath_start = Some(point);
            }
            "lineto" => {
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                let point = self.current_gstate.ctm.transform_point(x, y);
                self.current_gstate.current_path.line_to(point.0, point.1);
                self.current_gstate.current_point = Some(point);
            }
            "rlineto" => {
                let dy = self.pop_num()?;
                let dx = self.pop_num()?;
                let (cx, cy) = self.get_current_point_user().unwrap_or((0.0, 0.0));
                let nx = cx + dx;
                let ny = cy + dy;
                let point = self.current_gstate.ctm.transform_point(nx, ny);
                self.current_gstate.current_path.line_to(point.0, point.1);
                self.current_gstate.current_point = Some(point);
            }
            "curveto" => {
                let y3 = self.pop_num()?;
                let x3 = self.pop_num()?;
                let y2 = self.pop_num()?;
                let x2 = self.pop_num()?;
                let y1 = self.pop_num()?;
                let x1 = self.pop_num()?;
                let p1 = self.current_gstate.ctm.transform_point(x1, y1);
                let p2 = self.current_gstate.ctm.transform_point(x2, y2);
                let p3 = self.current_gstate.ctm.transform_point(x3, y3);
                self.current_gstate.current_path.curve_to(
                    p1.0, p1.1, p2.0, p2.1, p3.0, p3.1,
                );
                self.current_gstate.current_point = Some(p3);
            }
            "rcurveto" => {
                let dy3 = self.pop_num()?;
                let dx3 = self.pop_num()?;
                let dy2 = self.pop_num()?;
                let dx2 = self.pop_num()?;
                let dy1 = self.pop_num()?;
                let dx1 = self.pop_num()?;
                let (cx, cy) = self.get_current_point_user().unwrap_or((0.0, 0.0));
                let nx1 = cx + dx1;
                let ny1 = cy + dy1;
                let nx2 = nx1 + dx2;
                let ny2 = ny1 + dy2;
                let nx3 = nx2 + dx3;
                let ny3 = ny2 + dy3;
                let p1 = self.current_gstate.ctm.transform_point(nx1, ny1);
                let p2 = self.current_gstate.ctm.transform_point(nx2, ny2);
                let p3 = self.current_gstate.ctm.transform_point(nx3, ny3);
                self.current_gstate.current_path.curve_to(
                    p1.0, p1.1, p2.0, p2.1, p3.0, p3.1,
                );
                self.current_gstate.current_point = Some(p3);
            }
            "arc" => {
                let angle2 = self.pop_num()?;
                let angle1 = self.pop_num()?;
                let r = self.pop_num()?;
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                let was_empty = self.current_gstate.current_path.is_empty();
                let mut arc = Path::new();
                arc.arc(x, y, r, angle1, angle2, false);
                let mut transformed = arc.transform(&self.current_gstate.ctm);
                if !was_empty {
                    if let Some(PathSegment::MoveTo(x, y)) = transformed.segments.first().cloned() {
                        transformed.segments[0] = PathSegment::LineTo(x, y);
                    }
                }
                self.current_gstate.current_path.append(&transformed);
                let end_x = x + r * angle2.to_radians().cos();
                let end_y = y + r * angle2.to_radians().sin();
                self.set_current_point_user(end_x, end_y);
                if was_empty {
                    if let Some(PathSegment::MoveTo(x, y)) = transformed.segments.first().cloned() {
                        self.current_gstate.subpath_start = Some((x, y));
                    }
                }
            }
            "arcn" => {
                let angle2 = self.pop_num()?;
                let angle1 = self.pop_num()?;
                let r = self.pop_num()?;
                let y = self.pop_num()?;
                let x = self.pop_num()?;
                let was_empty = self.current_gstate.current_path.is_empty();
                let mut arc = Path::new();
                arc.arc(x, y, r, angle1, angle2, true);
                let mut transformed = arc.transform(&self.current_gstate.ctm);
                if !was_empty {
                    if let Some(PathSegment::MoveTo(x, y)) = transformed.segments.first().cloned() {
                        transformed.segments[0] = PathSegment::LineTo(x, y);
                    }
                }
                self.current_gstate.current_path.append(&transformed);
                let end_x = x + r * angle2.to_radians().cos();
                let end_y = y + r * angle2.to_radians().sin();
                self.set_current_point_user(end_x, end_y);
                if was_empty {
                    if let Some(PathSegment::MoveTo(x, y)) = transformed.segments.first().cloned() {
                        self.current_gstate.subpath_start = Some((x, y));
                    }
                }
            }
            "closepath" => {
                self.current_gstate.current_path.close_path();
                self.current_gstate.current_point = self.current_gstate.subpath_start;
            }
            "currentpoint" => {
                if let Some((x, y)) = self.get_current_point_user() {
                    self.operand_stack.push(Value::Real(x));
                    self.operand_stack.push(Value::Real(y));
                } else {
                    return Err(PsError::LimitCheck("currentpoint is undefined".to_string()));
                }
            }

            // Painting
            "fill" => {
                self.render_target.push_fill(
                    self.current_gstate.current_path.clone(),
                    self.current_gstate.color,
                    false,
                    self.current_gstate.clip_paths.clone(),
                );
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
                self.current_gstate.subpath_start = None;
            }
            "eofill" => {
                self.render_target.push_fill(
                    self.current_gstate.current_path.clone(),
                    self.current_gstate.color,
                    true,
                    self.current_gstate.clip_paths.clone(),
                );
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
                self.current_gstate.subpath_start = None;
            }
            "clip" | "eoclip" => {
                if !self.current_gstate.current_path.is_empty() {
                    self.current_gstate.clip_paths.push(crate::gstate::ClipPath {
                        path: self.current_gstate.current_path.clone(),
                        even_odd: name == "eoclip",
                    });
                }
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
                self.current_gstate.subpath_start = None;
            }
            "stroke" => {
                let (scaled_width, _) = self.current_gstate.ctm.transform_vector(self.current_gstate.line_width, 0.0);
                self.render_target.push_stroke(
                    self.current_gstate.current_path.clone(),
                    self.current_gstate.color,
                    scaled_width.abs(),
                    self.current_gstate.line_cap,
                    self.current_gstate.line_join,
                    self.current_gstate.miter_limit,
                    self.current_gstate.clip_paths.clone(),
                );
                self.current_gstate.current_path.clear();
                self.current_gstate.current_point = None;
                self.current_gstate.subpath_start = None;
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
                self.current_gstate.subpath_start = None;
            }

            // Style & Color
            "setlinewidth" => {
                let w = self.pop_num()?;
                self.current_gstate.line_width = w;
            }
            "currentlinewidth" => {
                self.operand_stack.push(Value::Real(self.current_gstate.line_width));
            }
            "setgray" => {
                let g = self.pop_num()?;
                self.current_gstate.color = Color::gray(g);
            }
            "currentgray" => {
                let g = 0.299 * self.current_gstate.color.r + 0.587 * self.current_gstate.color.g + 0.114 * self.current_gstate.color.b;
                self.operand_stack.push(Value::Real(g));
            }
            "setrgbcolor" => {
                let b = self.pop_num()?;
                let g = self.pop_num()?;
                let r = self.pop_num()?;
                self.current_gstate.color = Color::rgb(r, g, b);
            }
            "currentrgbcolor" => {
                self.operand_stack.push(Value::Real(self.current_gstate.color.r));
                self.operand_stack.push(Value::Real(self.current_gstate.color.g));
                self.operand_stack.push(Value::Real(self.current_gstate.color.b));
            }
            "setcmykcolor" => {
                let k = self.pop_num()?;
                let y = self.pop_num()?;
                let m = self.pop_num()?;
                let c = self.pop_num()?;
                self.current_gstate.color = Color::cmyk(c, m, y, k);
            }
            "currentcmykcolor" => {
                let r = self.current_gstate.color.r;
                let g = self.current_gstate.color.g;
                let b = self.current_gstate.color.b;
                let k = 1.0 - r.max(g).max(b);
                if (1.0 - k).abs() < 1e-6 {
                    self.operand_stack.push(Value::Real(0.0));
                    self.operand_stack.push(Value::Real(0.0));
                    self.operand_stack.push(Value::Real(0.0));
                    self.operand_stack.push(Value::Real(1.0));
                } else {
                    let c = (1.0 - r - k) / (1.0 - k);
                    let m = (1.0 - g - k) / (1.0 - k);
                    let y = (1.0 - b - k) / (1.0 - k);
                    self.operand_stack.push(Value::Real(c.clamp(0.0, 1.0)));
                    self.operand_stack.push(Value::Real(m.clamp(0.0, 1.0)));
                    self.operand_stack.push(Value::Real(y.clamp(0.0, 1.0)));
                    self.operand_stack.push(Value::Real(k.clamp(0.0, 1.0)));
                }
            }
            "setlinecap" => {
                let cap = self.pop_i64()?;
                self.current_gstate.line_cap = match cap {
                    1 => LineCap::Round,
                    2 => LineCap::Square,
                    _ => LineCap::Butt,
                };
            }
            "currentlinecap" => {
                let cap = match self.current_gstate.line_cap {
                    LineCap::Butt => 0,
                    LineCap::Round => 1,
                    LineCap::Square => 2,
                };
                self.operand_stack.push(Value::Integer(cap));
            }
            "setlinejoin" => {
                let join = self.pop_i64()?;
                self.current_gstate.line_join = match join {
                    1 => LineJoin::Round,
                    2 => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                };
            }
            "currentlinejoin" => {
                let join = match self.current_gstate.line_join {
                    LineJoin::Miter => 0,
                    LineJoin::Round => 1,
                    LineJoin::Bevel => 2,
                };
                self.operand_stack.push(Value::Integer(join));
            }
            "setmiterlimit" => {
                let limit = self.pop_num()?;
                self.current_gstate.miter_limit = limit;
            }
            "currentmiterlimit" => {
                self.operand_stack.push(Value::Real(self.current_gstate.miter_limit));
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
                let base_font = if let Some(f) = self.font_directory.get(&name)
                    .or_else(|| self.font_directory.get(&name.to_uppercase()))
                    .or_else(|| self.font_directory.get(&name.to_lowercase()))
                {
                    if f.charstrings.is_empty() {
                        crate::font::load_font_by_name(&name).unwrap_or_else(|| f.clone())
                    } else {
                        f.clone()
                    }
                } else {
                    crate::font::load_font_by_name(&name).unwrap_or_else(|| FontFace::new(&name))
                };
                self.font_directory.insert(name.clone(), base_font.clone());
                self.font_instances.push(base_font.clone());
                let font_id = self.font_instances.len() - 1;
                if let Some(type3_dict) = &base_font.type3_dict {
                    type3_dict
                        .borrow_mut()
                        .insert("_FontId".to_string(), Value::Integer(font_id as i64));
                    self.operand_stack.push(Value::Dict(type3_dict.clone()));
                    return Ok(true);
                }
                let dict = Rc::new(RefCell::new(HashMap::new()));
                dict.borrow_mut().insert("FontName".to_string(), Value::LiteralName(name.clone()));
                dict.borrow_mut().insert("FontType".to_string(), Value::Integer(1));
                dict.borrow_mut().insert("PaintType".to_string(), Value::Integer(0));
                dict.borrow_mut().insert("FontMatrix".to_string(), Value::new_array(vec![
                    Value::Real(0.001), Value::Real(0.0), Value::Real(0.0), Value::Real(0.001), Value::Real(0.0), Value::Real(0.0)
                ]));
                dict.borrow_mut().insert("FontBBox".to_string(), Value::new_array(vec![
                    Value::Real(0.0), Value::Real(-250.0), Value::Real(1000.0), Value::Real(750.0)
                ]));
                let enc: Vec<Value> = base_font.encoding.iter().map(|s| Value::LiteralName(s.clone())).collect();
                dict.borrow_mut().insert("Encoding".to_string(), Value::new_array(enc));
                dict.borrow_mut().insert("_FontId".to_string(), Value::Integer(font_id as i64));
                self.operand_stack.push(Value::Dict(dict));
            }
            "scalefont" => {
                let scale = self.pop_num()?;
                let font_dict = self.pop_dict()?;
                let font_id = font_dict.borrow().get("_FontId").and_then(|v| v.as_i64().ok()).map(|i| i as usize);
                let font = if let Some(id) = font_id {
                    self.font_instances.get(id).cloned()
                } else {
                    let font_name = font_dict.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default();
                    self.font_directory.get(&font_name)
                        .or_else(|| self.font_directory.get(&font_name.to_uppercase()))
                        .or_else(|| self.font_directory.get(&font_name.to_lowercase()))
                        .cloned()
                }.unwrap_or_else(|| FontFace::new("default"));
                let scaled_font = font.scalefont(scale);
                self.font_instances.push(scaled_font);
                let new_id = self.font_instances.len() - 1;
                let new_dict = Rc::new(RefCell::new(font_dict.borrow().clone()));
                new_dict.borrow_mut().insert("_FontId".to_string(), Value::Integer(new_id as i64));
                self.operand_stack.push(Value::Dict(new_dict));
            }
            "makefont" => {
                let mat_val = self.pop_value()?;
                let mat = self.val_to_matrix(mat_val)?;
                let font_dict = self.pop_dict()?;
                let font_id = font_dict.borrow().get("_FontId").and_then(|v| v.as_i64().ok()).map(|i| i as usize);
                let font = if let Some(id) = font_id {
                    self.font_instances.get(id).cloned()
                } else {
                    let font_name = font_dict.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default();
                    self.font_directory.get(&font_name)
                        .or_else(|| self.font_directory.get(&font_name.to_uppercase()))
                        .or_else(|| self.font_directory.get(&font_name.to_lowercase()))
                        .cloned()
                }.unwrap_or_else(|| FontFace::new("default"));
                let made_font = font.makefont(mat);
                self.font_instances.push(made_font);
                let new_id = self.font_instances.len() - 1;
                let new_dict = Rc::new(RefCell::new(font_dict.borrow().clone()));
                new_dict.borrow_mut().insert("_FontId".to_string(), Value::Integer(new_id as i64));
                self.operand_stack.push(Value::Dict(new_dict));
            }
            "setfont" => {
                let font_val = self.pop_value()?;
                let font = match font_val {
                    Value::Dict(d) => {
                        let font_id = d.borrow().get("_FontId").and_then(|v| v.as_i64().ok()).map(|i| i as usize);
                        if let Some(id) = font_id {
                            self.font_instances.get(id).cloned()
                        } else {
                            let font_name = d.borrow().get("FontName").map(|v| v.as_str_lossy()).unwrap_or_default();
                            self.font_directory.get(&font_name)
                                .or_else(|| self.font_directory.get(&font_name.to_uppercase()))
                                .or_else(|| self.font_directory.get(&font_name.to_lowercase()))
                                .cloned()
                        }
                    }
                    Value::LiteralName(n) | Value::Name(n) => {
                        self.font_directory.get(&n)
                            .or_else(|| self.font_directory.get(&n.to_uppercase()))
                            .or_else(|| self.font_directory.get(&n.to_lowercase()))
                            .cloned()
                    }
                    _ => None,
                }.unwrap_or_else(|| FontFace::new("default"));
                self.current_gstate.font = Some(font);
            }
            "definefont" => {
                let font_val = self.pop_value()?;
                let key = self.pop_key_name()?;
                let font_dict = match font_val {
                    Value::Dict(d) => d,
                    _ => Rc::new(RefCell::new(HashMap::new())),
                };
                font_dict.borrow_mut().insert("FontName".to_string(), Value::LiteralName(key.clone()));
                if !font_dict.borrow().contains_key("FontType") {
                    font_dict.borrow_mut().insert("FontType".to_string(), Value::Integer(1));
                }

                let base_font = if let Some(base_id) = font_dict.borrow().get("_FontId").and_then(|v| v.as_i64().ok()) {
                    self.font_instances.get(base_id as usize).cloned()
                } else {
                    None
                };

                let mut face = base_font.unwrap_or_else(|| {
                    self.font_directory.get(&key)
                        .or_else(|| self.font_directory.get(&key.to_uppercase()))
                        .or_else(|| self.font_directory.get(&key.to_lowercase()))
                        .cloned()
                        .unwrap_or_else(|| FontFace::new(&key))
                });
                face.name = key.clone();

                let is_type3 = font_dict
                    .borrow()
                    .get("FontType")
                    .and_then(|value| value.as_i64().ok())
                    == Some(3);
                if is_type3 {
                    face.type3_dict = Some(font_dict.clone());
                    if let Some(matrix) = font_dict
                        .borrow()
                        .get("FontMatrix")
                        .cloned()
                        .and_then(|value| self.val_to_matrix(value).ok())
                    {
                        face.matrix = matrix;
                    }
                }

                if let Some(enc_val) = font_dict.borrow().get("Encoding") {
                    if let Value::Array(arr) = enc_val {
                        let strings: Vec<String> = arr.borrow().iter().map(|v| v.as_str_lossy()).collect();
                        for (i, s) in strings.into_iter().enumerate() {
                            if i < face.encoding.len() {
                                face.encoding[i] = s;
                            }
                        }
                    }
                }

                self.font_directory.insert(key.clone(), face.clone());
                self.font_instances.push(face);
                let new_id = self.font_instances.len() - 1;
                font_dict.borrow_mut().insert("_FontId".to_string(), Value::Integer(new_id as i64));
                self.operand_stack.push(Value::Dict(font_dict));
            }
            "setcachedevice" => {
                let _ury = self.pop_num().ok();
                let _urx = self.pop_num().ok();
                let _lly = self.pop_num().ok();
                let _llx = self.pop_num().ok();
                let _wy = self.pop_num().ok();
                let _wx = self.pop_num().ok();
            }
            "setcharwidth" => {
                let _wy = self.pop_num().ok();
                let _wx = self.pop_num().ok();
            }
            "imagemask" => {
                let source = self.pop_value()?;
                let matrix = self.pop_value()?;
                let polarity = self.pop_bool()?;
                let h = self.pop_num().unwrap_or(1.0) as usize;
                let w = self.pop_num().unwrap_or(1.0) as usize;
                let total_needed = (w * h + 7) / 8;
                let data = self.read_image_source(source, total_needed, lexer)?;
                if w > 0 && h > 0 && !data.is_empty() {
                    let image_matrix = self
                        .val_to_matrix(matrix)
                        .unwrap_or_else(|_| Matrix2D::scale(w as f64, h as f64));
                    self.push_image_mask(w, h, &data, polarity, image_matrix);
                }
            }
            "charpath" => {
                let _stroke_bool = self.pop_bool()?;
                let str_val = self.pop_value()?;
                let bytes = match str_val {
                    Value::String(s) => s,
                    Value::Name(n) | Value::LiteralName(n) => n.into_bytes(),
                    _ => return Err(PsError::TypeCheck { expected: "string", got: str_val.type_name().to_string() }),
                };

                let (mut cx, cy) = self.get_current_point_user().unwrap_or((0.0, 0.0));
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
                            self.current_gstate.current_path.append(&transformed);
                            cx += width;
                            continue;
                        }
                    }

                    // Fallback advance for standard characters
                    cx += 6.0;
                }
                self.set_current_point_user(cx, cy);
            }
            "show" => {
                let str_val = self.pop_value()?;
                let bytes = match str_val {
                    Value::String(s) => s,
                    Value::Name(n) | Value::LiteralName(n) => n.into_bytes(),
                    _ => return Err(PsError::TypeCheck { expected: "string", got: str_val.type_name().to_string() }),
                };

                let (mut cx, cy) = self.get_current_point_user().unwrap_or((0.0, 0.0));
                let font = self.current_gstate.font.clone();
                for &b in &bytes {
                    let glyph_name = if let Some(f) = &font {
                        f.encoding.get(b as usize).cloned().unwrap_or_else(|| ".notdef".to_string())
                    } else {
                        (b as char).to_string()
                    };

                    if let Some(f) = &font {
                        if f.type3_dict.is_some() {
                            cx += self.render_type3_glyph(f, &glyph_name, cx, cy);
                            continue;
                        }
                        if let Some((glyph_path, width)) = f.get_glyph_path(&glyph_name) {
                            let placed = glyph_path.transform(&Matrix2D::translate(cx, cy));
                            let transformed = placed.transform(&self.current_gstate.ctm);
                            self.render_target.push_fill(
                                transformed,
                                self.current_gstate.color,
                                false,
                                self.current_gstate.clip_paths.clone(),
                            );
                            cx += width;
                            continue;
                        }
                    }
                    cx += 6.0;
                }
                self.set_current_point_user(cx, cy);
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
                let items = proc.borrow().clone();
                for item in items {
                    self.eval_value(item, lexer)?;
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
            Value::Integer(i) => Ok(i.to_string()),
            Value::Real(r) => Ok(r.to_string()),
            _ => Err(PsError::TypeCheck { expected: "name or key", got: val.type_name().to_string() }),
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
        let top = self.pop_value()?;
        match top {
            Value::Integer(count) => {
                let n = count.max(0) as usize;
                let len = self.operand_stack.len();
                if len < n {
                    return Err(PsError::StackUnderflow);
                }
                let items = self.operand_stack[len - n..].to_vec();
                self.operand_stack.extend(items);
            }
            Value::Array(dest_arr) => {
                let src_val = self.pop_value()?;
                let src_items = match src_val {
                    Value::Array(src_a) => src_a.borrow().clone(),
                    Value::ExecutableArray(src_a) => src_a.borrow().clone(),
                    _ => return Err(PsError::TypeCheck { expected: "array", got: src_val.type_name().to_string() }),
                };
                let mut dest = dest_arr.borrow_mut();
                let copy_len = src_items.len().min(dest.len());
                for i in 0..copy_len {
                    dest[i] = src_items[i].clone();
                }
                self.operand_stack.push(Value::Array(dest_arr.clone()));
            }
            Value::Dict(dest_dict) => {
                let src_val = self.pop_value()?;
                let src_dict = match src_val {
                    Value::Dict(d) => d,
                    _ => return Err(PsError::TypeCheck { expected: "dict", got: src_val.type_name().to_string() }),
                };
                let pairs: Vec<(String, Value)> = src_dict.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (k, v) in pairs {
                    dest_dict.borrow_mut().insert(k, v);
                }
                self.operand_stack.push(Value::Dict(dest_dict));
            }
            Value::String(mut dest_str) => {
                let src_val = self.pop_value()?;
                let src_bytes = match src_val {
                    Value::String(s) => s,
                    _ => return Err(PsError::TypeCheck { expected: "string", got: src_val.type_name().to_string() }),
                };
                let copy_len = src_bytes.len().min(dest_str.len());
                dest_str[..copy_len].copy_from_slice(&src_bytes[..copy_len]);
                self.operand_stack.push(Value::String(dest_str));
            }
            _ => return Err(PsError::TypeCheck { expected: "integer, array, dict, or string", got: top.type_name().to_string() }),
        }
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

    fn get_current_point_user(&self) -> Option<(f64, f64)> {
        let (dx, dy) = self.current_gstate.current_point?;
        let inv = self.current_gstate.ctm.inverse().unwrap_or(Matrix2D::identity());
        Some(inv.transform_point(dx, dy))
    }

    fn set_current_point_user(&mut self, ux: f64, uy: f64) {
        let (dx, dy) = self.current_gstate.ctm.transform_point(ux, uy);
        self.current_gstate.current_point = Some((dx, dy));
    }

    fn read_image_source(
        &mut self,
        source: Value,
        total_needed: usize,
        lexer: &mut Lexer,
    ) -> PsResult<Vec<u8>> {
        match source {
            Value::String(bytes) => Ok(bytes),
            proc => {
                let mut data = Vec::with_capacity(total_needed);
                while data.len() < total_needed {
                    self.execute_proc_value(proc.clone(), lexer)?;
                    let Value::String(bytes) = self.pop_value()? else {
                        break;
                    };
                    if bytes.is_empty() {
                        break;
                    }
                    data.extend(bytes);
                }
                Ok(data)
            }
        }
    }

    fn push_image_mask(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
        polarity: bool,
        image_matrix: Matrix2D,
    ) {
        let row_bytes = width.div_ceil(8);
        let mut rgba = vec![0; width * height * 4];
        let color = self.current_gstate.color;

        for y in 0..height {
            for x in 0..width {
                let byte = data.get(y * row_bytes + x / 8).copied().unwrap_or(0);
                let bit_set = (byte & (0x80 >> (x % 8))) != 0;
                if bit_set == polarity {
                    let pixel = (y * width + x) * 4;
                    rgba[pixel] = (color.r * 255.0).clamp(0.0, 255.0) as u8;
                    rgba[pixel + 1] = (color.g * 255.0).clamp(0.0, 255.0) as u8;
                    rgba[pixel + 2] = (color.b * 255.0).clamp(0.0, 255.0) as u8;
                    rgba[pixel + 3] = (color.a * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
        }

        let transform = image_matrix
            .inverse()
            .unwrap_or_else(|| Matrix2D::scale(1.0 / width as f64, 1.0 / height as f64))
            .concat(&self.current_gstate.ctm);
        self.render_target.push_image(
            width as u32,
            height as u32,
            rgba,
            transform,
            self.current_gstate.clip_paths.clone(),
        );
    }

    fn render_type3_glyph(
        &mut self,
        font: &FontFace,
        glyph_name: &str,
        current_x: f64,
        current_y: f64,
    ) -> f64 {
        let Some(font_dict) = &font.type3_dict else {
            return 0.0;
        };
        let record = {
            let dict = font_dict.borrow();
            let Some(Value::Dict(char_data)) = dict.get("CD") else {
                return 0.0;
            };
            char_data.borrow().get(glyph_name).cloned()
        };
        let Some(Value::Array(record)) = record else {
            return 0.0;
        };
        let record = record.borrow();
        let advance = record
            .first()
            .and_then(|value| value.as_f64().ok())
            .unwrap_or(0.0);

        if record.len() >= 10 {
            let width = record[5].as_i64().unwrap_or(0).max(0) as usize;
            let height = record[6].as_i64().unwrap_or(0).max(0) as usize;
            let tx = record[7].as_f64().unwrap_or(0.0);
            let ty = record[8].as_f64().unwrap_or(0.0);
            if let Value::String(data) = &record[9] {
                let saved_ctm = self.current_gstate.ctm;
                self.current_gstate.ctm = Matrix2D::new(1.0, 0.0, 0.0, -1.0, tx, ty)
                    .inverse()
                    .unwrap_or_default()
                    .concat(&font.matrix)
                    .concat(&Matrix2D::translate(current_x, current_y))
                    .concat(&saved_ctm);
                self.push_image_mask(width, height, data, true, Matrix2D::identity());
                self.current_gstate.ctm = saved_ctm;
            }
        }

        font.matrix.transform_vector(advance, 0.0).0
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

fn read_image_data(lexer: &mut Lexer<'_>, data_source: Option<&Value>) -> Vec<u8> {
    let mut filters = Vec::new();
    let mut source = data_source.cloned();

    while let Some(Value::Dict(dict)) = source {
        let (filter, next_source) = {
            let dict = dict.borrow();
            (
                dict.get("Filter").map(Value::as_str_lossy),
                dict.get("Source").cloned(),
            )
        };
        if let Some(filter) = filter {
            filters.push(filter);
        }
        source = next_source;
    }

    if filters.iter().any(|filter| filter == "ASCII85Decode") {
        let encoded = read_until_ascii85_end(lexer);
        let ascii85 = decode_ascii85(&encoded);
        if filters.iter().any(|filter| filter == "RunLengthDecode") {
            return decode_run_length(&ascii85);
        }
        return ascii85;
    }

    read_hex_image_data(lexer)
}

fn read_until_ascii85_end(lexer: &mut Lexer<'_>) -> Vec<u8> {
    let remaining = lexer.remaining_bytes();
    let end = remaining
        .windows(2)
        .position(|window| window == b"~>")
        .unwrap_or(remaining.len());
    let data = remaining[..end].to_vec();
    lexer.set_position(lexer.position() + end + if end < remaining.len() { 2 } else { 0 });
    data
}

fn decode_ascii85(input: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    let mut tuple: u64 = 0;
    let mut count = 0usize;

    for &byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'z' && count == 0 {
            decoded.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&byte) {
            continue;
        }

        tuple = tuple * 85 + u64::from(byte - b'!');
        count += 1;
        if count == 5 {
            decoded.push((tuple >> 24) as u8);
            decoded.push((tuple >> 16) as u8);
            decoded.push((tuple >> 8) as u8);
            decoded.push(tuple as u8);
            tuple = 0;
            count = 0;
        }
    }

    if count > 0 {
        for _ in count..5 {
            tuple = tuple * 85 + 84;
        }
        for shift in (1..count).rev() {
            decoded.push((tuple >> (shift * 8)) as u8);
        }
    }

    decoded
}

fn decode_run_length(input: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    let mut offset = 0usize;

    while offset < input.len() {
        let length = input[offset];
        offset += 1;

        match length {
            0..=127 => {
                let count = usize::from(length) + 1;
                let end = (offset + count).min(input.len());
                decoded.extend_from_slice(&input[offset..end]);
                offset = end;
            }
            128 => break,
            129..=255 => {
                if offset >= input.len() {
                    break;
                }
                let value = input[offset];
                offset += 1;
                decoded.extend(std::iter::repeat_n(value, 257 - usize::from(length)));
            }
        }
    }

    decoded
}

fn read_hex_image_data(lexer: &mut Lexer<'_>) -> Vec<u8> {
    let remaining = lexer.remaining_bytes();
    let mut hex_end = 0;
    while hex_end < remaining.len() && remaining[hex_end] != b'>' && remaining[hex_end] != b'~' {
        if remaining[hex_end] == b'%'
            || remaining[hex_end] == b'\n'
            || remaining[hex_end] == b'\r'
            || remaining[hex_end].is_ascii_hexdigit()
            || remaining[hex_end].is_ascii_whitespace()
        {
            hex_end += 1;
        } else {
            break;
        }
    }

    let hex_slice = &remaining[..hex_end];
    lexer.set_position(
        lexer.position()
            + hex_end
            + if hex_end < remaining.len() && remaining[hex_end] == b'>' {
                1
            } else {
                0
            },
    );

    let mut decoded = Vec::new();
    let mut high_nibble = None;
    for &byte in hex_slice {
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => continue,
        };
        if let Some(high) = high_nibble.take() {
            decoded.push((high << 4) | nibble);
        } else {
            high_nibble = Some(nibble);
        }
    }
    if let Some(high) = high_nibble {
        decoded.push(high << 4);
    }

    decoded
}

fn format_radix(val: i64, radix: u32) -> String {
    if radix < 2 || radix > 36 {
        return val.to_string();
    }
    if val == 0 {
        return "0".to_string();
    }
    let is_neg = val < 0;
    let mut u = val.unsigned_abs();
    let mut chars = Vec::new();
    while u > 0 {
        let digit = (u % (radix as u64)) as u8;
        let c = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + (digit - 10)
        };
        chars.push(c);
        u /= radix as u64;
    }
    if is_neg {
        chars.push(b'-');
    }
    chars.reverse();
    String::from_utf8_lossy(&chars).to_string()
}
