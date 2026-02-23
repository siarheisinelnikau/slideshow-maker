pub mod avi;
pub mod export;
pub mod model;
pub mod render;
pub mod storage;
pub mod updater;

#[cfg(test)]
pub(crate) fn fixture_dir() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".test-output");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
