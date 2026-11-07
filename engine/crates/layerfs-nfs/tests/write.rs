//! A writable Workspace served over loopback NFSv3: every changing
//! procedure and its error statuses, the publication-ticket contract (each
//! ticket released only after its reply was received, contended serial
//! reservations), the export's own handling of client silly renames and
//! AppleDouble names, and a product Commit whose tree equals what the same
//! operations made of a host directory.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/spy.rs"]
mod spy;
#[allow(dead_code)]
#[path = "support/tree.rs"]
mod tree;
use client::{
    Client, How, Set, ACCES, EXIST, INVAL, ISDIR, JUKEBOX, NAMETOOLONG, NOENT, NOTDIR, NOTEMPTY,
    NOTSUPP, NOT_SYNC, OK, PERM, STALE,
};
use layerfs_daemon::Mounted;
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

const UID: u32 = 501;
const GID: u32 = 20;
const FILE_SYNC: u32 = 2;
const LINUX_SILLY: &[u8] = b".nfs00000000000012ab00000001";
const MAC_SILLY: &[u8] = b".nfs.20051025.b896";

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-w{}-{label}", std::process::id()))
}
fn source(root: &Path) {
    fs::create_dir_all(root.join("src/nested")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), b"# fixture\nhello nfs\n").unwrap();
    fs::write(root.join("src/lib.rs"), b"pub fn answer() -> u32 { 42 }\n").unwrap();
    fs::write(root.join("src/nested/deep.txt"), b"deep\n").unwrap();
    fs::write(root.join("docs/guide.md"), b"guide\n").unwrap();
    fs::hard_link(root.join("src/lib.rs"), root.join("src/alias.rs")).unwrap();
    std::os::unix::fs::symlink("src/lib.rs", root.join("link")).unwrap();
    fs::write(root.join("run.sh"), b"#!/bin/sh\n").unwrap();
    fs::set_permissions(root.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
}
fn pattern(seed: u32, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(seed * 97) as u8)
        .collect()
}

#[test]
fn every_changing_procedure_and_its_statuses() {
    let base = temp("procedures");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // CREATE: UNCHECKED, GUARDED over an existing name, UNCHECKED again.
    let made = c.touch(&root, b"new.txt", 0o640).unwrap();
    assert_eq!(
        (made.attr.kind, made.attr.mode, made.attr.size),
        (1, 0o640, 0)
    );
    assert_eq!((made.attr.uid, made.attr.gid), (UID, GID));
    assert!(made.directory.before.is_some() && made.directory.after.is_some());
    let guarded = How::Guarded(Set::default());
    assert_eq!(c.create(&root, b"new.txt", guarded).map(|_| ()), Err(EXIST));
    let again = c.touch(&root, b"new.txt", 0o600).unwrap();
    assert_eq!(again.attr.fileid, made.attr.fileid);
    assert_eq!(
        again.attr.mode, 0o640,
        "UNCHECKED keeps an existing file's mode"
    );
    let file = made.handle.clone();

    // WRITE: FILE_SYNC with this export's verifier; a hole reads as zeros;
    // a call larger than one engine window is split and fully written.
    let (count, committed, verifier, wcc) = c.write(&file, 0, b"hello").unwrap();
    assert_eq!(
        (count, committed, verifier),
        (5, FILE_SYNC, server.verifier())
    );
    assert_eq!(wcc.after.unwrap().size, 5);
    let big = pattern(1, 300_000);
    let (count, _, _, wcc) = c.write(&file, 10, &big).unwrap();
    assert_eq!(count as usize, big.len());
    assert_eq!(wcc.after.unwrap().size, 300_010);
    let mut expected = b"hello\0\0\0\0\0".to_vec();
    expected.extend_from_slice(&big);
    assert_eq!(c.read_all(&file, 128 * 1024), expected);
    assert_eq!(c.commit(&file), Ok(server.verifier()));

    // SETATTR: truncate, mode, client mtime, owner no-op and refusal, guard.
    let set = |size, mode| Set {
        size,
        mode,
        ..Set::default()
    };
    let after = c
        .setattr(&file, set(Some(3), None), None)
        .unwrap()
        .after
        .unwrap();
    assert_eq!(after.size, 3);
    assert_eq!(c.read_all(&file, 4096), b"hel");
    let after = c
        .setattr(&file, set(None, Some(0o600)), None)
        .unwrap()
        .after
        .unwrap();
    assert_eq!(after.mode, 0o600);
    let stamp = Set {
        mtime: Some(Some((1_000_000_000, 5))),
        ..Set::default()
    };
    let after = c.setattr(&file, stamp, None).unwrap().after.unwrap();
    assert_eq!(after.mtime, (1_000_000_000, 5));
    let same = Set {
        uid: Some(UID),
        gid: Some(GID),
        ..Set::default()
    };
    assert!(
        c.setattr(&file, same, None).is_ok(),
        "chown to the same identity"
    );
    let other = Set {
        uid: Some(0),
        ..Set::default()
    };
    assert_eq!(
        c.setattr(&file, other, None).map(|_| ()).unwrap_err().0,
        PERM
    );
    let wrong = c.setattr(&file, set(Some(1), None), Some((7, 7)));
    assert_eq!(wrong.map(|_| ()).unwrap_err().0, NOT_SYNC);
    assert_eq!(
        c.getattr(&file).unwrap().size,
        3,
        "a failed guard changes nothing"
    );
    let right = c.setattr(&file, set(Some(1), None), Some(after.mtime));
    assert_eq!(right.unwrap().after.unwrap().size, 1);

    // MKDIR, SYMLINK, MKNOD.
    let dir = c.mkdir(&root, b"d", 0o750).unwrap();
    assert_eq!(
        (dir.attr.kind, dir.attr.mode, dir.attr.nlink),
        (2, 0o750, 2)
    );
    assert_eq!(c.mkdir(&root, b"d", 0o750).map(|_| ()), Err(EXIST));
    let (_, up) = c.lookup(&dir.handle, b"..").unwrap();
    assert_eq!(up.fileid, c.getattr(&root).unwrap().fileid);
    let link = c.symlink(&root, b"s", b"new.txt").unwrap();
    assert_eq!(link.attr.kind, 5);
    assert_eq!(c.readlink(&link.handle).unwrap(), b"new.txt");
    let long = vec![b'x'; 4096];
    assert_eq!(
        c.symlink(&root, b"long", &long).map(|_| ()),
        Err(NAMETOOLONG)
    );
    assert_eq!(c.mknod_fifo(&root, b"fifo"), NOTSUPP);
    assert_eq!(c.lookup(&root, b"fifo").map(|_| ()), Err(NOENT));

    // LINK: one more name and link count; directories and taken names refused.
    let linked = c.link(&file, &dir.handle, b"alias").unwrap();
    assert_eq!((linked.fileid, linked.nlink), (made.attr.fileid, 2));
    assert_ne!(c.link(&dir.handle, &root, b"dirlink").map(|_| ()), Ok(()));
    assert_eq!(c.link(&file, &root, b"README.md").map(|_| ()), Err(EXIST));

    // RENAME: across directories, of a directory, replacing, into itself.
    c.rename(&root, b"new.txt", &dir.handle, b"moved").unwrap();
    assert_eq!(c.lookup(&root, b"new.txt").map(|_| ()), Err(NOENT));
    let (_, moved) = c.lookup(&dir.handle, b"moved").unwrap();
    assert_eq!(moved.fileid, made.attr.fileid);
    let sub = c.mkdir(&dir.handle, b"sub", 0o755).unwrap();
    c.rename(&dir.handle, b"sub", &root, b"top").unwrap();
    let (_, up) = c.lookup(&sub.handle, b"..").unwrap();
    assert_eq!(
        up.fileid,
        c.getattr(&root).unwrap().fileid,
        ".. follows a move"
    );
    assert_eq!(c.getattr(&dir.handle).unwrap().nlink, 2);
    assert_ne!(
        c.rename(&root, b"d", &dir.handle, b"inside").map(|_| ()),
        Ok(())
    );
    let (readme, readme_attr) = c.lookup(&root, b"README.md").unwrap();
    c.rename(&root, b"run.sh", &root, b"README.md").unwrap();
    assert_eq!(c.getattr(&readme), Err(STALE), "the replaced inode is gone");
    let (_, replaced) = c.lookup(&root, b"README.md").unwrap();
    assert_ne!(replaced.fileid, readme_attr.fileid);
    assert_eq!(replaced.mode, 0o755);
    let missing = c.rename(&root, b"absent", &root, b"x").map(|_| ());
    assert_eq!(missing, Err(NOENT));

    // REMOVE and RMDIR.
    c.remove(&dir.handle, b"alias").unwrap();
    assert_eq!(
        c.getattr(&file).unwrap().nlink,
        1,
        "the other name keeps it"
    );
    assert_eq!(c.remove(&root, b"absent").map(|_| ()), Err(NOENT));
    assert_eq!(c.remove(&root, b"d").map(|_| ()), Err(ISDIR));
    assert_eq!(c.rmdir(&root, b"d").map(|_| ()), Err(NOTEMPTY));
    assert_eq!(c.rmdir(&root, b"README.md").map(|_| ()), Err(NOTDIR));
    assert_eq!(c.rmdir(&root, b".").map(|_| ()), Err(INVAL));
    c.remove(&dir.handle, b"moved").unwrap();
    assert_eq!(c.getattr(&file), Err(STALE), "its last name is gone");
    c.rmdir(&root, b"d").unwrap();
    assert_eq!(c.getattr(&dir.handle), Err(STALE));

    // Names the export refuses or the protocol forbids.
    // An AppleDouble file is created detached (see the test below); other
    // kinds under such a name are refused.
    c.touch(&root, b"._new.txt", 0o644).unwrap();
    c.remove(&root, b"._new.txt").unwrap();
    assert_eq!(c.mkdir(&root, b"._d", 0o755).map(|_| ()), Err(ACCES));
    let refused = c.rename(&root, b"s", &root, b"._s").map(|_| ());
    assert_eq!(refused, Err(ACCES));
    assert_eq!(c.touch(&root, b".", 0o644).map(|_| ()), Err(EXIST));
    let long_name = vec![b'n'; 256];
    assert_eq!(
        c.touch(&root, &long_name, 0o644).map(|_| ()),
        Err(NAMETOOLONG)
    );

    // EXCLUSIVE: a retransmission finds its own file; another verifier does not.
    let first = c
        .create(&root, b"excl", How::Exclusive(*b"verifier"))
        .unwrap();
    let again = c
        .create(&root, b"excl", How::Exclusive(*b"verifier"))
        .unwrap();
    assert_eq!(first.attr.fileid, again.attr.fileid);
    let other = c.create(&root, b"excl", How::Exclusive(*b"another!"));
    assert_eq!(other.map(|_| ()), Err(EXIST));

    // ACCESS on a writable export grants changes by mode.
    let (guide, _) = c.walk_to(&root, "docs/guide.md").unwrap();
    assert_eq!(c.access(&guide, 0x3f), Ok(0x01 | 0x04 | 0x08));
    let (docs, _) = c.lookup(&root, b"docs").unwrap();
    assert_eq!(c.access(&docs, 0x3f), Ok(0x01 | 0x02 | 0x04 | 0x08 | 0x10));

    // Every serial handed out holds one reference: removed ones gave theirs back.
    let held = server.held();
    assert_eq!(
        (
            held.silly,
            held.retained,
            held.names_refused,
            held.companions,
            held.names_detached
        ),
        (0, 0, 2, 0, 1)
    );
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn symlink_link_handles_preserve_opaque_targets_through_commit_and_fresh_view() {
    let base = temp("symlink-links");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let (inherited, inherited_attr) = c.lookup(&root, b"link").unwrap();
    let inherited_alias = c.link(&inherited, &root, b"inherited-alias").unwrap();
    assert_eq!(
        (inherited_alias.fileid, inherited_alias.nlink),
        (inherited_attr.fileid, 2)
    );
    assert_eq!(c.readlink(&inherited).unwrap(), b"src/lib.rs");
    let target = b"../missing/\xff";
    let fresh = c.symlink(&root, b"opaque", target).unwrap();
    let separate = c.symlink(&root, b"distinct", target).unwrap();
    assert_ne!(separate.attr.fileid, fresh.attr.fileid);
    // NFS LINK takes a filehandle, so source-path symlink following is impossible.
    let linked = c.link(&fresh.handle, &root, b"opaque-alias").unwrap();
    assert_eq!(
        (linked.kind, linked.fileid, linked.nlink),
        (5, fresh.attr.fileid, 2)
    );
    let (alias, alias_attr) = c.lookup(&root, b"opaque-alias").unwrap();
    assert_eq!(alias_attr.fileid, fresh.attr.fileid);
    assert_eq!(c.readlink(&alias).unwrap(), target);
    assert_eq!(c.readlink(&fresh.handle).unwrap(), target);
    let (directory, _) = c.lookup(&root, b"docs").unwrap();
    assert_eq!(
        c.link(&directory, &root, b"directory-alias").map(|_| ()),
        Err(PERM)
    );
    assert!(rig.commit().history.is_some());
    let view = rig.view([78; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = v.mnt("/");
    for (name, alias_name, expected) in [
        (
            b"link".as_slice(),
            b"inherited-alias".as_slice(),
            b"src/lib.rs".as_slice(),
        ),
        (
            b"opaque".as_slice(),
            b"opaque-alias".as_slice(),
            target.as_slice(),
        ),
    ] {
        let (a, a_attr) = v.lookup(&view_root, name).unwrap();
        let (b, b_attr) = v.lookup(&view_root, alias_name).unwrap();
        assert_eq!(
            (a_attr.kind, a_attr.fileid, a_attr.nlink),
            (5, b_attr.fileid, 2)
        );
        assert_eq!(b_attr.nlink, 2);
        assert_eq!(v.readlink(&a).unwrap(), expected);
        assert_eq!(v.readlink(&b).unwrap(), expected);
    }
    drop(v);
    assert_eq!(viewer.stop().retained, 0);
    rig.close_view(view);
    c.remove(&root, b"opaque").unwrap();
    assert_eq!(c.getattr(&alias).unwrap().nlink, 1);
    assert_eq!(c.readlink(&alias).unwrap(), target);
    c.remove(&root, b"opaque-alias").unwrap();
    assert_eq!(c.getattr(&alias), Err(STALE));
    assert!(rig.commit().history.is_some());
    let view = rig.view([79; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = v.mnt("/");
    assert_eq!(v.lookup(&view_root, b"opaque").map(|_| ()), Err(NOENT));
    assert_eq!(
        v.lookup(&view_root, b"opaque-alias").map(|_| ()),
        Err(NOENT)
    );
    let (distinct, attr) = v.lookup(&view_root, b"distinct").unwrap();
    assert_eq!(attr.nlink, 1);
    assert_eq!(v.readlink(&distinct).unwrap(), target);
    drop(v);
    assert_eq!(viewer.stop().retained, 0);
    rig.close_view(view);
    drop(c);
    let (_, outstanding) = rig.stop(server.stop());
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn silly_renames_stay_out_of_the_workspace() {
    let base = temp("silly");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    for (silly, name) in [(LINUX_SILLY, &b"open.bin"[..]), (MAC_SILLY, b"open2.bin")] {
        let made = c.touch(&root, name, 0o644).unwrap();
        c.write(&made.handle, 0, b"still open").unwrap();
        assert_eq!(c.lookup(&root, silly).map(|_| ()), Err(NOENT));
        c.rename(&root, name, &root, silly).unwrap();
        assert_eq!(c.lookup(&root, name).map(|_| ()), Err(NOENT));
        let (handle, attr) = c.lookup(&root, silly).unwrap();
        assert_eq!(handle, made.handle);
        assert_eq!(attr.size, 10);
        // The open file stays readable, writable and truncatable.
        c.write(&handle, 10, b"!").unwrap();
        assert_eq!(c.read_all(&handle, 4096), b"still open!");
        let truncate = Set {
            size: Some(5),
            ..Set::default()
        };
        assert_eq!(
            c.setattr(&handle, truncate, None)
                .unwrap()
                .after
                .unwrap()
                .size,
            5
        );
        let (entries, _) = c.list(&root, 64 * 1024, true);
        assert!(entries.iter().all(|e| e.name != silly && e.name != name));
        assert_eq!(server.held().silly, 1);
        c.remove(&root, silly).unwrap();
        assert_eq!(c.getattr(&handle), Err(STALE));
        assert_eq!(server.held().silly, 0);
    }
    // A user's own names that only resemble the patterns are ordinary.
    let made = c.touch(&root, b".nfsrc", 0o644).unwrap();
    c.rename(&root, b".nfsrc", &root, b".nfs-old").unwrap();
    assert!(c.lookup(&root, b".nfs-old").is_ok());
    drop(made);
    // A silly name still held at Commit is not captured; neither is the
    // name it replaced.
    let kept = c.touch(&root, b"kept.bin", 0o644).unwrap();
    c.write(&kept.handle, 0, b"kept").unwrap();
    c.rename(&root, b"kept.bin", &root, LINUX_SILLY).unwrap();
    let done = rig.commit();
    assert!(done.history.is_some(), "{:?}", done.history);
    let view = rig.view([76; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = v.mnt("/");
    let (entries, _) = v.list(&view_root, 64 * 1024, false);
    let names: Vec<Vec<u8>> = entries.into_iter().map(|e| e.name).collect();
    assert!(names.contains(&b".nfs-old".to_vec()));
    assert!(!names
        .iter()
        .any(|n| n.starts_with(b".nfs0") || n == b"kept.bin"));
    drop(v);
    let view_released = viewer.stop();
    assert_eq!(view_released.retained, 0);
    rig.close_view(view);
    // Release gives the silly inode back.
    drop(c);
    let released = server.stop();
    assert_eq!((released.silly, released.silly_renamed), (1, 3));
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn apple_double_files_stay_out_of_the_workspace() {
    let base = temp("appledouble");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    // What macOS copyfile does for a source carrying extended attributes:
    // the data file, then its `._` companion with the attributes.
    let data = c.touch(&root, b"copied.rs", 0o644).unwrap();
    c.write(&data.handle, 0, b"fn main() {}").unwrap();
    let made = c.touch(&root, b"._copied.rs", 0o644).unwrap();
    // The companion is the export's alone: the directory it names is answered
    // whole in both halves of wcc_data, so the client keeps its cached
    // attributes of the directory (see the job-count test for the facts).
    assert!(made.directory.before.is_some() && made.directory.after.is_some());
    c.write(&made.handle, 0, b"AppleDouble header").unwrap();
    // It reads back under its name for the export's life ...
    let (handle, attr) = c.lookup(&root, b"._copied.rs").unwrap();
    assert_eq!(handle, made.handle);
    assert_eq!(attr.size, 18);
    assert_eq!(c.read_all(&handle, 4096), b"AppleDouble header");
    // ... opening it again truncates it, GUARDED finds it taken ...
    let truncate = Set {
        size: Some(0),
        ..Set::default()
    };
    let again = c
        .create(&root, b"._copied.rs", How::Unchecked(truncate))
        .unwrap();
    assert_eq!(
        (again.handle.clone(), again.attr.size),
        (made.handle.clone(), 0)
    );
    let guarded = c.create(&root, b"._copied.rs", How::Guarded(Set::default()));
    assert_eq!(guarded.map(|_| ()), Err(EXIST));
    // ... READDIR does not list it, it moves only to another AppleDouble name,
    // and a real file cannot take such a name.
    let (entries, _) = c.list(&root, 64 * 1024, true);
    assert!(entries.iter().all(|e| !e.name.starts_with(b"._")));
    c.rename(&root, b"._copied.rs", &root, b"._moved.rs")
        .unwrap();
    assert_eq!(c.lookup(&root, b"._copied.rs").map(|_| ()), Err(NOENT));
    let back = c
        .rename(&root, b"._moved.rs", &root, b"plain.rs")
        .map(|_| ());
    assert_eq!(back, Err(ACCES));
    let onto = c.rename(&root, b"copied.rs", &root, b"._x").map(|_| ());
    assert_eq!(onto, Err(ACCES));
    assert_eq!(server.held().companions, 1);
    // REMOVE gives it back.
    c.remove(&root, b"._moved.rs").unwrap();
    assert_eq!(c.getattr(&handle), Err(STALE));
    assert_eq!(server.held().companions, 0);
    // One still held at Commit is not captured; the data file is.
    let kept = c.touch(&root, b"._kept", 0o644).unwrap();
    c.write(&kept.handle, 0, b"kept").unwrap();
    let done = rig.commit();
    assert!(done.history.is_some(), "{:?}", done.history);
    let view = rig.view([77; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = v.mnt("/");
    let (entries, _) = v.list(&view_root, 64 * 1024, false);
    let names: Vec<Vec<u8>> = entries.into_iter().map(|e| e.name).collect();
    assert!(names.contains(&b"copied.rs".to_vec()));
    assert!(!names.iter().any(|n| n.starts_with(b"._")));
    drop(v);
    let view_released = viewer.stop();
    assert_eq!(view_released.retained, 0);
    rig.close_view(view);
    drop(c);
    let released = server.stop();
    assert_eq!(
        (
            released.silly,
            released.companions,
            released.names_detached,
            released.names_refused
        ),
        (0, 1, 2, 1)
    );
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tickets_are_released_only_after_the_reply() {
    let base = temp("tickets");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let watch = Arc::new(spy::Watch::default());
    let services = Arc::new(spy::Spy {
        inner: Arc::new(Mounted(rig.bound.clone())),
        watch: watch.clone(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = rig.export_services(listener, services, UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    c.wire = Some(watch.wire.clone());
    let root = c.mnt("/");

    let made = c.touch(&root, b"f", 0o644).unwrap();
    // 300 000 bytes are three engine windows: three publications, three
    // tickets, all released after the one reply.
    c.write(&made.handle, 0, &pattern(2, 300_000)).unwrap();
    let truncate = Set {
        size: Some(10),
        ..Set::default()
    };
    c.setattr(&made.handle, truncate, None).unwrap();
    c.mkdir(&root, b"dir", 0o755).unwrap();
    c.rename(&root, b"f", &root, b"g").unwrap();
    c.remove(&root, b"g").unwrap();
    let published = watch.published.load(Ordering::Acquire);
    assert_eq!(published, 1 + 3 + 1 + 1 + 1 + 1);
    // The last call's tickets are released just after its reply arrived.
    let settled = std::time::Instant::now() + Duration::from_secs(5);
    while watch.released_after.load(Ordering::Acquire) < published
        && std::time::Instant::now() < settled
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(watch.released_after.load(Ordering::Acquire), published);
    assert_eq!(
        watch.released_before.load(Ordering::Acquire),
        0,
        "{:?}",
        watch.log.lock().unwrap()
    );

    // A contended allocator: nothing is reserved and nothing created; the
    // client is told to retry later, and the retry succeeds.
    watch.refuse_serials.store(1, Ordering::Release);
    assert_eq!(c.touch(&root, b"later", 0o644).map(|_| ()), Err(JUKEBOX));
    assert_eq!(watch.refused.load(Ordering::Acquire), 1);
    assert_eq!(c.lookup(&root, b"later").map(|_| ()), Err(NOENT));
    assert_eq!(watch.published.load(Ordering::Acquire), published);
    assert!(c.touch(&root, b"later", 0o644).is_ok());

    // No ticket is left pending: the Workspace captures and commits.
    let done = rig.commit();
    assert!(done.history.is_some(), "{:?}", done.history);
    assert_eq!(server.held().retained, 0);
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

/// The same operations, over NFS and on a host directory.
struct Twin<'a> {
    client: &'a mut Client,
    root: Vec<u8>,
    host: PathBuf,
}
impl Twin<'_> {
    fn handle(&mut self, path: &str) -> Vec<u8> {
        self.client.walk_to(&self.root, path).unwrap().0
    }
    fn split(path: &str) -> (&str, &str) {
        path.rsplit_once('/').unwrap_or(("", path))
    }
    fn mkdir(&mut self, path: &str, mode: u32) {
        let (parent, name) = Self::split(path);
        let parent = self.handle(parent);
        self.client.mkdir(&parent, name.as_bytes(), mode).unwrap();
        fs::create_dir(self.host.join(path)).unwrap();
        fs::set_permissions(self.host.join(path), fs::Permissions::from_mode(mode)).unwrap();
    }
    fn write(&mut self, path: &str, offset: u64, data: &[u8]) {
        let (parent, name) = Self::split(path);
        let parent = self.handle(parent);
        let handle = match self.client.lookup(&parent, name.as_bytes()) {
            Ok((handle, _)) => handle,
            Err(_) => {
                let made = self.client.touch(&parent, name.as_bytes(), 0o644).unwrap();
                fs::write(self.host.join(path), b"").unwrap();
                fs::set_permissions(self.host.join(path), fs::Permissions::from_mode(0o644))
                    .unwrap();
                made.handle
            }
        };
        self.client.write_all(&handle, offset, data, 64 * 1024);
        use std::os::unix::fs::FileExt;
        let file = fs::OpenOptions::new()
            .write(true)
            .open(self.host.join(path))
            .unwrap();
        file.write_all_at(data, offset).unwrap();
    }
    fn truncate(&mut self, path: &str, size: u64) {
        let handle = self.handle(path);
        let set = Set {
            size: Some(size),
            ..Set::default()
        };
        self.client.setattr(&handle, set, None).unwrap();
        let file = fs::OpenOptions::new()
            .write(true)
            .open(self.host.join(path))
            .unwrap();
        file.set_len(size).unwrap();
    }
    fn chmod(&mut self, path: &str, mode: u32) {
        let handle = self.handle(path);
        let set = Set {
            mode: Some(mode),
            ..Set::default()
        };
        self.client.setattr(&handle, set, None).unwrap();
        fs::set_permissions(self.host.join(path), fs::Permissions::from_mode(mode)).unwrap();
    }
    fn rename(&mut self, from: &str, to: &str) {
        let (a, an) = Self::split(from);
        let (b, bn) = Self::split(to);
        let (a, b) = (self.handle(a), self.handle(b));
        self.client
            .rename(&a, an.as_bytes(), &b, bn.as_bytes())
            .unwrap();
        fs::rename(self.host.join(from), self.host.join(to)).unwrap();
    }
    fn link(&mut self, existing: &str, new: &str) {
        let file = self.handle(existing);
        let (parent, name) = Self::split(new);
        let parent = self.handle(parent);
        self.client.link(&file, &parent, name.as_bytes()).unwrap();
        fs::hard_link(self.host.join(existing), self.host.join(new)).unwrap();
    }
    fn symlink(&mut self, path: &str, target: &str) {
        let (parent, name) = Self::split(path);
        let parent = self.handle(parent);
        self.client
            .symlink(&parent, name.as_bytes(), target.as_bytes())
            .unwrap();
        std::os::unix::fs::symlink(target, self.host.join(path)).unwrap();
    }
    fn remove(&mut self, path: &str) {
        let (parent, name) = Self::split(path);
        let parent = self.handle(parent);
        let directory = fs::symlink_metadata(self.host.join(path)).unwrap().is_dir();
        self.client
            .unlink(&parent, name.as_bytes(), directory)
            .unwrap();
        if directory {
            fs::remove_dir(self.host.join(path)).unwrap();
        } else {
            fs::remove_file(self.host.join(path)).unwrap();
        }
    }
    /// A file the client still has open when it is unlinked: renamed to a
    /// silly name over NFS, simply gone on the host.
    fn unlink_open(&mut self, path: &str) {
        let (parent, name) = Self::split(path);
        let parent = self.handle(parent);
        self.client
            .rename(&parent, name.as_bytes(), &parent, LINUX_SILLY)
            .unwrap();
        fs::remove_file(self.host.join(path)).unwrap();
    }
}

#[test]
fn a_commit_holds_what_the_same_operations_make_of_a_host_tree() {
    let base = temp("commit");
    let tree = base.join("tree");
    source(&tree);
    let host = base.join("host");
    fs::create_dir_all(&host).unwrap();
    assert!(std::process::Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", tree.display()))
        .arg(&host)
        .status()
        .unwrap()
        .success());
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let mut t = Twin {
        client: &mut client,
        root,
        host: host.clone(),
    };
    t.mkdir("a", 0o755);
    t.mkdir("a/b", 0o700);
    t.mkdir("a/b/c", 0o755);
    t.write("a/b/c/big.bin", 0, &pattern(3, 400_000));
    t.write("a/b/c/big.bin", 123_456, &pattern(4, 70_000));
    t.truncate("a/b/c/big.bin", 350_001);
    t.write("a/empty", 0, b"");
    t.write("a/hello.txt", 0, b"hello\n");
    t.chmod("a/hello.txt", 0o755);
    t.write("README.md", 0, b"# rewritten\n");
    t.truncate("src/nested/deep.txt", 0);
    t.rename("a/hello.txt", "a/b/hello.txt");
    t.rename("a/b/c", "c-moved");
    t.link("a/b/hello.txt", "hello-alias");
    t.symlink("a/to-hello", "b/hello.txt");
    t.remove("src/alias.rs");
    t.remove("docs/guide.md");
    t.remove("docs");
    t.write("tmp-old", 0, b"old\n");
    t.write("tmp-new", 0, b"new\n");
    t.rename("tmp-new", "tmp-old");
    t.write("unicod\u{e9}.txt", 0, "caf\u{e9}\n".as_bytes());
    t.write("open-at-commit", 0, b"open\n");
    t.unlink_open("open-at-commit");
    drop(t);
    let done = rig.commit();
    assert!(done.history.is_some(), "{:?}", done.history);

    // An independent Workspace bound to the committed Branch, read over NFS,
    // equals the host tree path for path and byte for byte.
    let expected = tree::oracle(&host);
    let view = rig.view([77; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let (paths, _, identities) = tree::walk(&mut v, &expected, 4096, (UID, GID));
    assert_eq!(paths, expected.len());
    let hello = fs::metadata(host.join("hello-alias")).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(identities[&hello.ino()].len(), 1, "both names, one inode");
    drop(v);
    viewer.stop();
    rig.close_view(view);
    assert_eq!(OK, 0);
    drop(client);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[derive(Default)]
struct JobCounts {
    observe: std::sync::atomic::AtomicU64,
    mutate: std::sync::atomic::AtomicU64,
    close_file: std::sync::atomic::AtomicU64,
    forget: std::sync::atomic::AtomicU64,
}
impl JobCounts {
    fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.observe.load(Ordering::Acquire),
            self.mutate.load(Ordering::Acquire),
            self.close_file.load(Ordering::Acquire),
            self.forget.load(Ordering::Acquire),
        )
    }
}
struct CountingMount {
    inner: Arc<dyn layerfs_mount_ports::ports::MountServices>,
    counts: Arc<JobCounts>,
}
struct CountingRequest {
    inner: Arc<dyn layerfs_mount_ports::ports::RequestServices>,
    counts: Arc<JobCounts>,
}
impl layerfs_mount_ports::ports::MountServices for CountingMount {
    fn request(
        &self,
        fence: &layerfs_mount_ports::ports::Fence,
    ) -> Result<
        Arc<dyn layerfs_mount_ports::ports::RequestServices>,
        layerfs_mount_ports::ports::ServiceError,
    > {
        Ok(Arc::new(CountingRequest {
            inner: self.inner.request(fence)?,
            counts: self.counts.clone(),
        }))
    }
}
impl layerfs_mount_ports::ports::RequestServices for CountingRequest {
    fn directory_visit(
        &self,
        mount: layerfs_overlay::KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> layerfs_mount_ports::ports::ServiceFuture<
        '_,
        layerfs_mount_ports::ports::ServiceReply<Arc<layerfs_workspace::KernelDirectoryWindow>>,
    > {
        self.inner
            .directory_visit(mount, serial, handle, offset, after)
    }
    fn publish_cookies(
        &self,
        offer: layerfs_overlay::KernelCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_mount_ports::ports::ServiceReply<()>>
    {
        self.inner.publish_cookies(offer, names)
    }
    fn close_directory(
        &self,
        mount: layerfs_overlay::KernelMount,
        serial: u64,
        handle: u64,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_mount_ports::ports::ServiceReply<()>>
    {
        self.inner.close_directory(mount, serial, handle)
    }
    fn read_visit(
        &self,
        mount: layerfs_overlay::KernelMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> layerfs_mount_ports::ports::ServiceFuture<
        '_,
        layerfs_mount_ports::ports::ServiceReply<Arc<layerfs_workspace::KernelWindow>>,
    > {
        self.inner.read_visit(mount, serial, handle, offset, length)
    }
    fn close_file(
        &self,
        mount: layerfs_overlay::KernelMount,
        serial: u64,
        handle: u64,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_mount_ports::ports::ServiceReply<()>>
    {
        self.counts.close_file.fetch_add(1, Ordering::AcqRel);
        self.inner.close_file(mount, serial, handle)
    }
    fn provider_failure(
        &self,
    ) -> Result<
        Option<layerfs_mount_ports::ports::ServiceError>,
        layerfs_mount_ports::ports::ServiceError,
    > {
        self.inner.provider_failure()
    }
    fn failed_base_read(
        &self,
        step: layerfs_mount_ports::ports::ServiceError,
    ) -> layerfs_mount_ports::ports::ServiceError {
        self.inner.failed_base_read(step)
    }
    fn observe_visit(
        &self,
        mount: layerfs_overlay::KernelMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        operation: layerfs_workspace::KernelReadOperation,
        facts: Arc<layerfs_workspace::VisitFacts>,
    ) -> layerfs_mount_ports::ports::ServiceFuture<
        '_,
        layerfs_mount_ports::ports::ServiceReply<Arc<layerfs_workspace::KernelReadOutcome>>,
    > {
        self.counts.observe.fetch_add(1, Ordering::AcqRel);
        self.inner
            .observe_visit(mount, request, serial, handle, operation, facts)
    }
    fn base(&self) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_workspace::BaseView> {
        self.inner.base()
    }
    fn mutate_visit(
        &self,
        request: layerfs_workspace::KernelVisitRequest,
    ) -> layerfs_mount_ports::ports::ServiceFuture<
        '_,
        layerfs_mount_ports::ports::ServiceReply<Arc<layerfs_workspace::KernelMutationOutcome>>,
    > {
        self.counts.mutate.fetch_add(1, Ordering::AcqRel);
        self.inner.mutate_visit(request)
    }
    fn recognize(
        &self,
        serial: u64,
        offset: u64,
        data: layerfs_workspace::WriteData,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_workspace::WriteData> {
        self.inner.recognize(serial, offset, data)
    }
    fn forget(
        &self,
        mount: layerfs_overlay::KernelMount,
        serial: u64,
        count: u64,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_mount_ports::ports::ServiceReply<()>>
    {
        self.counts.forget.fetch_add(1, Ordering::AcqRel);
        self.inner.forget(mount, serial, count)
    }
    fn reserve_serial(&self) -> Result<Option<u64>, layerfs_mount_ports::ports::ServiceError> {
        self.inner.reserve_serial()
    }
    fn reply_attempted(
        &self,
        publication: layerfs_overlay::Publication,
    ) -> layerfs_mount_ports::ports::ServiceFuture<'_, layerfs_mount_ports::ports::ServiceReply<()>>
    {
        self.inner.reply_attempted(publication)
    }
}

#[test]
fn create_write_remove_and_apple_double_avoid_redundant_owner_jobs_and_orphans() {
    let base = temp("job-counts");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let counts = Arc::new(JobCounts::default());
    let services = Arc::new(CountingMount {
        inner: Arc::new(Mounted(rig.bound.clone())),
        counts: counts.clone(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = rig.export_services(listener, services, UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // Warm the root directory's base facts with one initial MKDIR+RMDIR so
    // subsequent single-visit assertions measure resident steady-state jobs.
    let warm = c.mkdir(&root, b"warm", 0o755).unwrap();
    assert!(warm.directory.before.is_some() && warm.directory.after.is_some());
    let warm_rm = c.rmdir(&root, b"warm").unwrap();
    assert!(warm_rm.before.is_some() && warm_rm.after.is_some());

    // 1. CREATE opens an implicit writable handle in the same visit and
    //    carries wcc_data from the transaction with 0 observe_visit calls.
    let before = counts.snapshot();
    let made = c.touch(&root, b"single.txt", 0o644).unwrap();
    assert!(made.directory.before.is_some() && made.directory.after.is_some());
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 1, before.2, before.3),
        "CREATE: 1 mutate_visit, 0 observe_visit, 0 close_file, 0 forget"
    );
    assert_eq!((server.held().files, server.held().lookups), (1, 1));

    // 2. SETATTR carries wcc_data from the transaction with 0 observe_visit calls.
    let before = counts.snapshot();
    let chmod = Set {
        mode: Some(0o600),
        ..Set::default()
    };
    let wcc = c.setattr(&made.handle, chmod, None).unwrap();
    assert!(wcc.before.is_some() && wcc.after.is_some());
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 1, before.2, before.3),
        "SETATTR: 1 mutate_visit, 0 observe_visit"
    );

    // 3. WRITE reuses the implicit writable handle from CREATE (0 open_file
    //    observe_visit) and carries wcc_data with 0 extra getattr calls.
    let before = counts.snapshot();
    let (written, _, _, wcc) = c.write(&made.handle, 0, b"payload").unwrap();
    assert_eq!(written, 7);
    assert!(wcc.before.is_some() && wcc.after.is_some());
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 1, before.2, before.3),
        "WRITE: 1 mutate_visit, 0 observe_visit"
    );

    // 4. AppleDouble companion changing lifecycle (CREATE, WRITE, SETATTR,
    //    REMOVE) issues 0 SQLite owner jobs (0 observe_visit, 0 mutate_visit),
    //    and LOOKUP of an absent companion issues 0 lookup observe_visit
    //    (only the 1 parent getattr in nfs.rs proc 3).
    let before = counts.snapshot();
    let ad = c.touch(&root, b"._single.txt", 0o644).unwrap();
    // The directory's wcc_data is not a hole: a companion changes nothing in
    // the directory it names, so both halves answer from the facts the export
    // already had. A missing post-operation half makes the macOS client drop
    // the directory's cached attributes and GETATTR it again for a later
    // entry, which is the regression this cache removes.
    let pre = ad
        .directory
        .before
        .as_ref()
        .expect("a companion CREATE answers the directory's pre-op attributes");
    let post = ad
        .directory
        .after
        .as_ref()
        .expect("a companion CREATE answers the directory's post-op attributes");
    assert_eq!(
        pre.0, post.size,
        "a companion changes nothing in its directory"
    );
    let (ad_written, _, _, ad_wcc) = c.write(&ad.handle, 0, b"xattr").unwrap();
    assert_eq!(ad_written, 5);
    assert!(ad_wcc.before.is_some() && ad_wcc.after.is_some());
    let ad_set = c.setattr(&ad.handle, chmod, None).unwrap();
    assert!(ad_set.before.is_some() && ad_set.after.is_some());
    let ad_rm = c.remove(&root, b"._single.txt").unwrap();
    assert!(
        ad_rm.before.is_some() && ad_rm.after.is_some(),
        "a companion REMOVE answers the directory's pre- and post-op attributes"
    );
    assert_eq!(
        counts.snapshot(),
        before,
        "AppleDouble CREATE+WRITE+SETATTR+REMOVE must issue 0 engine owner jobs"
    );
    assert_eq!(c.lookup(&root, b"._single.txt").map(|_| ()), Err(NOENT));
    assert_eq!(
        counts.snapshot(),
        (before.0 + 1, before.1, before.2, before.3),
        "AppleDouble LOOKUP avoids the target lookup visit"
    );

    // 5. REMOVE drops the last link and retires the synthetic kernel's file
    //    and lookup custody inside the publishing transaction: 1 mutate_visit,
    //    0 observe_visit (no existing() lookup), 0 close_file, 0 forget, and
    //    0 fake orphans in the overlay.
    let before = counts.snapshot();
    let rm_wcc = c.remove(&root, b"single.txt").unwrap();
    assert!(rm_wcc.before.is_some() && rm_wcc.after.is_some());
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 1, before.2, before.3),
        "REMOVE: 1 mutate_visit, 0 observe_visit, 0 close_file, 0 forget"
    );
    assert_eq!((server.held().files, server.held().lookups), (0, 0));
    let resources = rig
        .bound
        .operation()
        .unwrap()
        .overlay()
        .resources()
        .unwrap();
    assert_eq!(resources.open.orphan_files, 0, "no fake orphan created");
    assert_eq!(resources.open.file_handles, 0);
    assert_eq!(
        resources.open.file_custody, 1,
        "only the root mount reference remains"
    );

    // 6. FILES_HELD (1024) holds > 256 created+written files without LRU
    //    close_file evictions, and removing them issues 0 close_file/forget.
    let before = counts.snapshot();
    for i in 0..300 {
        let name = format!("batch-{i}");
        let f = c.touch(&root, name.as_bytes(), 0o644).unwrap();
        c.write(&f.handle, 0, b"x").unwrap();
    }
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 600, before.2, before.3),
        "300 files created+written: 600 mutate_visit, 0 observe_visit, 0 close_file evictions"
    );
    assert_eq!(server.held().files, 300);
    for i in 0..300 {
        let name = format!("batch-{i}");
        c.remove(&root, name.as_bytes()).unwrap();
    }
    assert_eq!(
        counts.snapshot(),
        (before.0, before.1 + 900, before.2, before.3),
        "300 files removed: 300 mutate_visit, 0 observe_visit, 0 close_file, 0 forget"
    );
    assert_eq!((server.held().files, server.held().lookups), (0, 0));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}
