use crate::model::*;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    temp.write_all(&data).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn load_project(path: &Path) -> Result<Project> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
        return Err("Project file exceeds 4 MiB".into());
    }
    let mut p: Project = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    p.validate()?;
    p.resolve_paths(path.parent().unwrap_or(Path::new(".")));
    Ok(p)
}
pub fn save_project(path: &Path, project: &Project) -> Result<()> {
    project.validate()?;
    let destination = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let same_source = |source: &Path| {
        std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()) == destination
    };
    if project
        .slides
        .iter()
        .any(|s| matches!(&s.media,Media::Photo(p) if same_source(p)))
        || project.audio.as_deref().is_some_and(same_source)
    {
        return Err("A project cannot overwrite source media".into());
    }
    atomic_json(path, project)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub startup_updates: bool,
    pub ffmpeg: Option<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            startup_updates: true,
            ffmpeg: None,
        }
    }
}
pub fn settings_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("SlideshowMaker")
        .join("settings.json")
}
pub fn load_settings() -> Settings {
    std::fs::read(settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_roundtrip_and_reject_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.slideshow.json");
        let p = Project {
            slides: vec![Slide::photo(PathBuf::from("photo.png"))],
            ..Project::default()
        };
        save_project(&path, &p).unwrap();
        let loaded = load_project(&path).unwrap();
        assert!(matches!(&loaded.slides[0].media,Media::Photo(p) if p.is_absolute()));
        let mut bad = p.clone();
        bad.format_version = 2;
        assert!(save_project(&path, &bad).is_err());
        assert_eq!(load_project(&path).unwrap().format_version, 1);
    }
}
