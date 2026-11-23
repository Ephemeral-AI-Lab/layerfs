//! A `._` name the Workspace view holds is an ordinary Workspace file while
//! the export keeps macOS AppleDouble companions out: READDIR lists it and
//! LOOKUP, READ, WRITE, CREATE, RENAME onto it and REMOVE reach it; only a
//! `._` name the view does not hold is a companion in export memory.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use client::{Client, How, Set, ACCES, EXIST, NOENT};
use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

const UID: u32 = 501;
const GID: u32 = 20;
/// The first serial of the export's own companions.
const COMPANIONS: u64 = 1 << 62;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-adv{}-{label}", std::process::id()))
}

#[test]
fn a_base_dot_underscore_file_is_listed_read_and_written_like_any_other() {
    let base = temp("view");
    let tree = base.join("tree");
    drop(fs::remove_dir_all(&base));
    fs::create_dir_all(tree.join("d")).unwrap();
    fs::write(tree.join("foo"), b"main\n").unwrap();
    fs::write(tree.join("._foo"), b"committed AppleDouble\n").unwrap();
    fs::write(tree.join("d/._bar"), b"nested\n").unwrap();
    fs::write(tree.join("plain.txt"), b"plain\n").unwrap();
    fs::write(tree.join("other"), b"replacement\n").unwrap();
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // READDIR and LOOKUP agree: the listed name opens, from the Workspace.
    let (entries, _) = c.list(&root, 64 * 1024, true);
    assert!(entries.iter().any(|entry| entry.name == b"._foo"));
    let (real, attr) = c.lookup(&root, b"._foo").unwrap();
    assert!(
        attr.fileid < COMPANIONS,
        "a Workspace inode, not a companion"
    );
    assert_eq!(attr.size, 22);
    assert_eq!(c.read_all(&real, 4096), b"committed AppleDouble\n");
    let (d, _) = c.lookup(&root, b"d").unwrap();
    let (nested, _) = c.lookup(&d, b"._bar").unwrap();
    assert_eq!(c.read_all(&nested, 4096), b"nested\n");

    // CREATE of a name the view holds is the ordinary CREATE of that file:
    // GUARDED finds it taken, UNCHECKED opens it, and no companion shadows it.
    let guarded = c.create(&root, b"._foo", How::Guarded(Set::default()));
    assert_eq!(guarded.map(|_| ()), Err(EXIST));
    let opened = c
        .create(&root, b"._foo", How::Unchecked(Set::default()))
        .unwrap();
    assert_eq!(
        (opened.handle.clone(), opened.attr.size),
        (real.clone(), 22)
    );
    assert_eq!(server.held().companions, 0);

    // WRITE reaches the Workspace file.
    c.write(&real, 0, b"COMMITTED").unwrap();
    assert_eq!(c.read_all(&real, 4096), b"COMMITTED AppleDouble\n");

    // A new companion of a file with no real `._` name behaves as before:
    // export memory, never listed, never committed.
    let companion = c.touch(&root, b"._plain.txt", 0o644).unwrap();
    assert!(companion.attr.fileid >= COMPANIONS);
    c.write(&companion.handle, 0, b"xattr").unwrap();
    let (found, _) = c.lookup(&root, b"._plain.txt").unwrap();
    assert_eq!(found, companion.handle);
    assert_eq!(server.held().companions, 1);
    let (entries, _) = c.list(&root, 64 * 1024, true);
    assert!(!entries.iter().any(|entry| entry.name == b"._plain.txt"));
    // A new `._` name is still kept out of the Workspace: a file is not
    // renamed onto one the view lacks, but replaces one the view holds.
    let refused = c.rename(&root, b"plain.txt", &root, b"._new").map(|_| ());
    assert_eq!(refused, Err(ACCES));
    c.rename(&root, b"other", &d, b"._bar").unwrap();
    let (replaced, _) = c.lookup(&d, b"._bar").unwrap();
    assert_eq!(c.read_all(&replaced, 4096), b"replacement\n");
    // REMOVE of a name the view holds removes the Workspace file.
    c.remove(&d, b"._bar").unwrap();
    assert_eq!(c.lookup(&d, b"._bar").map(|_| ()), Err(NOENT));

    // The Commit holds the edited `._foo` and the removal, not the companion.
    server.close_files();
    let done = rig.commit();
    assert!(done.history.is_some(), "{:?}", done.history);
    let view = rig.view([79; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut v = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = v.mnt("/");
    let (handle, _) = v.lookup(&view_root, b"._foo").unwrap();
    assert_eq!(v.read_all(&handle, 4096), b"COMMITTED AppleDouble\n");
    assert_eq!(v.lookup(&view_root, b"._plain.txt").map(|_| ()), Err(NOENT));
    let (view_d, _) = v.lookup(&view_root, b"d").unwrap();
    assert_eq!(v.lookup(&view_d, b"._bar").map(|_| ()), Err(NOENT));
    drop(v);
    let _released = viewer.stop();
    rig.close_view(view);
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    drop(fs::remove_dir_all(&base));
}
