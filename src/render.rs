use crate::model::*;
use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{DynamicImage, ImageDecoder, ImageReader, Rgb, RgbImage, imageops};
use std::path::PathBuf;

#[derive(Default)]
pub struct Renderer {
    cache: Vec<(PathBuf, RgbImage)>,
}
impl Renderer {
    fn photo(&mut self, path: &PathBuf) -> Result<&RgbImage> {
        if !self.cache.iter().any(|(p, _)| p == path) {
            let mut reader =
                ImageReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(16000);
            limits.max_image_height = Some(16000);
            limits.max_alloc = Some(256 * 1024 * 1024);
            reader.limits(limits);
            let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
            let orientation = decoder.orientation().map_err(|e| e.to_string())?;
            let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
            img.apply_orientation(orientation);
            // Bound the two-item decoded cache as well as source decoder allocation.
            let img = img
                .resize(2048, 2048, imageops::FilterType::Triangle)
                .to_rgba8();
            let rgb = RgbImage::from_fn(img.width(), img.height(), |x, y| {
                let p = img.get_pixel(x, y);
                let a = u32::from(p[3]);
                Rgb([
                    ((u32::from(p[0]) * a) / 255) as u8,
                    ((u32::from(p[1]) * a) / 255) as u8,
                    ((u32::from(p[2]) * a) / 255) as u8,
                ])
            });
            if self.cache.len() == 2 {
                self.cache.remove(0);
            }
            self.cache.push((path.clone(), rgb));
        }
        Ok(&self.cache.iter().find(|(p, _)| p == path).unwrap().1)
    }
    pub fn frame(&mut self, p: &Project, seconds: f64, size: (u32, u32)) -> Result<RgbImage> {
        let (i, mix) = p
            .position(seconds)
            .ok_or("Add a photo or title card first")?;
        let mut a = self.slide(&p.slides[i], p.theme, size)?;
        if mix > 0.0 {
            let b = self.slide(&p.slides[i + 1], p.theme, size)?;
            let bg = p.theme.colors().0;
            for (a, b) in a.pixels_mut().zip(b.pixels()) {
                for c in 0..3 {
                    a[c] = if p.transition == Transition::FadeThroughBackground {
                        if mix < 0.5 {
                            lerp(a[c], bg[c], mix * 2.0)
                        } else {
                            lerp(bg[c], b[c], mix * 2.0 - 1.0)
                        }
                    } else {
                        lerp(a[c], b[c], mix)
                    };
                }
            }
        }
        Ok(a)
    }
    fn slide(&mut self, slide: &Slide, theme: Theme, (w, h): (u32, u32)) -> Result<RgbImage> {
        let (bg, fg, accent) = theme.colors();
        let mut out = RgbImage::from_pixel(w, h, Rgb(bg));
        match &slide.media {
            Media::Photo(path) => {
                let source = self.photo(path)?;
                let rotated;
                let source = match slide.rotation {
                    1 => {
                        rotated = imageops::rotate90(source);
                        &rotated
                    }
                    2 => {
                        rotated = imageops::rotate180(source);
                        &rotated
                    }
                    3 => {
                        rotated = imageops::rotate270(source);
                        &rotated
                    }
                    _ => source,
                };
                let sx = w as f64 / source.width() as f64;
                let sy = h as f64 / source.height() as f64;
                let scale = if slide.fit == Fit::Fit {
                    sx.min(sy)
                } else {
                    sx.max(sy)
                };
                let rw = (source.width() as f64 * scale).round().max(1.0) as u32;
                let rh = (source.height() as f64 * scale).round().max(1.0) as u32;
                // Avoid huge intermediates for extreme panoramic input in Fill mode.
                let cropped = if slide.fit == Fit::Fill {
                    let cw = (w as f64 / scale)
                        .round()
                        .max(1.0)
                        .min(source.width() as f64) as u32;
                    let ch = (h as f64 / scale)
                        .round()
                        .max(1.0)
                        .min(source.height() as f64) as u32;
                    imageops::crop_imm(
                        source,
                        (source.width() - cw) / 2,
                        (source.height() - ch) / 2,
                        cw,
                        ch,
                    )
                    .to_image()
                } else {
                    source.clone()
                };
                let (rw, rh) = if slide.fit == Fit::Fill {
                    (w, h)
                } else {
                    (rw, rh)
                };
                let resized = imageops::resize(&cropped, rw, rh, imageops::FilterType::Triangle);
                imageops::overlay(
                    &mut out,
                    &resized,
                    (i64::from(w) - i64::from(rw)) / 2,
                    (i64::from(h) - i64::from(rh)) / 2,
                );
                if !slide.caption.trim().is_empty() {
                    caption(&mut out, &slide.caption, [250, 250, 248], false)?;
                }
            }
            Media::Title => {
                let y = h / 4;
                for yy in y..y + (h / 100).max(2) {
                    for x in w / 3..w * 2 / 3 {
                        out.put_pixel(x, yy, Rgb(accent));
                    }
                }
                caption(&mut out, &slide.caption, fg, true)?;
            }
        }
        Ok(out)
    }
}
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 * (1.0 - t) + b as f32 * t).round() as u8
}

fn caption(img: &mut RgbImage, text: &str, color: [u8; 3], title: bool) -> Result<()> {
    let font =
        FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).map_err(|e| e.to_string())?;
    let (w, h) = img.dimensions();
    let max_width = w as f32 * 0.86;
    let mut scale = h.min(w) as f32 * if title { 0.075 } else { 0.048 };
    let mut lines;
    loop {
        let scaled = font.as_scaled(PxScale::from(scale));
        lines = wrap(text, &scaled, max_width);
        if lines.len() as f32 * scale * 1.3 <= h as f32 * if title { 0.48 } else { 0.30 }
            || scale <= 7.0
        {
            break;
        }
        scale *= 0.9;
    }
    let scaled = font.as_scaled(PxScale::from(scale));
    let line_h = scale * 1.3;
    let total = lines.len() as f32 * line_h;
    let top = if title {
        (h as f32 - total) / 2.0
    } else {
        h as f32 - total - h as f32 * 0.05
    };
    if !title {
        for y in ((top - h as f32 * 0.03).max(0.0) as u32)..h {
            for x in 0..w {
                let p = img.get_pixel_mut(x, y);
                for c in 0..3 {
                    p[c] = lerp(p[c], 0, 0.66);
                }
            }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let width: f32 = line
            .chars()
            .map(|c| scaled.h_advance(scaled.glyph_id(c)))
            .sum();
        let mut x = (w as f32 - width) / 2.0;
        let baseline = top + i as f32 * line_h + scaled.ascent();
        for c in line.chars() {
            let id = scaled.glyph_id(c);
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, a| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                        let p = img.get_pixel_mut(px as u32, py as u32);
                        for j in 0..3 {
                            p[j] = lerp(p[j], color[j], a);
                        }
                    }
                });
            }
            x += scaled.h_advance(id);
        }
    }
    Ok(())
}
fn wrap<F: Font>(text: &str, font: &impl ScaleFont<F>, max: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    let mut width = 0.0;
    for c in text.chars() {
        let cw = font.h_advance(font.glyph_id(c));
        if c == '\n' || width + cw > max {
            lines.push(std::mem::take(&mut line));
            width = 0.0;
            if c == '\n' {
                continue;
            }
        }
        line.push(c);
        width += cw;
    }
    lines.push(line);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_preserves_edges_fill_is_explicit_and_transition_blends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        RgbImage::from_pixel(200, 100, Rgb([255, 0, 0]))
            .save(&path)
            .unwrap();
        let mut p = Project {
            slides: vec![Slide::photo(path), Slide::title()],
            ..Project::default()
        };
        let mut r = Renderer::default();
        let frame = r.frame(&p, 0.0, (100, 100)).unwrap();
        assert_eq!(frame.get_pixel(50, 0).0, p.theme.colors().0);
        assert_eq!(frame.get_pixel(50, 50).0, [255, 0, 0]);
        p.slides[0].fit = Fit::Fill;
        assert_eq!(
            r.frame(&p, 0.0, (100, 100)).unwrap().get_pixel(0, 0).0,
            [255, 0, 0]
        );
        let mixed = r.frame(&p, 3.65, (100, 100)).unwrap();
        assert!(mixed.get_pixel(0, 0)[0] < 250);
    }
}
