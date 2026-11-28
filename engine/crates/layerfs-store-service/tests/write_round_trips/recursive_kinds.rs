//! #818 W5 (lane W): recursive `rm` and `cp` take each listed page's kinds
//! from one demand and still copy and remove exactly what they did. A mixed
//! tree (files, nested directories, a symlink to a file and one to a
//! directory, two hard links, a path `project.hide_path` hides) is copied
//! and removed; a directory of 1,100 files (over the listing page of 1,024)
//! is copied and then removed in an `file.apply` that first adds entries in
//! the private overlay (kinds the overlay answers). The copy keeps names,
//! types, modes, sizes, contents, symlink targets and hard links; the
//! hidden path is not copied. Runs on SQLite, and on PostgreSQL with
//! `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it).
use crate::rt;

use layerfs_protocol::sdk::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Opened, Store};
use std::collections::BTreeMap;

/// One entry of a walked tree: type, mode, size (a link's target length),
/// file text, and the path of an earlier entry with its inode (hard links).
type Entry = (String, i64, i64, String, Option<String>);

fn call(opened: &Opened, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    opened
        .call(method, fields)
        .unwrap_or_else(|refusal| panic!("{method}: {refusal}"))
}

/// The entries of `dir`, every page of `file.list`.
fn listed(opened: &Opened, dir: &str) -> Vec<(String, String, i64)> {
    let (mut out, mut position) = (Vec::new(), SdkValue::Null);
    loop {
        let mut fields = vec![branch(), path(dir), ("pageSize", SdkValue::Int(1000))];
        if !matches!(position, SdkValue::Null) {
            fields.push(("pagePos", position.clone()));
        }
        let page = call(opened, "file.list", fields);
        let Some(SdkValue::List(items)) = page.get("items") else {
            panic!("no items: {page:?}");
        };
        for item in items {
            out.push((
                item.get_text("path").unwrap().to_owned(),
                item.get_text("type").unwrap().to_owned(),
                item.get_int("inode").unwrap(),
            ));
        }
        match page.get("nextPagePos") {
            None | Some(SdkValue::Null) => return out,
            Some(next) => position = next.clone(),
        }
    }
}

/// The tree under `root`, keyed by path relative to it.
fn walk(opened: &Opened, root: &str) -> BTreeMap<String, Entry> {
    let (mut out, mut inodes) = (BTreeMap::new(), BTreeMap::<i64, String>::new());
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        for (full, kind, inode) in listed(opened, &dir) {
            let relative = full[root.len()..].to_owned();
            let stat = call(opened, "file.stat", vec![branch(), path(&full)]);
            let body = match kind.as_str() {
                "file" => {
                    let read = call(opened, "file.read", vec![branch(), path(&full)]);
                    read.get_text("text").unwrap_or_default().to_owned()
                }
                _ => String::new(),
            };
            if kind == "dir" {
                stack.push(full.clone());
            }
            let linked = (kind == "file")
                .then(|| inodes.get(&inode).cloned())
                .flatten();
            inodes.entry(inode).or_insert_with(|| relative.clone());
            let mode = stat.get_int("mode").unwrap();
            let size = stat.get_int("size").unwrap();
            out.insert(relative, (kind, mode, size, body, linked));
        }
    }
    out
}

fn build(source: &std::path::Path) {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let mix = source.join("mix");
    std::fs::create_dir_all(mix.join("sub/deeper")).unwrap();
    std::fs::create_dir_all(mix.join("empty")).unwrap();
    std::fs::create_dir_all(mix.join("hide/inner")).unwrap();
    for (name, text) in [
        ("a.txt", "a\n"),
        ("sub/b.txt", "b\n"),
        ("sub/deeper/c.txt", "c\n"),
        ("hide/inner/gone.txt", "gone\n"),
        ("hide/kept.txt", "kept\n"),
    ] {
        std::fs::write(mix.join(name), text).unwrap();
    }
    std::fs::write(mix.join("run.sh"), "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(mix.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::hard_link(mix.join("a.txt"), mix.join("sub/a-link.txt")).unwrap();
    symlink("a.txt", mix.join("to-file")).unwrap();
    symlink("sub", mix.join("to-dir")).unwrap();
    let big = source.join("big");
    std::fs::create_dir_all(&big).unwrap();
    for n in 0..1100 {
        std::fs::write(big.join(format!("f{n:04}.txt")), format!("{n}\n")).unwrap();
    }
}

fn scenario(store: &mut Store) {
    let source = store.dir.join("source");
    build(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("kinds");
    rt::fork_work(&opened);
    call(
        &opened,
        "project.hide_path",
        vec![("path", t("/mix/hide/inner"))],
    );

    // The copy keeps everything visible, and hides nothing it copied.
    let original = walk(&opened, "/mix");
    assert!(original.contains_key("/hide/kept.txt"), "{original:?}");
    assert!(!original.contains_key("/hide/inner"), "the hidden path");
    assert_eq!(original["/sub/a-link.txt"].4.as_deref(), Some("/a.txt"));
    assert_eq!(original["/to-dir"].0, "symlink");
    let copied = call(
        &opened,
        "file.cp",
        vec![branch(), ("from", t("/mix")), ("to", t("/copy"))],
    );
    assert_eq!(copied.get_text("kind"), Some("Committed"), "{copied:?}");
    assert_eq!(walk(&opened, "/copy"), original, "the copy of /mix");

    // The recursive remove takes the whole tree, the copy stays.
    let removed = call(
        &opened,
        "file.rm",
        vec![branch(), path("/mix"), ("recursive", SdkValue::Bool(true))],
    );
    assert_eq!(removed.get_text("kind"), Some("Committed"), "{removed:?}");
    let gone = opened.call("file.stat", vec![branch(), path("/mix")]);
    assert!(gone.is_err_and(|e| e.starts_with("NoSuchPath")), "/mix");
    assert_eq!(walk(&opened, "/copy"), original, "the copy stays");

    // Over one listing page: copy 1,100 files, then remove the original in
    // one apply after adding overlay entries (a file and nested directories).
    call(
        &opened,
        "file.cp",
        vec![branch(), ("from", t("/big")), ("to", t("/big2"))],
    );
    let copies = listed(&opened, "/big2");
    assert_eq!(copies.len(), 1100);
    assert!(copies.iter().all(|(_, kind, _)| kind == "file"));
    let change = |op: &str, path: &str| SdkValue::map([("op", t(op)), ("path", t(path))]);
    let mut write = change("write", "/big/zz/sub/new.txt");
    if let SdkValue::Map(fields) = &mut write {
        fields.push(("content".into(), t("new\n")));
    }
    let mut remove = change("rm", "/big");
    if let SdkValue::Map(fields) = &mut remove {
        fields.push(("recursive".into(), SdkValue::Bool(true)));
    }
    let applied = call(
        &opened,
        "file.apply",
        vec![
            branch(),
            (
                "changes",
                SdkValue::List(vec![change("mkdir", "/big/zz/sub"), write, remove]),
            ),
        ],
    );
    assert_eq!(applied.get_text("kind"), Some("Committed"), "{applied:?}");
    let gone = opened.call("file.stat", vec![branch(), path("/big")]);
    assert!(gone.is_err_and(|e| e.starts_with("NoSuchPath")), "/big");
    assert_eq!(listed(&opened, "/big2").len(), 1100);
    opened.stop();
}

#[test]
fn sqlite_recursive_rm_and_cp_by_page_kinds() {
    let mut store = Store::sqlite("w5-kinds-sqlite");
    scenario(&mut store);
}

#[test]
fn postgresql_recursive_rm_and_cp_by_page_kinds() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("w5kinds") else {
        return;
    };
    scenario(&mut store);
}
