//! The volume guard's final path beyond the fixed buffer: a deep local
//! directory still passes through the regrowth, and a deep share spelling
//! is still refused. The old fixed-array regrowth could never fit a longer
//! requirement and left a zeroed buffer on which the `\\?\UNC\` refusal
//! silently passed (round-6 D1 F1). The share spelling is derived from the
//! temporary directory's own drive letter, so the discriminating half runs
//! on every host — a hard-coded `D:` silently skipped it wherever the
//! temporary directory lives elsewhere (round-8 F1 F1).
#![cfg(target_os = "windows")]
use layerfs_store_service::kernel_guard;
use std::path::PathBuf;

/// A directory chain whose final path exceeds 1023 UTF-16 units. The
/// verbatim spelling is built textually — `PathBuf::from(r"\\?\").join`
/// drops the prefix for an absolute component (round-8 F1 F3) — so
/// creation works beyond MAX_PATH on every host, relaxed or not.
fn deep(parent: &std::path::Path, levels: usize) -> (PathBuf, PathBuf) {
    let component = |level: usize| format!("guard-{level:03}-{}", "x".repeat(190));
    let mut plain = parent.to_path_buf();
    let mut verbatim = PathBuf::from(format!(r"\\?\{}", parent.display()));
    for level in 0..levels {
        plain.push(component(level));
        verbatim.push(component(level));
    }
    std::fs::create_dir_all(&verbatim).unwrap();
    (plain, verbatim)
}

#[test]
fn a_deep_local_directory_still_names_its_volume() {
    let base = std::env::temp_dir().join(format!("layerfs-volguard-local-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let (plain, verbatim) = deep(&base, 6); // ~1160 units of tail
    let filesystem =
        kernel_guard::check(&plain.join("store.sqlite")).expect("the local deep path passes");
    assert_eq!(filesystem.to_lowercase(), "ntfs");
    let _ = std::fs::remove_dir_all(&verbatim);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_deep_share_spelling_is_still_refused() {
    // The same physical tree addressed through the local machine's
    // administrative share for the temporary directory's own drive: the
    // guard's final path is `\\?\UNC\…` and, past the fixed buffer, must
    // still be recognised as a share.
    let base = std::env::temp_dir().join(format!("layerfs-volguard-share-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let (plain, verbatim) = deep(&base, 6);
    let spelling = plain.to_string_lossy().to_string();
    let units: Vec<char> = spelling.chars().collect();
    assert!(
        units.len() > 2 && units[1] == ':',
        "this test needs a drive-letter temporary directory, got {spelling}"
    );
    let drive = units[0].to_ascii_uppercase();
    let tail: String = units[3..].iter().collect();
    let share = PathBuf::from(format!(r"\\localhost\{drive}$\{tail}"));
    let refusal =
        kernel_guard::check(&share.join("store.sqlite")).expect_err("a share spelling is refused");
    assert!(
        refusal.0.contains("network share"),
        "the refusal names the share: {}",
        refusal.0
    );
    let _ = std::fs::remove_dir_all(&verbatim);
    let _ = std::fs::remove_dir_all(&base);
}
