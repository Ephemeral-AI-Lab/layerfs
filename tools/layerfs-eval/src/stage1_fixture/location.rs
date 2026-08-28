use super::contract::{EvalResult, FIXTURE_VERSION};
use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("evaluator is under workspace/tools")
        .to_owned()
}

pub fn fixture_root() -> PathBuf {
    workspace_root()
        .join("target/layerfs-stage1-fixtures")
        .join(FIXTURE_VERSION)
}

pub fn input_path(replacement: bool) -> PathBuf {
    fixture_root().join("input").join(if replacement {
        "S1-replace-100.bin"
    } else {
        super::contract::FILE_PATH
    })
}

pub(super) fn resolved_base_source(fixture: &Path, base: &str) -> EvalResult<PathBuf> {
    let fixture = fixture.canonicalize().map_err(super::error::io_error)?;
    let source = fixture.join("bases").join(base);
    let source = source.canonicalize().map_err(super::error::io_error)?;
    if source != fixture.join("bases").join(base) {
        return Err("base is not the exact sealed fixture path".to_owned());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::{fixture_root, resolved_base_source};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preparation_reopen_source_uses_private_fixture_root() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-eval-private-fixture-root-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let base = root.join("bases/read-reconstruct");
        fs::create_dir_all(&base).unwrap();
        assert_eq!(
            resolved_base_source(&root, "read-reconstruct").unwrap(),
            base.canonicalize().unwrap()
        );
        assert_ne!(root.canonicalize().unwrap(), fixture_root());
        fs::remove_dir_all(root).unwrap();
    }
}
