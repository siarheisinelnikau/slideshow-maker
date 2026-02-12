use crate::{
    avi::{Audio, Avi},
    model::*,
    render::Renderer,
};
use image::codecs::jpeg::JpegEncoder;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Progress {
    pub cancelled: Arc<AtomicBool>,
    pub done: Arc<AtomicU32>,
    pub total: Arc<AtomicU32>,
    pub encoding: Arc<AtomicBool>,
}
impl Progress {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("Export cancelled; no destination file was created".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Avi,
    Mp4,
}
pub fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
pub fn export(
    project: &Project,
    destination: &Path,
    format: Format,
    ffmpeg: Option<&Path>,
    progress: &Progress,
) -> Result<()> {
    project.validate()?;
    if project.slides.is_empty() {
        return Err("Add photos or a title card before exporting".into());
    }
    if destination.exists() {
        return Err("Destination already exists. Choose a new filename; existing files are never overwritten".into());
    }
    if format == Format::Mp4 && ffmpeg.is_none() {
        return Err("Choose a compatible FFmpeg executable in Settings to export MP4".into());
    }
    let parent = destination.parent().unwrap_or(Path::new("."));
    let temp = tempfile::tempdir_in(parent).map_err(|e| e.to_string())?;
    let avi_path = temp.path().join("render.avi");
    let audio = project.audio.as_deref().map(Audio::read).transpose()?;
    let frames = project.frames();
    progress.total.store(frames, Ordering::Relaxed);
    let (w, h) = project.layout.size(project.height);
    let file = std::fs::File::create(&avi_path).map_err(|e| e.to_string())?;
    let mut avi = Avi::new(
        std::io::BufWriter::new(file),
        w,
        h,
        project.fps,
        frames,
        audio.as_ref(),
    )?;
    let mut renderer = Renderer::default();
    let mut jpeg = Vec::new();
    for n in 0..frames {
        progress.check()?;
        let frame = renderer.frame(project, n as f64 / project.fps as f64, (w, h))?;
        jpeg.clear();
        JpegEncoder::new_with_quality(&mut jpeg, 88)
            .encode_image(&frame)
            .map_err(|e| e.to_string())?;
        let pcm = audio.as_ref().map(|a| {
            a.frame(
                n,
                project.fps,
                frames,
                project.audio_volume,
                project.audio_loop,
            )
        });
        avi.frame(&jpeg, pcm.as_deref())?;
        progress.done.store(n + 1, Ordering::Relaxed);
    }
    let mut writer = avi.finish()?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())?;
    drop(writer);
    progress.check()?;
    let final_path = if format == Format::Mp4 {
        progress.encoding.store(true, Ordering::Relaxed);
        let output = temp.path().join("encoded.mp4");
        transcode(ffmpeg.unwrap(), &avi_path, &output, progress)?;
        output
    } else {
        avi_path
    };
    progress.check()?;
    // Atomic publication without replacing an existing destination (including races).
    let mut published = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut source = std::fs::File::open(&final_path).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        progress.check()?;
        let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        published
            .write_all(&buffer[..n])
            .map_err(|e| e.to_string())?;
    }
    published.as_file().sync_all().map_err(|e| e.to_string())?;
    progress.check()?;
    published
        .persist_noclobber(destination)
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn transcode(ffmpeg: &Path, input: &Path, output: &Path, progress: &Progress) -> Result<()> {
    // MPEG-4 Part 2 and AAC avoid requiring libx264/GPL-enabled builds.
    let log = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut child = command(ffmpeg)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-i"])
        .arg(input)
        .args([
            "-c:v",
            "mpeg4",
            "-q:v",
            "3",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-threads",
            "2",
            "-progress",
            "pipe:1",
        ])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone().map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let p = progress.clone();
    // One pipe reader avoids blocking the cancellation loop or a full child pipe.
    let reader = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if let Some(n) = line
                .strip_prefix("frame=")
                .and_then(|n| n.trim().parse::<u32>().ok())
            {
                p.done.store(n, Ordering::Relaxed);
            }
        }
    });
    loop {
        if let Err(e) = progress.check() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(e);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = reader.join();
                if status.success() {
                    return Ok(());
                }
                use std::io::{Seek, SeekFrom};
                let mut log = log;
                let _ = log.seek(SeekFrom::Start(0));
                let mut text = String::new();
                let _ = log.take(16 * 1024).read_to_string(&mut text);
                return Err(format!("FFmpeg failed ({status}): {text}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(e.to_string());
            }
        }
    }
}
pub fn suggested_name(p: &Project, format: Format) -> PathBuf {
    let name: String = p
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    PathBuf::from(format!(
        "{}.{}",
        if name.trim().is_empty() {
            "Slideshow"
        } else {
            &name
        },
        if format == Format::Avi { "avi" } else { "mp4" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exports_real_jpeg_frames_and_cancels_without_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("movie.avi");
        let mut slide = Slide::title();
        slide.seconds = 0.5;
        let p = Project {
            slides: vec![slide],
            height: 360,
            fps: 12,
            ..Project::default()
        };
        export(&p, &path, Format::Avi, None, &Progress::default()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        let offset = bytes.windows(4).position(|v| v == b"movi").unwrap() + 4;
        assert_eq!(&bytes[offset..offset + 4], b"00dc");
        let len = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let image = image::load_from_memory(&bytes[offset + 8..offset + 8 + len]).unwrap();
        assert_eq!(image.width(), 640);
        assert!(export(&p, &path, Format::Avi, None, &Progress::default()).is_err());
        let cancel = Progress::default();
        cancel.cancel();
        let cancelled = dir.path().join("cancel.avi");
        assert!(export(&p, &cancelled, Format::Avi, None, &cancel).is_err());
        assert!(!cancelled.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
