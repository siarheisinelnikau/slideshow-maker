//! Product configuration for the shared release downloader and portable installer.
use crate::model::Result;
pub use rust_auto_updator::Release as Update;
use rust_auto_updator::{Config, Prepared};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub struct Staged {
    pub version: String,
    prepared: Prepared,
}

pub fn config() -> Result<Config> {
    let mut config = Config::new(
        "siarheisinelnikau",
        "slideshow-maker",
        env!("CARGO_PKG_VERSION"),
        "SlideshowMaker",
    )?;
    config.checksum_name = "{executable}.sha256".into();
    config.max_executable_size = 256 * 1024 * 1024;
    config.install.stage_prefix = ".SlideshowMaker-update-".into();
    config.install.helper_name = "SlideshowMaker-update-helper.exe".into();
    Ok(config)
}

pub fn check() -> Result<Option<Update>> {
    rust_auto_updator::check_latest(&config()?)
}

pub fn download(update: &Update, progress: Arc<AtomicU64>) -> Result<Staged> {
    let prepared = rust_auto_updator::prepare_download(update, &config()?, |received, _| {
        progress.store(received, Ordering::Relaxed);
    })?;
    Ok(Staged {
        version: update.version.to_string(),
        prepared,
    })
}

pub fn restart_install(staged: &mut Staged) -> Result<()> {
    rust_auto_updator::handoff(&mut staged.prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_release_contract_and_origin_are_preserved() {
        let config = config().unwrap();
        let json = |origin: &str| {
            serde_json::json!({
            "tag_name":"v0.2.0", "html_url":format!("{origin}/releases/tag/v0.2.0"),
            "draft":false,"prerelease":false,"assets":[
                {"name":"SlideshowMaker-v0.2.0.exe","size":2048,
                 "browser_download_url":format!("{origin}/releases/download/v0.2.0/SlideshowMaker-v0.2.0.exe")},
                {"name":"SlideshowMaker-v0.2.0.exe.sha256","size":100,
                 "browser_download_url":format!("{origin}/releases/download/v0.2.0/SlideshowMaker-v0.2.0.exe.sha256")}
            ]
        }).to_string()
        };
        let release = rust_auto_updator::parse_release(&json(&config.repository_url()), &config)
            .unwrap()
            .unwrap();
        assert!(
            release
                .checksum_url()
                .unwrap()
                .ends_with("SlideshowMaker-v0.2.0.exe.sha256")
        );
        assert!(
            rust_auto_updator::parse_release(&json("https://github.com/other/app"), &config)
                .is_err()
        );
    }
}
