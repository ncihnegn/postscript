use crate::error::{PsError, PsResult};
use crate::gstate::{ClipPath, Color, LineCap, LineJoin};
use crate::path::{Path, PathSegment};
use tiny_skia::{
    Color as SkColor, FillRule, LineCap as SkLineCap, LineJoin as SkLineJoin,
    Mask, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Fill {
        path: Path,
        color: Color,
        even_odd: bool,
        clip_paths: Vec<ClipPath>,
    },
    Stroke {
        path: Path,
        color: Color,
        width: f64,
        cap: LineCap,
        join: LineJoin,
        miter_limit: f64,
        clip_paths: Vec<ClipPath>,
    },
    Image {
        width: u32,
        height: u32,
        rgba_data: Vec<u8>,
        transform: crate::matrix::Matrix2D,
        clip_paths: Vec<ClipPath>,
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

    pub fn push_fill(&mut self, path: Path, color: Color, even_odd: bool, clip_paths: Vec<ClipPath>) {
        if !path.is_empty() {
            self.commands.push(DrawCommand::Fill {
                path,
                color,
                even_odd,
                clip_paths,
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
        clip_paths: Vec<ClipPath>,
    ) {
        if !path.is_empty() {
            self.commands.push(DrawCommand::Stroke {
                path,
                color,
                width,
                cap,
                join,
                miter_limit,
                clip_paths,
            });
        }
    }

    pub fn push_image(
        &mut self,
        width: u32,
        height: u32,
        rgba_data: Vec<u8>,
        transform: crate::matrix::Matrix2D,
        clip_paths: Vec<ClipPath>,
    ) {
        if width > 0 && height > 0 && !rgba_data.is_empty() {
            self.commands.push(DrawCommand::Image {
                width,
                height,
                rgba_data,
                transform,
                clip_paths,
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
                DrawCommand::Image { width, height, rgba_data, transform, clip_paths } => {
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
                            let mask = self.clip_mask(clip_paths);
                            pixmap.draw_pixmap(
                                0, 0,
                                img_pixmap,
                                &tiny_skia::PixmapPaint::default(),
                                sk_transform,
                                mask.as_ref(),
                            );
                        }
                    }
                }
                DrawCommand::Fill { path, color, even_odd, clip_paths } => {
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

                        let mask = self.clip_mask(clip_paths);
                        pixmap.fill_path(
                            &sk_path,
                            &paint,
                            fill_rule,
                            Transform::identity(),
                            mask.as_ref(),
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
                    clip_paths,
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

                        let mask = self.clip_mask(clip_paths);
                        pixmap.stroke_path(
                            &sk_path,
                            &paint,
                            &stroke,
                            Transform::identity(),
                            mask.as_ref(),
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

    fn clip_mask(&self, clip_paths: &[ClipPath]) -> Option<Mask> {
        if clip_paths.is_empty() {
            return None;
        }
        let mut mask = Mask::new(self.width, self.height)?;

        for (index, clip) in clip_paths.iter().enumerate() {
            let fill_rule = if clip.even_odd {
                FillRule::EvenOdd
            } else {
                FillRule::Winding
            };

            if index == 0 {
                mask.fill_path(
                    &self.build_skia_path(&clip.path)?,
                    fill_rule,
                    true,
                    Transform::identity(),
                );
            } else {
                mask.intersect_path(
                    &self.build_skia_path(&clip.path)?,
                    fill_rule,
                    true,
                    Transform::identity(),
                );
            }
        }

        Some(mask)
    }
}
