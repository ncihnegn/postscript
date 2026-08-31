use postscript::{execute_page_with_embedded_recovery, DscDocument, Interpreter, Color};
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: psview <input.ps> [-o output.png] [--page <n>] [--width <px>] [--height <px>]");
        process::exit(1);
    }

    let input_path = &args[1];
    let mut output_path = "output.png".to_string();
    let mut page_num = 1;
    let mut width = 612 * 2;
    let mut height = 792 * 2;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 < args.len() {
                    output_path = args[i + 1].clone();
                    i += 2;
                    continue;
                }
            }
            "--page" => {
                if i + 1 < args.len() {
                    if let Ok(p) = args[i + 1].parse::<usize>() {
                        page_num = p;
                    }
                    i += 2;
                    continue;
                }
            }
            "--width" => {
                if i + 1 < args.len() {
                    if let Ok(w) = args[i + 1].parse::<u32>() {
                        width = w;
                    }
                    i += 2;
                    continue;
                }
            }
            "--height" => {
                if i + 1 < args.len() {
                    if let Ok(h) = args[i + 1].parse::<u32>() {
                        height = h;
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    println!("Loading PostScript file: {}", input_path);
    let bytes = match fs::read(input_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading file {}: {}", input_path, e);
            process::exit(1);
        }
    };

    let dsc = DscDocument::parse(&bytes);
    println!("DSC Info:");
    println!("  Title: {:?}", dsc.title);
    println!("  Creator: {:?}", dsc.creator);
    let (page_w, page_h) = dsc.page_size();

    let mut interp = Interpreter::with_page_size(page_w, page_h, width, height);
    let page_index = page_num.saturating_sub(1);

    if !dsc.pages.is_empty() && page_index < dsc.pages.len() {
        println!("Rendering DSC page {} (label: {})...", page_num, dsc.pages[page_index].label);
        if let Some((start, end)) = dsc.preamble_range {
            if let Err(e) = interp.execute_bytes(&bytes[start..end]) {
                eprintln!("Warning during preamble execution: {}", e);
            }
        }
        let page = &dsc.pages[page_index];
        execute_page_with_embedded_recovery(
            &mut interp,
            &bytes[page.start_byte_offset..page.end_byte_offset],
        );
    } else {
        println!("Executing full PostScript stream...");
        if let Err(e) = interp.execute_bytes(&bytes) {
            eprintln!("Warning during execution: {}", e);
        }
    }

    let target = interp.pages_rendered.last().unwrap_or(&interp.render_target);
    println!("Vector draw commands recorded: {}", target.commands.len());

    match target.render_to_pixmap(Color::WHITE) {
        Ok(pixmap) => {
            if let Err(e) = pixmap.save_png(&output_path) {
                eprintln!("Error saving PNG {}: {}", output_path, e);
                process::exit(1);
            }
            println!("Successfully rendered to {}", output_path);
        }
        Err(e) => {
            eprintln!("Error rasterizing page: {}", e);
            process::exit(1);
        }
    }
}
