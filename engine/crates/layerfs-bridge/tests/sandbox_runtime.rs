//! The Sandbox runtime layer's command line, its presence check and the
//! search path a command gets under it (R5).
use layerfs_bridge::sandbox_runtime::{command, holds, search_path, GUEST, LOADER};
use std::{ffi::OsStr, path::Path};

#[test]
fn the_command_runs_the_program_through_the_bundled_loader() {
    let Some(name) = LOADER else { return };
    let command = command(Path::new(GUEST), "/bin/x").unwrap();
    assert_eq!(
        command.get_program(),
        Path::new(GUEST).join("lib").join(name)
    );
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(
        arguments,
        [
            OsStr::new("--inhibit-cache"),
            OsStr::new("--library-path"),
            OsStr::new("/.layerfs/runtime/lib"),
            OsStr::new("/bin/x"),
        ]
    );
}

#[test]
fn a_directory_without_the_loader_holds_no_runtime() {
    let directory = std::env::temp_dir().join(format!("lfs-runtime-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("lib")).unwrap();
    assert!(!holds(&directory));
    if let Some(name) = LOADER {
        std::fs::write(directory.join("lib").join(name), b"x").unwrap();
        assert!(holds(&directory));
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn the_runtime_bin_ends_the_search_path_once() {
    let runtime = Path::new(GUEST);
    assert_eq!(
        search_path(OsStr::new("/usr/bin:/bin"), runtime),
        "/usr/bin:/bin:/.layerfs/runtime/bin"
    );
    assert_eq!(
        search_path(OsStr::new("/.layerfs/runtime/bin:/usr/bin"), runtime),
        "/.layerfs/runtime/bin:/usr/bin"
    );
    assert_eq!(
        search_path(OsStr::new(""), runtime),
        "/.layerfs/runtime/bin"
    );
}
