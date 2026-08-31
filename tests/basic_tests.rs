use postscript::{
    execute_page_with_embedded_recovery, Color, DrawCommand, DscDocument, Interpreter, Matrix2D,
    PathSegment, Value,
};

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
fn test_count_to_mark() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("[ 1 2 3 counttomark").unwrap();
    assert_eq!(interp.operand_stack.last(), Some(&Value::Integer(3)));
}

#[test]
fn test_roll_and_string_put() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("1 2 3 3 -1 roll").unwrap();
    assert_eq!(
        interp.operand_stack,
        vec![Value::Integer(2), Value::Integer(3), Value::Integer(1)]
    );

    interp.execute_str("4 string dup 0 71 put").unwrap();
    assert_eq!(
        interp.operand_stack.last(),
        Some(&Value::String(vec![71, 0, 0, 0]))
    );
}

#[test]
fn test_executable_array_put_updates_loaded_procedure() {
    let mut interp = Interpreter::new(100, 100);
    interp
        .execute_str("/proc { 1 } def /proc load 0 42 put proc")
        .unwrap();

    assert_eq!(interp.operand_stack, vec![Value::Integer(42)]);
}

#[test]
fn test_type3_font_show_renders_bitmap_glyphs() {
    let mut interp = Interpreter::new(100, 100);
    interp
        .execute_str(
            "
            /BitmapFont 10 dict dup begin
            /FontType 3 def
            /FontMatrix [1 0 0 1 0 0] def
            /Encoding 256 array def
            0 1 255 { Encoding exch /.notdef put } for
            Encoding 65 /A put
            /CD 1 dict def
            CD /A [10 0 8 8 0 8 8 0 0 <8000000000000000>] put
            end
            /BitmapFont exch definefont pop
            /BitmapFont findfont 1 scalefont setfont
            10 10 moveto
            (A) show
            ",
        )
        .unwrap();

    assert!(interp
        .render_target
        .commands
        .iter()
        .any(|command| matches!(command, DrawCommand::Image { width: 8, height: 8, .. })));
}

#[test]
fn test_stopped_catches_errors_and_stop() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("7 { 2 3 add } stopped").unwrap();
    assert_eq!(interp.operand_stack, vec![
        Value::Integer(7),
        Value::Real(5.0),
        Value::Bool(false),
    ]);

    interp.execute_str("{ 99 undefined_operator } stopped").unwrap();
    assert_eq!(interp.operand_stack, vec![
        Value::Integer(7),
        Value::Real(5.0),
        Value::Bool(false),
        Value::Bool(true),
    ]);

    interp.execute_str("{ stop } stopped").unwrap();
    assert_eq!(interp.operand_stack, vec![
        Value::Integer(7),
        Value::Real(5.0),
        Value::Bool(false),
        Value::Bool(true),
        Value::Bool(true),
    ]);
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
fn test_matrix_transform_operators_return_transformed_matrix() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("3 4 matrix translate").unwrap();
    let Value::Array(matrix) = interp.operand_stack.pop().unwrap() else {
        panic!("expected a matrix");
    };
    let matrix = matrix.borrow();
    assert_eq!(matrix[4], Value::Real(3.0));
    assert_eq!(matrix[5], Value::Real(4.0));

    interp.execute_str("2 3 matrix scale").unwrap();
    let Value::Array(matrix) = interp.operand_stack.pop().unwrap() else {
        panic!("expected a matrix");
    };
    let matrix = matrix.borrow();
    assert_eq!(matrix[0], Value::Real(2.0));
    assert_eq!(matrix[3], Value::Real(3.0));

    interp.execute_str("90 matrix rotate").unwrap();
    let Value::Array(matrix) = interp.operand_stack.pop().unwrap() else {
        panic!("expected a matrix");
    };
    let matrix = matrix.borrow();
    assert!(matrix[0].as_f64().unwrap().abs() < 1e-12);
    assert_eq!(matrix[1], Value::Real(1.0));
    assert_eq!(matrix[2], Value::Real(-1.0));
    assert!(matrix[3].as_f64().unwrap().abs() < 1e-12);
}

#[test]
fn test_pattern_stub_preserves_pattern_and_rectfill_paints() {
    let mut interp = Interpreter::new(100, 100);
    interp
        .execute_str(
            "<< /PatternType 1 /BGnd [0.2 0.3 0.4 false] >> matrix makepattern \
             setpattern 10 10 20 20 rectfill",
        )
        .unwrap();

    let Some(DrawCommand::Fill { color, .. }) = interp.render_target.commands.last() else {
        panic!("expected a fill command");
    };
    assert_eq!(*color, Color::rgb(0.2, 0.3, 0.4));
}

#[test]
fn test_path_coordinates_are_fixed_when_ctm_changes_before_painting() {
    let mut interp = Interpreter::new(200, 200);
    interp
        .execute_str(
            "
            newpath
            10 10 moveto
            100 10 lineto
            [1 0 0 1 25 30] concat
            stroke
        ",
        )
        .unwrap();

    match &interp.render_target.commands[0] {
        DrawCommand::Stroke { path, .. } => {
            assert_eq!(
                path.segments,
                vec![
                    PathSegment::MoveTo(10.0, 190.0),
                    PathSegment::LineTo(100.0, 190.0),
                ]
            );
        }
        _ => panic!("expected a stroke command"),
    }
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
fn test_clipping_path_is_applied_to_draw_commands() {
    let mut interp = Interpreter::new(100, 100);
    interp.execute_str("
        newpath
        10 10 moveto
        50 10 lineto
        50 50 lineto
        10 50 lineto
        closepath
        clip
        newpath
        0 0 moveto
        100 0 lineto
        100 100 lineto
        0 100 lineto
        closepath
        fill
    ").unwrap();

    assert_eq!(interp.current_gstate.clip_paths.len(), 1);
    match &interp.render_target.commands[0] {
        DrawCommand::Fill { clip_paths, .. } => assert_eq!(clip_paths.len(), 1),
        _ => panic!("expected a fill command"),
    }
}

#[test]
fn test_dsc_page_size_prefers_declared_media_size() {
    let dsc = DscDocument::parse(b"%!PS-Adobe-3.0\n%%BoundingBox: 12 15 599 782\n%%BeginSetup\n2 dict dup /PageSize [612 792] put setpagedevice\n%%EndSetup\n%%Page: 1 1\nshowpage\n%%EOF\n");
    assert_eq!(dsc.page_size(), (612.0, 792.0));
}

#[test]
fn test_page_execution_ignores_post_showpage_cleanup() {
    let mut interp = Interpreter::new(100, 100);
    execute_page_with_embedded_recovery(
        &mut interp,
        b"showpage\nAdobe_WinNT_Driver_Gfx dup /terminate get exec\n",
    );
    assert_eq!(interp.pages_rendered.len(), 1);
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
