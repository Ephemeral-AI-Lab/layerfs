//! Rule 3 of the control split (layerfs-project#101, #105): in
//! `layerfs-store`, `sqlite/` and `postgresql/` never import each other; the
//! kind-neutral root chooses by `StoreKind`.
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(sources(&path));
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

/// The paths by which a file under one kind folder could reach `other`.
fn reaches(code: &str, other: &str) -> bool {
    let crate_path = format!("crate::{other}");
    let named = format!("layerfs_store::{other}");
    let up = format!("super::{other}");
    code.contains(&crate_path) || code.contains(&named) || code.contains(&up)
}

fn check(kind: &str, other: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(kind);
    let files = sources(&dir);
    assert!(!files.is_empty(), "{} holds no source", dir.display());
    for file in files {
        assert!(
            !reaches(&code(&file), other),
            "{} imports {other}/",
            file.display()
        );
    }
}

#[test]
fn sqlite_never_imports_postgresql() {
    check("sqlite", "postgresql");
}

#[test]
fn postgresql_never_imports_sqlite() {
    check("postgresql", "sqlite");
}

#[test]
fn the_check_sees_an_import() {
    assert!(reaches("use crate::postgresql::open;", "postgresql"));
    assert!(reaches("super::super::sqlite::family::run()", "sqlite"));
    assert!(!reaches("use crate::record::StoreKind;", "sqlite"));
}
