use crate::model::Result;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const API: &str = "https://api.github.com/repos/siarheisinelnikau/slideshow-maker/releases/latest";
const DOWNLOAD_PREFIX: &str =
    "https://github.com/siarheisinelnikau/slideshow-maker/releases/download/";
#[derive(Clone, Debug)]
pub struct Update {
    pub version: Version,
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}
pub fn select_release(json: &str, current: &str) -> Result<Option<Update>> {
    let release: Release =
        serde_json::from_str(json).map_err(|e| format!("Invalid release response: {e}"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version =
        Version::parse(release.tag_name.trim_start_matches('v')).map_err(|e| e.to_string())?;
    if !version.pre.is_empty() || version <= Version::parse(current).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Automatic installation currently supports Windows x64 only".into());
    }
    let name = format!("SlideshowMaker-v{version}.exe");
    let executable = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or("The newer release has no compatible portable executable")?;
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == format!("{name}.sha256"))
        .ok_or("The newer release has no required SHA-256 checksum asset")?;
    if !executable.browser_download_url.starts_with(DOWNLOAD_PREFIX)
        || !checksum.browser_download_url.starts_with(DOWNLOAD_PREFIX)
        || executable.size == 0
        || executable.size > 256 * 1024 * 1024
    {
        return Err("Release assets have an unexpected origin or size".into());
    }
    Ok(Some(Update {
        version,
        url: executable.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        size: executable.size,
    }))
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into()
}
pub fn check() -> Result<Option<Update>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut response=agent.get(API).header("User-Agent","SlideshowMaker/0.1.0").header("Accept","application/vnd.github+json").call()
        .map_err(|e|format!("Release check unavailable (repository may not be published, or network/rate limit): {e}"))?;
    let mut text = String::new();
    response
        .body_mut()
        .as_reader()
        .take(2 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    select_release(&text, env!("CARGO_PKG_VERSION"))
}
pub fn verify_checksum(bytes: &[u8], checksum: &str) -> Result<()> {
    let expected = checksum.split_whitespace().next().ok_or("Empty checksum")?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid SHA-256 checksum".into());
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("Downloaded update failed its SHA-256 check".into());
    }
    Ok(())
}
fn verify_pe(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err("Update is not a Windows executable".into());
    }
    let off = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
    if off.checked_add(6).is_none_or(|end| end > bytes.len())
        || &bytes[off..off + 4] != b"PE\0\0"
        || bytes[off + 4..off + 6] != [0x64, 0x86]
    {
        return Err("Update is not a compatible Windows x64 executable".into());
    }
    Ok(())
}
pub struct Staged {
    pub path: PathBuf,
    pub target: PathBuf,
    pub version: Version,
}
pub fn download(update: &Update, progress: Arc<AtomicU64>) -> Result<Staged> {
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = target
        .parent()
        .ok_or("Executable has no installation directory")?;
    // Creating the file before the network request catches read-only installations early.
    let mut staged = tempfile::Builder::new()
        .prefix(".SlideshowMaker-update-")
        .suffix(".exe")
        .tempfile_in(parent)
        .map_err(|e| format!("Cannot write update next to the application: {e}"))?;
    let agent = agent();
    let mut checksum_response = agent
        .get(&update.checksum_url)
        .header("User-Agent", "SlideshowMaker/0.1.0")
        .call()
        .map_err(|e| e.to_string())?;
    let mut checksum = String::new();
    checksum_response
        .body_mut()
        .as_reader()
        .take(4096)
        .read_to_string(&mut checksum)
        .map_err(|e| e.to_string())?;
    let mut response = agent
        .get(&update.url)
        .header("User-Agent", "SlideshowMaker/0.1.0")
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(update.size as usize);
    let mut buffer = [0u8; 65536];
    let mut reader = response.body_mut().as_reader();
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if bytes.len() as u64 + n as u64 > update.size {
            return Err("Download exceeds the published asset size".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
        progress.store(bytes.len() as u64, Ordering::Relaxed);
    }
    if bytes.len() as u64 != update.size {
        return Err("Incomplete update download".into());
    }
    verify_checksum(&bytes, &checksum)?;
    verify_pe(&bytes)?;
    staged.write_all(&bytes).map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    let (_, path) = staged.keep().map_err(|e| e.to_string())?;
    Ok(Staged {
        path,
        target,
        version: update.version.clone(),
    })
}
pub fn restart_install(staged: &Staged) -> Result<()> {
    if !cfg!(windows) {
        return Err("Portable installation requires Windows".into());
    }
    let parent = staged
        .target
        .parent()
        .ok_or("Missing installation directory")?;
    let mut helper = tempfile::Builder::new()
        .prefix(".SlideshowMaker-install-")
        .suffix(".ps1")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    helper
        .write_all(include_bytes!("../scripts/install-update.ps1"))
        .map_err(|e| e.to_string())?;
    let (_, script) = helper.keep().map_err(|e| e.to_string())?;
    let system_root = std::env::var_os("SystemRoot").ok_or("Windows SystemRoot is unavailable")?;
    let powershell = Path::new(&system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let result = crate::export::command(&powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-TargetPath")
        .arg(&staged.target)
        .arg("-StagedPath")
        .arg(&staged.path)
        .arg("-ParentId")
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = result {
        let _ = std::fs::remove_file(script);
        return Err(format!("Could not start update helper: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_versions_and_unpublished_errors_are_not_up_to_date() {
        assert!(select_release("{}", "0.1.0").is_err());
        assert!(
            select_release(
                r#"{"tag_name":"v0.1.0","draft":false,"prerelease":false,"assets":[]}"#,
                "0.1.0"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            select_release(
                r#"{"tag_name":"v0.2.0-beta.1","draft":false,"prerelease":true,"assets":[]}"#,
                "0.1.0"
            )
            .unwrap()
            .is_none()
        );
        assert!(Version::parse("0.10.0").unwrap() > Version::parse("0.9.0").unwrap());
    }
    #[test]
    fn checksum_rejects_tampered_download() {
        let hash = format!("{:x}  update.exe", Sha256::digest(b"good"));
        assert!(verify_checksum(b"good", &hash).is_ok());
        assert!(verify_checksum(b"bad", &hash).is_err());
        assert!(verify_pe(b"not an executable").is_err());
    }
}
