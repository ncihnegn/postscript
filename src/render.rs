use crate::error::{PsError, PsResult};
use crate::gstate::{Color, LineCap, LineJoin};
use crate::path::{Path, PathSegment};
use tiny_skia::{
    Color as SkColor, FillRule, LineCap as SkLineCap, LineJoin as SkLineJoin,
    Paint, PathBuilder, Pixmap, Stroke, Transform,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Fill {
        path: Path,
        color: Color,
        even_odd: bool,
    },
    Stroke {
        path: Path,
        color: Color,
        width: f64,
        cap: LineCap,
        join: LineJoin,
        miter_limit: f64,
    },
    Image {
        width: u32,
        height: u32,
        rgba_data: Vec<u8>,
        transform: crate::matrix::Matrix2D,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RenderTarget {
    pub width: u32,
    pub height: u32,
    pub commands: Vec<DrawCommand>,
}

impl RenderTarget {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
        }
    }

    pub fn push_fill(&mut self, path: Path, color: Color, even_odd: bool) {
        if !path.is_empty() {
            self.commands.push(DrawCommand::Fill {
                path,
                color,
                even_odd,
            });
        }
    }

    pub fn push_stroke(
        &mut self,
        path: Path,
        color: Color,
        width: f64,
        cap: LineCap,
        join: LineJoin,
        miter_limit: f64,
    ) {
        if !path.is_empty() {
            self.commands.push(DrawCommand::Stroke {
                path,
                color,
                width,
                cap,
                join,
                miter_limit,
            });
        }
    }

    pub fn push_image(&mut self, width: u32, height: u32, rgba_data: Vec<u8>, transform: crate::matrix::Matrix2D) {
        if width > 0 && height > 0 && !rgba_data.is_empty() {
            self.commands.push(DrawCommand::Image {
                width,
                height,
                rgba_data,
                transform,
            });
        }
    }

    pub fn render_to_pixmap(&self, background: Color) -> PsResult<Pixmap> {
        let mut pixmap = Pixmap::new(self.width, self.height)
            .ok_or_else(|| PsError::LimitCheck("could not allocate pixmap".to_string()))?;

        pixmap.fill(SkColor::from_rgba(
            background.r as f32,
            background.g as f32,
            background.b as f32,
            background.a as f32,
        ).unwrap_or(SkColor::WHITE));

        for cmd in &self.commands {
            match cmd {
                DrawCommand::Image { width, height, rgba_data, transform } => {
                    if let Some(size) = tiny_skia::IntSize::from_wh(*width, *height) {
                        if let Some(img_pixmap) = tiny_skia::PixmapRef::from_bytes(rgba_data, size.width(), size.height()) {
                            let sk_transform = Transform::from_row(
                                transform.a as f32,
                                transform.b as f32,
                                transform.c as f32,
                                transform.d as f32,
                                transform.tx as f32,
                                transform.ty as f32,
                            );
                            pixmap.draw_pixmap(
                                0, 0,
                                img_pixmap,
                                &tiny_skia::PixmapPaint::default(),
                                sk_transform,
                                None,
                            );
                        }
                    }
                }
                DrawCommand::Fill { path, color, even_odd } => {
                    if let Some(sk_path) = self.build_skia_path(path) {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(
                            (color.r * 255.0).clamp(0.0, 255.0) as u8,
                            (color.g * 255.0).clamp(0.0, 255.0) as u8,
                            (color.b * 255.0).clamp(0.0, 255.0) as u8,
                            (color.a * 255.0).clamp(0.0, 255.0) as u8,
                        );
                        paint.anti_alias = true;

                        let fill_rule = if *even_odd {
                            FillRule::EvenOdd
                        } else {
                            FillRule::Winding
                        };

                        pixmap.fill_path(
                            &sk_path,
                            &paint,
                            fill_rule,
                            Transform::identity(),
                            None,
                        );
                    }
                }
                DrawCommand::Stroke {
                    path,
                    color,
                    width,
                    cap,
                    join,
                    miter_limit,
                } => {
                    if let Some(sk_path) = self.build_skia_path(path) {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(
                            (color.r * 255.0).clamp(0.0, 255.0) as u8,
                            (color.g * 255.0).clamp(0.0, 255.0) as u8,
                            (color.b * 255.0).clamp(0.0, 255.0) as u8,
                            (color.a * 255.0).clamp(0.0, 255.0) as u8,
                        );
                        paint.anti_alias = true;

                        let sk_cap = match cap {
                            LineCap::Butt => SkLineCap::Butt,
                            LineCap::Round => SkLineCap::Round,
                            LineCap::Square => SkLineCap::Square,
                        };

                        let sk_join = match join {
                            LineJoin::Miter => SkLineJoin::Miter,
                            LineJoin::Round => SkLineJoin::Round,
                            LineJoin::Bevel => SkLineJoin::Bevel,
                        };

                        let stroke = Stroke {
                            width: (*width as f32).max(0.5),
                            line_cap: sk_cap,
                            line_join: sk_join,
                            miter_limit: *miter_limit as f32,
                            ..Default::default()
                        };

                        pixmap.stroke_path(
                            &sk_path,
                            &paint,
                            &stroke,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        Ok(pixmap)
    }

    fn build_skia_path(&self, path: &Path) -> Option<tiny_skia::Path> {
        let mut builder = PathBuilder::new();
        let mut in_figure = false;

        for seg in &path.segments {
            match *seg {
                PathSegment::MoveTo(x, y) => {
                    builder.move_to(x as f32, y as f32);
                    in_figure = true;
                }
                PathSegment::LineTo(x, y) => {
                    if !in_figure {
                        builder.move_to(x as f32, y as f32);
                        in_figure = true;
                    } else {
                        builder.line_to(x as f32, y as f32);
                    }
                }
                PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => {
                    if !in_figure {
                        builder.move_to(x1 as f32, y1 as f32);
                        in_figure = true;
                    }
                    builder.cubic_to(
                        x1 as f32, y1 as f32,
                        x2 as f32, y2 as f32,
                        x3 as f32, y3 as f32,
                    );
                }
                PathSegment::ClosePath => {
                    if in_figure {
                        builder.close();
                        in_figure = false;
                    }
                }
            }
        }

        builder.finish()
    }
}
