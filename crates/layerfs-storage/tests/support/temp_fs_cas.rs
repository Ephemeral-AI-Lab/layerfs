use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

pub struct TempFsCas {
    path: PathBuf,
}

impl TempFsCas {
    pub fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temporary root");
        let path = parent.join(format!(
            "layerfs-storage-{label}-{}-{sequence:016x}",
            std::process::id()
        ));
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn create_dir(&self) {
        fs::create_dir_all(&self.path).expect("create temporary fixture root");
    }
}

impl Drop for TempFsCas {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
