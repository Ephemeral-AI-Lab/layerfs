//! Rule 4 of the control split (layerfs-project#101, #106): in
//! `layerfs-sandbox`, `workspace/` may use the Sandbox modules; the Sandbox
//! modules never import `workspace/`.
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`, except the `workspace/` folder.
fn sources(dir: &Path, workspace: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path == workspace {
            continue;
        }
        if path.is_dir() {
            found.extend(sources(&path, workspace));
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
    found
}

/// The file's code with `//` comments removed.
fn code(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `code`, in a file `depth` folders below `src/`, reaches the
/// crate's `workspace` module (`super::` climbs one module per folder, and
/// `daemon/workspace` is another module).
fn reaches(code: &str, depth: usize) -> bool {
    let up = format!("{}workspace", "super::".repeat(depth + 1));
    [
        "crate::workspace",
        "layerfs_sandbox::workspace",
        up.as_str(),
    ]
    .iter()
    .any(|path| code.contains(path))
}

#[test]
fn sandbox_modules_never_import_workspace() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let workspace = src.join("workspace");
    assert!(workspace.is_dir(), "{} is missing", workspace.display());
    let files = sources(&src, &workspace);
    assert!(files.len() > 10, "too few Sandbox sources: {}", files.len());
    for file in files {
        if file == src.join("lib.rs") {
            // the crate root declares the module and nothing else of it
            assert!(!code(&file).contains("crate::workspace"));
            continue;
        }
        let depth = file.strip_prefix(&src).unwrap().components().count() - 1;
        assert!(
            !reaches(&code(&file), depth),
            "{} imports workspace/",
            file.display()
        );
    }
}

#[test]
fn the_check_sees_an_import() {
    assert!(reaches("use crate::workspace::serve;", 0));
    assert!(reaches("super::workspace::dispatch::call", 0));
    assert!(reaches("super::super::workspace::serve", 1));
    assert!(!reaches("use super::workspace::MountedWorkspace;", 1));
    assert!(!reaches(
        "use crate::daemon::workspace::MountedWorkspace;",
        0
    ));
}
