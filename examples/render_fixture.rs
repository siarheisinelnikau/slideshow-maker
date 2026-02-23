//! Safe, headless fixture: no dialogs, network, playback or application window.
use image::{Rgb, RgbImage};
use slideshow_maker::{
    export::{Format, Progress, export},
    model::*,
    render::Renderer,
    storage::save_project,
};
use std::{io::Write, path::PathBuf};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-output");
    std::fs::create_dir_all(&root)?;
    let first = root.join("synthetic-wide.png");
    let second = root.join("synthetic-tall.png");
    RgbImage::from_fn(240, 120, |x, y| {
        Rgb([(x % 256) as u8, (y * 2 % 256) as u8, 80])
    })
    .save(&first)?;
    RgbImage::from_fn(90, 180, |x, y| {
        Rgb([60, (x * 2 % 256) as u8, (y % 256) as u8])
    })
    .save(&second)?;
    let wav = root.join("synthetic-music.wav");
    let mut file = std::fs::File::create(&wav)?;
    let samples: Vec<i16> = (0..16000)
        .map(|n| ((n as f32 * 440.0 * std::f32::consts::TAU / 8000.0).sin() * 3000.0) as i16)
        .collect();
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + samples.len() as u32 * 2).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&8000u32.to_le_bytes())?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&(samples.len() as u32 * 2).to_le_bytes())?;
    for s in samples {
        file.write_all(&s.to_le_bytes())?;
    }
    drop(file);
    let mut title = Slide::title();
    title.caption = "Synthetic family story".into();
    title.seconds = 0.5;
    let mut wide = Slide::photo(first);
    wide.seconds = 1.0;
    wide.caption = "A wide photo · Fit keeps every edge".into();
    let mut tall = Slide::photo(second);
    tall.seconds = 1.0;
    tall.caption = "A tall photo, safely letterboxed".into();
    let project = Project {
        slides: vec![title, wide, tall],
        audio: Some(wav),
        audio_loop: true,
        height: 360,
        fps: 12,
        ..Project::default()
    };
    let map_err = |e: String| std::io::Error::other(e);
    save_project(&root.join("synthetic.slideshow.json"), &project).map_err(map_err)?;
    let mut renderer = Renderer::default();
    renderer
        .frame(&project, 0.75, project.layout.size(360))
        .map_err(map_err)?
        .save(root.join("composited-frame.png"))?;
    let output = root.join("synthetic.avi");
    // Reruns preserve existing output: select a fresh filename instead of deleting it.
    let output = if output.exists() {
        root.join(format!(
            "synthetic-{}.avi",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
        ))
    } else {
        output
    };
    export(&project, &output, Format::Avi, None, &Progress::default()).map_err(map_err)?;
    println!("{}", output.display());
    Ok(())
}
