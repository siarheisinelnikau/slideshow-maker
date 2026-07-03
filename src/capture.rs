//! Bounded client-area capture through the production egui UI and CPU rasterizer.
use crate::ui::Editor;
use egui_software_backend::BufferMutRef;
use slideshow_maker::storage::load_project;
use std::{
    error::Error,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct Options {
    output: PathBuf,
    screen: String,
    project: Option<PathBuf>,
    width: u32,
    height: u32,
    scale: f32,
}
impl Options {
    pub fn parse(arguments: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            output: PathBuf::new(),
            screen: "editor".into(),
            project: None,
            width: 1180,
            height: 780,
            scale: 1.0,
        };
        let mut args = arguments.iter();
        while let Some(flag) = args.next() {
            let value = args.next().ok_or("Every capture flag requires a value")?;
            match flag.as_str() {
                "--screenshot" => options.output = value.into(),
                "--screen" => options.screen = value.clone(),
                "--project" => options.project = Some(value.into()),
                "--size" => {
                    let (w, h) = value
                        .split_once('x')
                        .ok_or("Size must be WIDTHxHEIGHT in logical pixels")?;
                    options.width = w.parse()?;
                    options.height = h.parse()?;
                }
                "--scale" => options.scale = value.parse()?,
                _ => return Err(format!("Unknown capture flag: {flag}").into()),
            }
        }
        if !options.output.is_absolute()
            || !["editor", "settings", "export", "music"].contains(&options.screen.as_str())
            || !(900..=1920).contains(&options.width)
            || !(640..=1080).contains(&options.height)
            || !options.scale.is_finite()
            || !(1.0..=2.0).contains(&options.scale)
        {
            return Err("Capture requires an absolute output, editor/settings/export/music screen, size 900–1920 x 640–1080, and scale 1–2".into());
        }
        Ok(options)
    }
}
pub fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let started = Instant::now();
    let project = options
        .project
        .as_deref()
        .map(load_project)
        .transpose()?
        .unwrap_or_default();
    let ctx = egui::Context::default();
    // Native scale is supplied in RawInput. Setting pixels-per-point as well
    // would apply the same scale twice through egui's zoom factor.
    ctx.set_zoom_factor(1.0);
    let mut editor = Editor::for_capture(ctx.clone(), project, &options.screen)?;
    let width = (options.width as f32 * options.scale).round() as u32;
    let height = (options.height as f32 * options.scale).round() as u32;
    let mut pixels = vec![0u32; width as usize * height as usize];
    let mut renderer = crate::rasterizer::create();
    let mut rendered = 0;
    let mut settled = false;
    for frame in 0..8 {
        if started.elapsed() > Duration::from_secs(8) {
            return Err("Capture exceeded its internal deadline".into());
        }
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(options.width as f32, options.height as f32),
            )),
            time: Some(frame as f64 * 0.25),
            predicted_dt: 0.25,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(options.scale);
        let output = ctx.run(input, |ctx| editor.update(ctx));
        if (output.pixels_per_point - options.scale).abs() > 0.01 {
            return Err("Capture scale does not match the rasterized viewport".into());
        }
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .unwrap_or(Duration::MAX);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let mut buffer = BufferMutRef::new(
            bytemuck::cast_slice_mut(&mut pixels),
            width as usize,
            height as usize,
        );
        renderer.render(
            &mut buffer,
            &primitives,
            &output.textures_delta,
            output.pixels_per_point,
        );
        rendered += 1;
        if frame >= 2 && delay > Duration::from_millis(100) {
            settled = true;
            break;
        }
    }
    editor.shutdown();
    if !settled {
        return Err("Capture UI did not settle within eight frames".into());
    }
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        let pixel = pixels[y as usize * width as usize + x as usize];
        image::Rgb([(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])
    });
    let parent = options
        .output
        .parent()
        .ok_or("Capture output has no directory")?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    image.write_to(file.as_file_mut(), image::ImageFormat::Png)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(&options.output)?;
    let manifest = serde_json::json!({"screen":options.screen,"client_area_only":true,"logical_size":[options.width,options.height],"physical_size":[width,height],"scale":options.scale,"renderer":"production egui CPU rasterizer; no native window or GDI presentation in capture mode","frames":rendered,"elapsed_ms":started.elapsed().as_millis(),"network_requests":0,"settings_writes":0,"version":env!("CARGO_PKG_VERSION")});
    std::fs::write(
        options.output.with_extension("png.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}
