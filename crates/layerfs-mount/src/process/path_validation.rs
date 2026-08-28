#[cfg(any(target_os = "linux", test))]
#[derive(Debug)]
pub(super) struct ValidatedPaths {
    pub(super) store: std::path::PathBuf,
    pub(super) mount: std::path::PathBuf,
    pub(super) spool: std::path::PathBuf,
    pub(super) receipt: std::path::PathBuf,
}

#[cfg(any(target_os = "linux", test))]
pub(super) const SOURCE_COMMIT: &str = match option_env!("LAYERFS_SOURCE_COMMIT") {
    Some(value) => value,
    None => "UNBOUND",
};
#[cfg(any(target_os = "linux", test))]
pub(super) const SOURCE_TREE: &str = match option_env!("LAYERFS_SOURCE_TREE") {
    Some(value) => value,
    None => "UNBOUND",
};

#[cfg(any(target_os = "linux", test))]
pub(super) fn required_integrity(
    arguments: &std::collections::HashMap<String, String>,
) -> Result<layerfs_workspace::IntegrityMode, Box<dyn std::error::Error>> {
    match arguments.get("integrity").map(String::as_str) {
        Some("trusted") => Ok(layerfs_workspace::IntegrityMode::TrustedLocalDev),
        Some("verified") => Ok(layerfs_workspace::IntegrityMode::Verified),
        Some(value) => Err(format!("unsupported integrity mode {value}").into()),
        None => Err("missing --integrity".into()),
    }
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn source_identity(
    allow_unbound: bool,
    commit: &'static str,
    tree: &'static str,
) -> Result<(&'static str, &'static str), Box<dyn std::error::Error>> {
    let valid = |value: &str| {
        value.len() == 40
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if valid(commit) && valid(tree) || allow_unbound && commit == "UNBOUND" && tree == "UNBOUND" {
        Ok((commit, tree))
    } else {
        Err("invalid embedded LayerFS source identity".into())
    }
}

#[cfg(test)]
pub(super) fn prepare_paths(
    store: &std::path::Path,
    mount: &std::path::Path,
    spool: &std::path::Path,
    receipt: &std::path::Path,
) -> Result<ValidatedPaths, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(mount)?;
    let mount = std::fs::canonicalize(mount)?;
    if std::fs::read_dir(&mount)?.next().is_some() {
        return Err(format!("mountpoint must be empty: {}", mount.display()).into());
    }
    let store = canonical_target(store)?;
    let spool = canonical_target(spool)?;
    let receipt = canonical_target(receipt)?;
    if receipt.exists() {
        return Err(format!("receipt already exists: {}", receipt.display()).into());
    }
    let external = [&store, &spool, &receipt];
    for (index, path) in external.iter().enumerate() {
        if path.starts_with(&mount) {
            return Err(format!("path must be outside mount: {}", path.display()).into());
        }
        for other in &external[..index] {
            if path == other || same_existing_file(path, other)? {
                return Err(format!(
                    "Store, spool, and receipt paths must be distinct: {}",
                    path.display()
                )
                .into());
            }
        }
    }
    Ok(ValidatedPaths {
        store,
        mount,
        spool,
        receipt,
    })
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn canonical_target(
    path: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = absolute
        .file_name()
        .ok_or_else(|| format!("path must name a file: {}", path.display()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let target = std::fs::canonicalize(parent)?.join(name);
    if target.exists() {
        Ok(std::fs::canonicalize(target)?)
    } else {
        Ok(target)
    }
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn same_existing_file(
    left: &std::path::Path,
    right: &std::path::Path,
) -> Result<bool, std::io::Error> {
    use std::os::unix::fs::MetadataExt;
    let (Ok(left), Ok(right)) = (std::fs::metadata(left), std::fs::metadata(right)) else {
        return Ok(false);
    };
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_is_explicit_and_validated() {
        let mut arguments = std::collections::HashMap::new();
        assert!(required_integrity(&arguments).is_err());
        arguments.insert("integrity".to_owned(), "trusted".to_owned());
        assert_eq!(
            required_integrity(&arguments).unwrap(),
            layerfs_workspace::IntegrityMode::TrustedLocalDev
        );
        arguments.insert("integrity".to_owned(), "verified".to_owned());
        assert_eq!(
            required_integrity(&arguments).unwrap(),
            layerfs_workspace::IntegrityMode::Verified
        );
        arguments.insert("integrity".to_owned(), "other".to_owned());
        assert!(required_integrity(&arguments).is_err());
    }

    #[test]
    fn release_source_identity_must_be_a_complete_bound_pair() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        assert!(source_identity(true, SOURCE_COMMIT, SOURCE_TREE).is_ok());
        assert_eq!(source_identity(false, oid, oid).unwrap(), (oid, oid));
        assert!(source_identity(false, "UNBOUND", "UNBOUND").is_err());
        assert!(source_identity(true, "UNBOUND", "UNBOUND").is_ok());
        assert!(source_identity(true, oid, "UNBOUND").is_err());
        assert!(source_identity(true, "0123", oid).is_err());
    }

    #[test]
    fn store_spool_and_receipt_are_distinct_and_outside_mount() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-mount-paths-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mount = root.join("mount");
        let safe = prepare_paths(
            &root.join("store.sqlite"),
            &mount,
            &root.join("spool"),
            &root.join("receipt.json"),
        )
        .unwrap();
        assert!(!safe.store.starts_with(&safe.mount));
        assert!(!safe.spool.starts_with(&safe.mount));
        assert!(!safe.receipt.starts_with(&safe.mount));
        assert!(prepare_paths(
            &mount.join("store.sqlite"),
            &mount,
            &root.join("spool-2"),
            &root.join("receipt-2.json"),
        )
        .is_err());
        std::fs::write(root.join("existing-receipt.json"), b"existing").unwrap();
        assert!(prepare_paths(
            &root.join("store-4.sqlite"),
            &mount,
            &root.join("spool-4"),
            &root.join("existing-receipt.json"),
        )
        .is_err());
        std::fs::write(mount.join("occupied"), b"occupied").unwrap();
        assert!(prepare_paths(
            &root.join("store-5.sqlite"),
            &mount,
            &root.join("spool-5"),
            &root.join("receipt-5.json"),
        )
        .is_err());
        assert!(prepare_paths(
            &root.join("same"),
            &mount,
            &root.join("same"),
            &root.join("receipt-3.json"),
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
