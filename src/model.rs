use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    Landscape,
    Portrait,
    Square,
}
impl Layout {
    pub fn size(self, height: u32) -> (u32, u32) {
        match self {
            Self::Landscape => ((height * 16 / 9) & !1, height),
            Self::Portrait => ((height * 9 / 16) & !1, height),
            Self::Square => (height, height),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Fit {
    #[default]
    Fit,
    Fill,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Midnight,
    WarmPaper,
    Ocean,
}
impl Theme {
    pub fn colors(self) -> ([u8; 3], [u8; 3], [u8; 3]) {
        match self {
            Self::Midnight => ([19, 25, 40], [246, 244, 239], [239, 174, 104]),
            Self::WarmPaper => ([244, 234, 213], [46, 43, 36], [174, 88, 54]),
            Self::Ocean => ([16, 52, 64], [235, 250, 245], [114, 216, 191]),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Transition {
    Cut,
    #[default]
    Dissolve,
    FadeThroughBackground,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Media {
    Photo(PathBuf),
    Title,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slide {
    pub media: Media,
    pub caption: String,
    pub seconds: f64,
    pub fit: Fit,
    pub rotation: u8,
}
impl Slide {
    pub fn photo(path: PathBuf) -> Self {
        Self {
            media: Media::Photo(path),
            caption: String::new(),
            seconds: 4.0,
            fit: Fit::Fit,
            rotation: 0,
        }
    }
    pub fn title() -> Self {
        Self {
            media: Media::Title,
            caption: "Our favorite moments".into(),
            seconds: 3.0,
            fit: Fit::Fit,
            rotation: 0,
        }
    }
    pub fn label(&self) -> String {
        match &self.media {
            Media::Photo(p) => p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Media::Title => "Title card".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub slides: Vec<Slide>,
    pub layout: Layout,
    pub theme: Theme,
    pub transition: Transition,
    pub transition_seconds: f64,
    pub audio: Option<PathBuf>,
    pub audio_volume: f32,
    pub audio_loop: bool,
    pub fps: u32,
    pub height: u32,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: 1,
            name: "Family memories".into(),
            slides: vec![],
            layout: Layout::Landscape,
            theme: Theme::Midnight,
            transition: Transition::Dissolve,
            transition_seconds: 0.7,
            audio: None,
            audio_volume: 0.7,
            audio_loop: false,
            fps: 24,
            height: 720,
        }
    }
}
impl Project {
    pub fn duration(&self) -> f64 {
        self.slides.iter().map(|s| s.seconds).sum()
    }
    pub fn frames(&self) -> u32 {
        (self.duration() * self.fps as f64).ceil() as u32
    }
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            return Err("Unsupported project version".into());
        }
        if self.slides.len() > 500 {
            return Err("A project supports at most 500 slides".into());
        }
        if ![12, 24, 30].contains(&self.fps) || ![360, 720, 1080].contains(&self.height) {
            return Err("Unsupported frame rate or resolution".into());
        }
        if !self.transition_seconds.is_finite()
            || !(0.0..=2.0).contains(&self.transition_seconds)
            || !self.audio_volume.is_finite()
            || !(0.0..=1.0).contains(&self.audio_volume)
        {
            return Err("Invalid transition or audio settings".into());
        }
        for slide in &self.slides {
            if !slide.seconds.is_finite()
                || !(0.5..=120.0).contains(&slide.seconds)
                || slide.rotation > 3
                || slide.caption.chars().count() > 500
            {
                return Err("Slides require 0.5–120 seconds, a valid rotation, and at most 500 caption characters".into());
            }
        }
        if self.duration() > 3600.0 {
            return Err("The first version supports videos up to one hour".into());
        }
        Ok(())
    }
    pub fn set_total(&mut self, seconds: f64) -> Result<()> {
        if self.slides.is_empty() || !seconds.is_finite() {
            return Err("Add slides first".into());
        }
        let ratio = seconds / self.duration();
        let values: Vec<_> = self.slides.iter().map(|s| s.seconds * ratio).collect();
        if seconds > 3600.0 || values.iter().any(|v| !(0.5..=120.0).contains(v)) {
            return Err("Total duration would put a slide outside 0.5–120 seconds (maximum total: one hour)".into());
        }
        for (slide, value) in self.slides.iter_mut().zip(values) {
            slide.seconds = value;
        }
        Ok(())
    }
    // Dissolves use the end of each slide; slide durations always add up to the total.
    pub fn position(&self, seconds: f64) -> Option<(usize, f32)> {
        let mut start = 0.0;
        for (i, slide) in self.slides.iter().enumerate() {
            let end = start + slide.seconds;
            if seconds < end || i == self.slides.len() - 1 {
                let fade = self.transition_seconds.min(slide.seconds / 2.0);
                let mix = if self.transition == Transition::Cut
                    || i + 1 == self.slides.len()
                    || fade == 0.0
                {
                    0.0
                } else {
                    ((seconds - (end - fade)) / fade).clamp(0.0, 1.0) as f32
                };
                return Some((i, mix));
            }
            start = end;
        }
        None
    }
    pub fn resolve_paths(&mut self, base: &Path) {
        for s in &mut self.slides {
            if let Media::Photo(p) = &mut s.media
                && p.is_relative()
            {
                *p = base.join(&*p);
            }
        }
        if let Some(p) = &mut self.audio
            && p.is_relative()
        {
            *p = base.join(&*p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeline_and_rescaling_are_exact() {
        let mut p = Project {
            slides: vec![Slide::title(), Slide::title()],
            ..Project::default()
        };
        p.set_total(10.0).unwrap();
        assert_eq!(p.duration(), 10.0);
        assert_eq!(p.frames(), 240);
        assert_eq!(p.position(5.0), Some((1, 0.0)));
        assert!(p.position(4.65).unwrap().1 > 0.49);
        assert!(p.set_total(0.1).is_err());
        assert_eq!(p.duration(), 10.0);
    }
    #[test]
    fn rejects_invalid_files() {
        let mut p = Project {
            fps: 0,
            ..Project::default()
        };
        assert!(p.validate().is_err());
        p.fps = 24;
        p.slides.push(Slide::title());
        p.slides[0].seconds = f64::NAN;
        assert!(p.validate().is_err());
    }
}
