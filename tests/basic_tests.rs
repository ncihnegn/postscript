use postscript::{Color, Interpreter, Matrix2D, Value};

#[test]
fn test_stack_and_arithmetic() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("3 4 add 2 mul 4 sub").unwrap();
    assert_eq!(interp.operand_stack.pop().unwrap(), Value::Real(10.0));
}

#[test]
fn test_control_flow() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("true { 42 } { 99 } ifelse").unwrap();
    assert_eq!(interp.operand_stack.pop().unwrap(), Value::Integer(42));

    interp.execute_str("0 1 1 5 { add } for").unwrap();
    assert_eq!(interp.operand_stack.pop().unwrap(), Value::Real(15.0));
}

#[test]
fn test_dictionary_scoping() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("/foo 123 def foo").unwrap();
    assert_eq!(interp.operand_stack.pop().unwrap(), Value::Integer(123));

    interp.execute_str("<< /a 10 /b 20 >> begin a b add end").unwrap();
    assert_eq!(interp.operand_stack.pop().unwrap(), Value::Real(30.0));
}

#[test]
fn test_matrix_transforms() {
    let m = Matrix2D::translate(10.0, 20.0).concat(&Matrix2D::scale(2.0, 3.0));
    let (x, y) = m.transform_point(5.0, 5.0);
    assert_eq!(x, 30.0);
    assert_eq!(y, 75.0);
}

#[test]
fn test_graphics_drawing() {
    let mut interp = Interpreter::new(200, 200);
    interp.execute_str("
        newpath
        10 10 moveto
        100 10 lineto
        100 100 lineto
        closepath
        0.5 setgray
        fill
    ").unwrap();

    assert_eq!(interp.render_target.commands.len(), 1);
    let pixmap = interp.render_target.render_to_pixmap(Color::WHITE).unwrap();
    assert_eq!(pixmap.width(), 200);
    assert_eq!(pixmap.height(), 200);
}

#[test]
fn test_sh_ps_font_loading() {
    let bytes = std::fs::read("/Users/ningchen/Temp/arXiv/math/9201303/sh.ps").unwrap();
    let pixmap = postscript::render_ps_to_pixmap(&bytes, 0, 612 * 2, 792 * 2).unwrap();
    assert_eq!(pixmap.width(), 1224);
    assert_eq!(pixmap.height(), 1584);
}
