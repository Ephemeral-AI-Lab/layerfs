use phase6_live_probe::engine::Engine;
fn fresh() -> (std::path::PathBuf, Engine) {
    let p = std::env::temp_dir().join(format!(
        "p6-rename-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    let e = Engine::create(&p, 2).unwrap();
    (p, e)
}
#[test]
fn move_descendants_keeps_ids_and_parent_counts() {
    let (p, mut e) = fresh();
    let a = e.create_node(1, b"a", 2, 0o755).unwrap().id;
    let b = e.create_node(1, b"b", 2, 0o755).unwrap().id;
    let dir = e.create_node(a, b"tree", 2, 0o755).unwrap().id;
    let child = e.create_node(dir, b"file", 1, 0o644).unwrap().id;
    e.write(child, 0, b"literal").unwrap();
    e.db.execute("UPDATE inodes SET dirty=0", []).unwrap();
    e.db.execute("DELETE FROM changed_names", []).unwrap();
    e.rename(a, b"tree", b, b"moved", false).unwrap();
    assert_eq!(e.lookup(a, b"tree").unwrap(), None);
    assert_eq!(e.lookup(b, b"moved").unwrap(), Some(dir));
    assert_eq!(e.lookup(dir, b"file").unwrap(), Some(child));
    assert_eq!(e.node(dir).unwrap().parent, b);
    assert_eq!(e.node(a).unwrap().children, 0);
    assert_eq!(e.node(a).unwrap().subdirs, 0);
    assert_eq!(e.node(b).unwrap().subdirs, 1);
    assert!(!e.node(child).unwrap().dirty);
    let mut bytes = [0; 7];
    e.read(child, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"literal");
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn refused_cycles_types_nonempty_and_noreplace_preserve_namespace() {
    let (p, mut e) = fresh();
    let dir = e.create_node(1, b"dir", 2, 0o755).unwrap().id;
    let nested = e.create_node(dir, b"nested", 2, 0o755).unwrap().id;
    let file = e.create_node(1, b"file", 1, 0o644).unwrap().id;
    let revision = e.revision;
    assert_eq!(
        e.rename(1, b"dir", nested, b"cycle", false).unwrap_err(),
        "EINVAL"
    );
    assert_eq!(
        e.rename(1, b"file", 1, b"dir", false).unwrap_err(),
        "EISDIR"
    );
    assert_eq!(
        e.rename(1, b"dir", 1, b"file", false).unwrap_err(),
        "ENOTDIR"
    );
    assert_eq!(
        e.rename(1, b"file", 1, b"file", true).unwrap_err(),
        "EEXIST"
    );
    let empty = e.create_node(1, b"empty", 2, 0o755).unwrap().id;
    assert_eq!(
        e.rename(1, b"empty", 1, b"dir", false).unwrap_err(),
        "ENOTEMPTY"
    );
    assert_eq!(e.lookup(1, b"dir").unwrap(), Some(dir));
    assert_eq!(e.lookup(1, b"file").unwrap(), Some(file));
    assert_eq!(e.node(nested).unwrap().parent, dir);
    assert_eq!(e.lookup(1, b"empty").unwrap(), Some(empty));
    assert_eq!(e.revision, revision + 1);
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn replacement_preserves_open_victim_and_same_parent_refcounts() {
    let (p, mut e) = fresh();
    let a = e.create_node(1, b"a", 1, 0o644).unwrap().id;
    let b = e.create_node(1, b"b", 1, 0o644).unwrap().id;
    e.write(a, 0, b"new").unwrap();
    e.write(b, 0, b"old").unwrap();
    let h = e.open(b, libc::O_RDWR).unwrap();
    e.rename(1, b"a", 1, b"b", false).unwrap();
    assert_eq!(e.lookup(1, b"b").unwrap(), Some(a));
    assert_eq!(e.node(1).unwrap().children, 1);
    assert_eq!(e.node(b).unwrap().links, 0);
    e.handle(h, b, true).unwrap();
    let mut bytes = [0; 3];
    e.read(b, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"old");
    e.write(b, 0, b"FD!").unwrap();
    e.read(b, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"FD!");
    e.close(h, b).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn real_directory_listing_uses_names_parent_with_inode_parent_present() {
    let (p, mut e) = fresh();
    let z = e.create_node(1, b"z", 2, 0o755).unwrap().id;
    let a = e.create_node(1, b"a", 1, 0o644).unwrap().id;
    let nested = e.create_node(z, b"nested", 1, 0o644).unwrap().id;
    let root_handle = e.open(1, libc::O_RDONLY).unwrap();
    let nested_handle = e.open(z, libc::O_RDONLY).unwrap();
    let page = e.directory_page(root_handle, 1, 0).unwrap();
    assert_eq!(
        page.iter()
            .map(|r| (r.inode, r.name.clone(), r.kind))
            .collect::<Vec<_>>(),
        vec![(z, b"z".to_vec(), 2), (a, b"a".to_vec(), 1)]
    );
    let page = e.directory_page(nested_handle, z, 0).unwrap();
    assert_eq!(
        page.iter()
            .map(|r| (r.inode, r.name.clone(), r.kind))
            .collect::<Vec<_>>(),
        vec![(nested, b"nested".to_vec(), 1)]
    );
    e.close(root_handle, 1).unwrap();
    e.close(nested_handle, z).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
