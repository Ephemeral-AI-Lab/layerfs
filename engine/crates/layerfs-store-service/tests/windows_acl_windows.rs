//! The owner-only DACL boundary on Windows: what this process creates is
//! restricted to its owner, a shared entry is refused, and the owner keeps
//! working access.
#![cfg(target_os = "windows")]
use layerfs_store_service::windows_acl::{require_owner_only, restrict_to_owner};

fn scratch(label: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-acl-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn a_restricted_entry_is_owner_only_and_still_usable() {
    let root = scratch("owner");
    let file = root.join("record");
    std::fs::write(&file, b"body").unwrap();
    restrict_to_owner(&file).unwrap();
    restrict_to_owner(&root).unwrap();
    require_owner_only(&file).unwrap();
    require_owner_only(&root).unwrap();
    // The owner keeps full access after the restriction.
    std::fs::write(&file, b"second").unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"second");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_entry_shared_with_everyone_is_refused() {
    let root = scratch("shared");
    let file = root.join("record");
    std::fs::write(&file, b"body").unwrap();
    let granted = std::process::Command::new("icacls")
        .arg(&file)
        .args(["/grant", "*S-1-1-0:(R)"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(granted.success(), "icacls grants Everyone read");
    let error = require_owner_only(&file).unwrap_err();
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::Unsupported,
        "a shared entry never passes the privacy boundary: {error}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
