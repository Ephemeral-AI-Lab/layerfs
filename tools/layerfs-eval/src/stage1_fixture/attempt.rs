use super::apfs::{prove_distinct_inodes, strict_clone_id};
use super::contract::{Attempt, BaseManifest, CloneReceipt, EvalResult};
use super::error::{display_error, io_error};
use super::location::{fixture_root, resolved_base_source, workspace_root};
use super::selector::read_selector;
use super::tree::{make_writable, tree_sizes};
use crate::legacy_full::{IntegrityMode, LayerFs, OpenedLayerFs};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static ATTEMPT_SERIAL: AtomicU64 = AtomicU64::new(0);

impl Attempt {
    pub fn create(base: &str, expected: &BaseManifest) -> EvalResult<Self> {
        Self::create_from(&fixture_root(), base, expected)
    }

    pub fn create_from(fixture: &Path, base: &str, expected: &BaseManifest) -> EvalResult<Self> {
        let reset_started = Instant::now();
        let source = resolved_base_source(fixture, base)?;
        let attempts = workspace_root().join("target/layerfs-stage1-attempts");
        fs::create_dir_all(&attempts).map_err(io_error)?;
        let attempts = attempts.canonicalize().map_err(io_error)?;
        let serial = ATTEMPT_SERIAL.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(display_error)?
            .as_nanos();
        let root = attempts.join(format!("attempt-{}-{nonce}-{serial}", std::process::id()));
        fs::create_dir(&root).map_err(io_error)?;
        let marker = format!(
            "layerfs-stage1-attempt:{}:{nonce}:{serial}",
            std::process::id()
        );
        let store = root.join("store");
        if let Err(error) = fs::write(root.join("OWNED"), marker.as_bytes()) {
            let _ = fs::remove_dir(&root);
            return Err(io_error(error));
        }
        let mut attempt = Self {
            root,
            store,
            marker,
            clone: CloneReceipt::default(),
        };
        let started = Instant::now();
        let status = Command::new("/bin/cp")
            .arg("-cR")
            .arg(&source)
            .arg(&attempt.store)
            .status()
            .map_err(io_error)?;
        let clone_wall_ns = started.elapsed().as_nanos();
        if !status.success() {
            return Err(format!(
                "APFS clone reset unavailable: /bin/cp -cR exited {status}"
            ));
        }
        let (source_logical_bytes, source_allocated_bytes) = tree_sizes(&source)?;
        let (destination_logical_bytes, destination_allocated_bytes) = tree_sizes(&attempt.store)?;
        if source_logical_bytes != destination_logical_bytes {
            return Err("clone reset logical-size mismatch".to_owned());
        }
        let distinct_regular_inodes = prove_distinct_inodes(&source, &attempt.store)?;
        let clone_id = strict_clone_id(&source, &attempt.store)?;
        make_writable(&attempt.root)?;
        let selector = read_selector(&attempt.store)?;
        if selector.store_id != expected.store_id
            || selector.profile_id != expected.profile_id
            || selector.generation != expected.selector_generation
        {
            return Err("clone reset StoreId/profile/CURRENT mismatch".to_owned());
        }
        attempt.clone = CloneReceipt {
            wall_ns: reset_started.elapsed().as_nanos(),
            clone_wall_ns,
            source_logical_bytes,
            destination_logical_bytes,
            source_allocated_bytes,
            destination_allocated_bytes,
            distinct_regular_inodes,
            clone_id,
        };
        Ok(attempt)
    }

    pub fn store(&self) -> &Path {
        &self.store
    }

    pub fn open(&self, expected: &BaseManifest, mode: IntegrityMode) -> EvalResult<OpenedLayerFs> {
        let selector = read_selector(&self.store)?;
        if selector.store_id != expected.store_id
            || selector.profile_id != expected.profile_id
            || selector.generation != expected.selector_generation
        {
            return Err("attempt selector identity mismatch".to_owned());
        }
        let opened = LayerFs::open_with_integrity(&self.store, mode).map_err(display_error)?;
        let head = opened.ref_state.clone();
        if head.root != expected.root || head.generation != expected.generation {
            return Err(format!(
                "attempt expected RefState mismatch for {}",
                expected.name
            ));
        }
        Ok(opened)
    }

    pub fn cleanup(mut self) -> EvalResult<()> {
        self.cleanup_inner()?;
        self.root = PathBuf::new();
        Ok(())
    }

    fn cleanup_inner(&self) -> EvalResult<()> {
        if self.root.as_os_str().is_empty() || !self.root.exists() {
            return Ok(());
        }
        let parent = workspace_root()
            .join("target/layerfs-stage1-attempts")
            .canonicalize()
            .map_err(io_error)?;
        let root = self.root.canonicalize().map_err(io_error)?;
        if root.parent() != Some(parent.as_path())
            || fs::read_to_string(root.join("OWNED")).map_err(io_error)? != self.marker
        {
            return Err(format!(
                "refusing unowned attempt cleanup: {}",
                root.display()
            ));
        }
        make_writable(&root)?;
        fs::remove_dir_all(root).map_err(io_error)
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let _ = self.cleanup_inner();
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::{read_selector, workspace_root, Attempt, BaseManifest, CloneReceipt, LayerFs};
    #[cfg(target_os = "macos")]
    use std::fs;
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(target_os = "macos")]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_attempt_admission_cleans_its_owned_clone() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-eval-attempt-cleanup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let fixture = root.join("fixture");
        let base = fixture.join("bases/read-reconstruct");
        fs::create_dir_all(base.parent().unwrap()).unwrap();
        let opened = LayerFs::open(&base).unwrap();
        let state = opened.ref_state.clone();
        drop(opened);
        let selector = read_selector(&base).unwrap();
        let expected = BaseManifest {
            name: "read-reconstruct".to_owned(),
            root: state.root,
            root_a: None,
            root_b: None,
            generation: state.generation,
            selector_generation: selector.generation,
            store_id: "deliberately-wrong-store-id".to_owned(),
            profile_id: selector.profile_id,
            store_database_bytes: super::super::selector::selected_database_bytes(
                &base,
                selector.generation,
            )
            .unwrap(),
        };
        let attempts = workspace_root().join("target/layerfs-stage1-attempts");
        fs::create_dir_all(&attempts).unwrap();
        let before = fs::read_dir(&attempts).unwrap().count();
        assert!(Attempt::create_from(&fixture, "read-reconstruct", &expected).is_err());
        assert_eq!(fs::read_dir(&attempts).unwrap().count(), before);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sealed_root = attempts.join(format!(
            "attempt-{}-{nonce}-sealed-cleanup",
            std::process::id()
        ));
        let sealed_store = sealed_root.join("store/nested");
        fs::create_dir_all(&sealed_store).unwrap();
        let marker = format!("sealed-cleanup-{nonce}");
        fs::write(sealed_root.join("OWNED"), &marker).unwrap();
        fs::write(sealed_store.join("file"), b"sealed").unwrap();
        fs::set_permissions(&sealed_store, std::fs::Permissions::from_mode(0o555)).unwrap();
        fs::set_permissions(
            sealed_store.parent().unwrap(),
            std::fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        drop(Attempt {
            root: sealed_root.clone(),
            store: sealed_root.join("store"),
            marker,
            clone: CloneReceipt::default(),
        });
        assert!(!sealed_root.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
