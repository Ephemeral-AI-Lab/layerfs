use super::contract::{EvalResult, BUFFER_BYTES, FILE_BYTES};
use super::error::{display_error, io_error};
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(crate) fn tree_sizes(root: &Path) -> EvalResult<(u64, u64)> {
    fn walk(path: &Path, logical: &mut u64, allocated: &mut u64) -> EvalResult<()> {
        let mut entries = fs::read_dir(path)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("fixture/reset may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&entry.path(), logical, allocated)?;
            } else if metadata.is_file() {
                *logical = logical
                    .checked_add(metadata.len())
                    .ok_or_else(|| "tree logical size overflow".to_owned())?;
                *allocated = allocated
                    .checked_add(metadata.blocks().saturating_mul(512))
                    .ok_or_else(|| "tree allocated size overflow".to_owned())?;
            }
        }
        Ok(())
    }
    let mut logical = 0;
    let mut allocated = 0;
    walk(root, &mut logical, &mut allocated)?;
    Ok((logical, allocated))
}

pub fn tree_digest(root: &Path, exclude: Option<&Path>) -> EvalResult<String> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    paths.sort();
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    for relative in paths {
        if exclude == Some(relative.as_path()) {
            continue;
        }
        let path = root.join(&relative);
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            return Err("fixture may not contain symlinks".to_owned());
        }
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update(&[if metadata.is_dir() { b'd' } else { b'f' }]);
        hasher.update(&(if metadata.is_dir() { 0o555_u32 } else { 0o444 }).to_be_bytes());
        hasher.update(&metadata.len().to_be_bytes());
        if metadata.is_file() {
            let mut file = File::open(path).map_err(io_error)?;
            loop {
                let read = file.read(&mut buffer).map_err(io_error)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn maximum_regular_file(root: &Path) -> EvalResult<Option<(PathBuf, u64)>> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    let mut maximum = None;
    for relative in paths {
        let metadata = fs::symlink_metadata(root.join(&relative)).map_err(io_error)?;
        if metadata.is_file()
            && maximum
                .as_ref()
                .is_none_or(|(_, bytes)| metadata.len() > *bytes)
        {
            maximum = Some((relative, metadata.len()));
        }
    }
    Ok(maximum)
}

pub fn verify_user_file_ceiling(root: &Path) -> EvalResult<()> {
    if let Some((path, bytes)) = maximum_regular_file(root)? {
        if bytes > FILE_BYTES {
            return Err(format!(
                "user input/intermediate/output {} is {bytes} bytes (> {FILE_BYTES})",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(super) fn collect_paths(root: &Path, path: &Path, output: &mut Vec<PathBuf>) -> EvalResult<()> {
    let mut entries = fs::read_dir(path)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .map_err(display_error)?
            .to_owned();
        output.push(relative);
        let metadata = fs::symlink_metadata(&entry_path).map_err(io_error)?;
        if metadata.is_dir() {
            collect_paths(root, &entry_path, output)?;
        }
    }
    Ok(())
}

pub(crate) fn seal_tree(root: &Path) -> EvalResult<()> {
    fn walk(path: &Path) -> EvalResult<()> {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("fixture may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&path)?;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).map_err(io_error)?;
            } else {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).map_err(io_error)?;
            }
        }
        Ok(())
    }
    walk(root)?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o555)).map_err(io_error)
}

pub(crate) fn verify_sealed(root: &Path) -> EvalResult<()> {
    let mut paths = vec![PathBuf::new()];
    collect_paths(root, root, &mut paths)?;
    for relative in paths {
        let metadata = fs::symlink_metadata(root.join(&relative)).map_err(io_error)?;
        let expected = if metadata.is_dir() { 0o555 } else { 0o444 };
        if metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o777 != expected {
            return Err(format!("fixture seal mismatch at {}", relative.display()));
        }
    }
    Ok(())
}

pub(crate) fn make_writable(root: &Path) -> EvalResult<()> {
    fn walk(path: &Path) -> EvalResult<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(io_error)?;
        for entry in fs::read_dir(path).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                return Err("attempt may not contain symlinks".to_owned());
            }
            if metadata.is_dir() {
                walk(&path)?;
            } else {
                fs::set_permissions(path, fs::Permissions::from_mode(0o644)).map_err(io_error)?;
            }
        }
        Ok(())
    }
    walk(root)
}

pub(crate) fn sync_directory(path: &Path) -> EvalResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::tree_digest;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn inventory_digest_normalizes_the_sealed_permission_contract() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-eval-inventory-permissions-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let file = root.join("entry");
        fs::write(&file, b"inventory").unwrap();
        let writable = tree_digest(&root, None).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(writable, tree_digest(&root, None).unwrap());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
