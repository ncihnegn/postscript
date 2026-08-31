use crate::error::{PsError, PsResult};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone)]
pub enum Value {
    Integer(i64),
    Real(f64),
    Bool(bool),
    String(Vec<u8>),
    Name(String),        // executable name: add, moveto, etc.
    LiteralName(String), // literal name: /Foo, /Times-Roman, etc.
    ImmediateName(String), // immediately resolved name: //Foo
    Array(Rc<RefCell<Vec<Value>>>),
    ExecutableArray(Rc<Vec<Value>>), // { ... } procedure block
    Dict(Rc<RefCell<HashMap<String, Value>>>),
    Mark,
    Null,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Real(a), Value::Real(b)) => a == b,
            (Value::Integer(a), Value::Real(b)) | (Value::Real(b), Value::Integer(a)) => {
                (*a as f64) == *b
            }
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Name(a), Value::Name(b))
            | (Value::LiteralName(a), Value::LiteralName(b))
            | (Value::ImmediateName(a), Value::ImmediateName(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::ExecutableArray(a), Value::ExecutableArray(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Mark, Value::Mark) | (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl Value {
    pub fn new_array(items: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(items)))
    }

    pub fn new_proc(items: Vec<Value>) -> Self {
        Value::ExecutableArray(Rc::new(items))
    }

    pub fn new_dict() -> Self {
        Value::Dict(Rc::new(RefCell::new(HashMap::new())))
    }

    pub fn new_string<S: Into<Vec<u8>>>(s: S) -> Self {
        Value::String(s.into())
    }

    pub fn as_i64(&self) -> PsResult<i64> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Real(r) => Ok(*r as i64),
            _ => Err(PsError::TypeCheck {
                expected: "integer",
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_f64(&self) -> PsResult<f64> {
        match self {
            Value::Real(r) => Ok(*r),
            Value::Integer(i) => Ok(*i as f64),
            _ => Err(PsError::TypeCheck {
                expected: "number",
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_bool(&self) -> PsResult<bool> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(PsError::TypeCheck {
                expected: "boolean",
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_str_lossy(&self) -> String {
        match self {
            Value::String(s) => String::from_utf8_lossy(s).to_string(),
            Value::Name(n) | Value::LiteralName(n) | Value::ImmediateName(n) => n.clone(),
            _ => format!("{}", self),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "integertype",
            Value::Real(_) => "realtype",
            Value::Bool(_) => "booleantype",
            Value::String(_) => "stringtype",
            Value::Name(_) | Value::LiteralName(_) | Value::ImmediateName(_) => "nametype",
            Value::Array(_) => "arraytype",
            Value::ExecutableArray(_) => "arraytype",
            Value::Dict(_) => "dicttype",
            Value::Mark => "marktype",
            Value::Null => "nulltype",
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Real(r) => write!(f, "{}", r),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "({})", String::from_utf8_lossy(s)),
            Value::Name(n) => write!(f, "{}", n),
            Value::LiteralName(n) => write!(f, "/{}", n),
            Value::ImmediateName(n) => write!(f, "//{}", n),
            Value::Array(a) => write!(f, "{:?}", a.borrow()),
            Value::ExecutableArray(a) => write!(f, "{{{:?}}}", a),
            Value::Dict(d) => write!(f, "<<dict: {} entries>>", d.borrow().len()),
            Value::Mark => write!(f, "-mark-"),
            Value::Null => write!(f, "null"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Real(r) => write!(f, "{}", r),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "{}", String::from_utf8_lossy(s)),
            Value::Name(n) => write!(f, "{}", n),
            Value::LiteralName(n) => write!(f, "/{}", n),
            Value::ImmediateName(n) => write!(f, "//{}", n),
            Value::Array(a) => write!(f, "[array: {}]", a.borrow().len()),
            Value::ExecutableArray(a) => write!(f, "{{proc: {} ops}}", a.len()),
            Value::Dict(d) => write!(f, "<dict:{}>", d.borrow().len()),
            Value::Mark => write!(f, "-mark-"),
            Value::Null => write!(f, "null"),
        }
    }
}
