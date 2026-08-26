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
fn test_dancing_ps_page3() {
    let bytes = std::fs::read("/Users/ningchen/Temp/arXiv/cs/0011047/dancing.ps").unwrap();
    let dsc = postscript::DscDocument::parse(&bytes);
    println!("Dsc total pages: {:?}, parsed pages count: {}", dsc.total_pages, dsc.pages.len());
    let page3 = &dsc.pages[2];
    println!("Page 3 label: {}, range: {}..{}", page3.label, page3.start_byte_offset, page3.end_byte_offset);
    let mut interp = postscript::Interpreter::with_page_size(612.0, 792.0, 612 * 2, 792 * 2);
    if let Some((start, end)) = dsc.preamble_range {
        interp.execute_bytes(&bytes[start..end]).unwrap();
    }
    let page_bytes = &bytes[page3.start_byte_offset..page3.end_byte_offset];
    let res = interp.execute_bytes(page_bytes);
    println!("Page 3 execute result: {:?}", res);
    println!("Page 3 draw commands count: {}", interp.render_target.commands.len());
    for (i, cmd) in interp.render_target.commands.iter().take(20).enumerate() {
        println!("  cmd {}: {:?}", i, cmd);
    }
}
