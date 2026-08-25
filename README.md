# `postscript`

A native, standalone PostScript interpreter, Document Structuring Conventions (DSC) parser, and 2D vector renderer written in pure Rust.

## Features

- **PostScript Virtual Machine**:
  - Operand, dictionary, and graphics state stacks (`userdict`, `systemdict`, `FontDirectory`).
  - Arithmetic and mathematical operations (`add`, `sub`, `mul`, `div`, `idiv`, `mod`, `sin`, `cos`, `atan`, `exp`, `ln`, `sqrt`).
  - Control flow: `if`, `ifelse`, `for`, `repeat`, `loop`, `forall`, `exit`, `exec`.
  - Dictionary and array operations (`def`, `load`, `store`, `dict`, `begin`, `end`, `known`, `get`, `put`, `aload`, `astore`).
- **2D Graphics & Quartz/CoreGraphics-style Vector Engine**:
  - 2D affine transformation matrices (`translate`, `scale`, `rotate`, `concat`, `matrix`, `currentmatrix`, `setmatrix`).
  - Path construction (`newpath`, `moveto`, `rmoveto`, `lineto`, `rlineto`, `curveto`, `rcurveto`, `arc`, `arcn`, `closepath`, `currentpoint`).
  - Painting & rasterization: `fill`, `eofill`, `stroke`, `showpage`.
  - Color models: RGB (`setrgbcolor`), Grayscale (`setgray`), CMYK (`setcmykcolor`).
  - Stroke properties: `setlinewidth`, `setlinecap`, `setlinejoin`, `setmiterlimit`.
- **Type 1 Font & Decryption Subsystem**:
  - Type 1 / `eexec` cipher stream decryption filter ($R = 55665$).
  - Type 1 CharStrings bytecodes decryption ($R = 4330$) and glyph outline interpreter.
  - Font operations: `findfont`, `scalefont`, `makefont`, `setfont`, `charpath`, `show`.
- **Document Structuring Conventions (DSC) Parser**:
  - Automatically indexes multi-page documents (`%%Page: <label> <ordinal>`).
  - Extracts metadata (`%%Title:`, `%%Creator:`, `%%BoundingBox:`, `%%HiResBoundingBox:`).
  - Isolates and executes prolog and document setup prior to per-page execution.
- **Renderer**:
  - Software rasterization using `tiny-skia` to render crisp vector graphics and text directly to RGBA / PNG images.

## CLI Usage

```bash
# Render page 1 of a PostScript document to PNG
cargo run --release --bin psview -- /path/to/document.ps -o page1.png --page 1

# Specify resolution / dimensions
cargo run --release --bin psview -- /path/to/document.ps -o page1.png --page 1 --width 1224 --height 1584
```

## Library API Usage

```rust
use postscript::{DscDocument, Interpreter, Color, render_ps_to_png};

// Direct high-level rendering
let ps_bytes = std::fs::read("document.ps").unwrap();
let png_bytes = render_ps_to_png(&ps_bytes, 0, 1224, 1584).unwrap();
std::fs::write("output.png", png_bytes).unwrap();

// Or granular Virtual Machine control
let mut interp = Interpreter::new(800, 600);
interp.execute_str("
    newpath
    100 100 moveto
    300 100 lineto
    200 250 lineto
    closepath
    1 0 0 setrgbcolor
    fill
").unwrap();

let pixmap = interp.render_target.render_to_pixmap(Color::WHITE).unwrap();
pixmap.save_png("triangle.png").unwrap();
```
