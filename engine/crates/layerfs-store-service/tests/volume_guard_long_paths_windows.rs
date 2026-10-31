//! The volume guard's final path beyond the fixed buffer: a deep local
//! directory still passes through the regrowth, and a deep share spelling
//! is still refused. The old fixed-array regrowth could never fit a longer
//! requirement and left a zeroed buffer on which the `\\?\UNC\` refusal
//! silently passed (round-6 D1 F1).
#![cfg(target_os = "windows")]
use layerfs_store_service::kernel_guard;
use std::path::PathBuf;

/// A directory chain whose final path exceeds 1023 UTF-16 units, created
/// through a verbatim spelling so creation itself works beyond MAX_PATH.
fn deep(parent: &std::path::Path, levels: usize) -> PathBuf {
    let mut path = PathBuf::from(r"\\?\").join(parent);
    for level in 0..levels {
        path.push(format!("guard-{:03}-{}", level, "x".repeat(190)));
    }
    std::fs::create_dir_all(&path).unwrap();
    // The plain (non-verbatim) spelling of the same directory.
    let mut plain = parent.to_path_buf();
    for level in 0..levels {
        plain.push(format!("guard-{:03}-{}", level, "x".repeat(190)));
    }
    plain
}

#[test]
fn a_deep_local_directory_still_names_its_volume() {
    let base = std::env::temp_dir().join(format!("layerfs-volguard-local-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let deep = deep(&base, 6); // ~1160 units of tail
    let store = deep.join("store.sqlite");
    let filesystem = kernel_guard::check(&store).expect("the local deep path passes");
    assert_eq!(filesystem.to_lowercase(), "ntfs");
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn a_deep_share_spelling_is_still_refused() {
    // The same physical tree addressed through the local machine's
    // administrative share: the guard's final path is `\\?\UNC\…` and,
    // past the fixed buffer, must still be recognised as a share.
    let base = std::env::temp_dir().join(format!("layerfs-volguard-share-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let deep = deep(&base, 6);
    let plain = deep.to_string_lossy().to_string();
    let Some(tail) = plain
        .strip_prefix("D:\\")
        .or_else(|| plain.strip_prefix("d:\\"))
    else {
        // The temporary directory is not on D:; the share spelling of this
        // machine's share is unknown, so this test cannot run here.
        std::fs::remove_dir_all(&base).unwrap();
        return;
    };
    let share = PathBuf::from(r"\\localhost\D$\").join(tail);
    let store = share.join("store.sqlite");
    let refusal = kernel_guard::check(&store).expect_err("a share spelling is refused");
    assert!(
        refusal.0.contains("network share"),
        "the refusal names the share: {}",
        refusal.0
    );
    std::fs::remove_dir_all(&base).unwrap();
}
