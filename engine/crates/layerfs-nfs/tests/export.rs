//! A committed Branch served read-only over loopback NFSv3: every path,
//! byte, link target, mode and identity of the source tree through the
//! protocol alone, the synthetic kernel's reference discipline, cookies,
//! refusals and stale handles across engine mounts.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/tree.rs"]
mod tree;
use client::{Attr, Client, BADHANDLE, BAD_COOKIE, NOENT, ROFS, STALE};
use layerfs_nfs::xdr::Writer;
use std::{
    collections::BTreeSet,
    fs,
    net::TcpListener,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tree::{oracle, walk, Node};

const UID: u32 = 501;
const GID: u32 = 20;
const MANY: usize = 700;
const BIG: usize = 300_001;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-{}-{label}", std::process::id()))
}
fn source(root: &Path) {
    fs::create_dir_all(root.join("src/nested/deep")).unwrap();
    fs::create_dir_all(root.join("many")).unwrap();
    fs::create_dir_all(root.join("emptydir")).unwrap();
    fs::write(root.join("README.md"), b"# fixture\nhello nfs\n").unwrap();
    fs::write(
        root.join("big.bin"),
        (0..BIG).map(|n| (n % 251) as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(root.join("empty"), b"").unwrap();
    fs::write(root.join("src/lib.rs"), b"pub fn answer() -> u32 { 42 }\n").unwrap();
    fs::write(root.join("src/nested/deep/file.txt"), b"deep\n").unwrap();
    fs::hard_link(root.join("src/lib.rs"), root.join("src/lib-alias.rs")).unwrap();
    std::os::unix::fs::symlink("src/lib.rs", root.join("link")).unwrap();
    std::os::unix::fs::symlink("../nowhere", root.join("dangling")).unwrap();
    fs::write(root.join("unicod\u{e9}.txt"), "caf\u{e9}\n").unwrap();
    fs::write(root.join("run.sh"), b"#!/bin/sh\necho run\n").unwrap();
    fs::set_permissions(root.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    for n in 0..MANY {
        fs::write(root.join(format!("many/f-{n:04}")), format!("{n}\n")).unwrap();
    }
}
#[test]
fn a_committed_branch_reads_completely_over_nfs() {
    let base = temp("complete");
    let tree = base.join("tree");
    source(&tree);
    let expected = oracle(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    assert_eq!(rig.entries as usize, expected.len());
    let server = rig.serve(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID);
    let mut client = Client::connect(server.local_addr(), UID, GID);

    let (paths, bytes, identities) = walk(&mut client, &expected, 4096, (UID, GID));
    assert_eq!(paths, expected.len());
    assert!(bytes > BIG);
    // One inode behind both names of the hard link, two names for it, and
    // distinct inodes everywhere else.
    let alias = expected.iter().find_map(|(path, node)| match node {
        Node::File { ino, .. } if path == Path::new("src/lib.rs") => Some(*ino),
        _ => None,
    });
    let fileids = &identities[&alias.unwrap()];
    assert_eq!(fileids.len(), 1, "both names show one fileid");
    let lib = client.mnt("/");
    let (src, _) = client.lookup(&lib, b"src").unwrap();
    let (_, lib_attr) = client.lookup(&src, b"lib.rs").unwrap();
    assert_eq!(lib_attr.nlink, 2);
    let all: BTreeSet<u64> = identities.values().flatten().copied().collect();
    assert_eq!(
        all.len(),
        identities.len(),
        "distinct inodes, distinct fileids"
    );

    // Re-walking takes no further engine references: one per handed-out serial.
    // Every path but the root holds one reference; the hard link's two names
    // share one serial.
    let serials = expected.len() - 2;
    let held = server.held();
    assert_eq!(held.lookups, serials, "one reference per non-root serial");
    walk(&mut client, &expected, 64 * 1024, (UID, GID));
    assert_eq!(server.held().lookups, held.lookups);

    let released = server.stop();
    assert_eq!(released.lookups, serials);
    assert!(released.files > 0 && released.directories > 0);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0, "every Store reader was given back");
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn readdirplus_lists_a_directory_with_attributes_and_handles() {
    use std::os::unix::ffi::OsStrExt;
    let base = temp("plus");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, true);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let (src, src_attr) = client.lookup(&root, b"src").unwrap();
    let held = server.held().lookups;

    // One reply lists the whole directory: the window that lists its last
    // name is its end, so no further request is spent on an empty one.
    let calls = client.calls;
    let (entries, _, eof, directory) = client
        .readdir_with_directory(&src, 0, 0, 4096, true)
        .unwrap();
    assert_eq!(client.calls - calls, 1, "one READDIRPLUS for the directory");
    assert!(eof, "the reply that lists the last name is the end");
    assert_eq!(
        directory,
        Some(src_attr),
        "dir_attributes of the listed directory"
    );
    let names: BTreeSet<Vec<u8>> = entries.iter().map(|entry| entry.name.clone()).collect();
    assert_eq!(
        names,
        [
            b".".to_vec(),
            b"..".to_vec(),
            b"lib.rs".to_vec(),
            b"lib-alias.rs".to_vec(),
            b"nested".to_vec()
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
    let mut read = 0;
    for entry in &entries {
        // The dots name objects the client already holds; a real name says
        // what it is and hands out the handle to read it without a LOOKUP.
        if entry.name == b"." || entry.name == b".." {
            assert_eq!((&entry.attr, &entry.handle), (&None, &None), "dots");
            continue;
        }
        let attr = entry.attr.clone().expect("READDIRPLUS attributes");
        let handle = entry.handle.clone().expect("READDIRPLUS handle");
        assert_eq!(handle.len(), 25, "a full NFSv3 handle");
        assert_eq!(attr.fileid, entry.fileid, "the same identity");
        // The same facts a LOOKUP reports, and the handle answers alone.
        let (found, looked_up) = client.lookup(&src, &entry.name).unwrap();
        assert_eq!(found, handle, "LOOKUP hands out the same handle");
        assert_eq!(looked_up, attr, "LOOKUP reports the same attributes");
        assert_eq!(client.getattr(&handle).unwrap(), attr, "the handle answers");
        if attr.kind == 1 {
            let bytes = client.read_all(&handle, 128 * 1024);
            let expected = fs::read(
                tree.join("src")
                    .join(std::ffi::OsStr::from_bytes(&entry.name)),
            )
            .unwrap();
            assert_eq!(bytes, expected, "bytes from the listed handle");
            read += bytes.len();
        }
    }
    assert!(read > 0, "the listing's handles read the files");
    // READDIR still answers the same directory in one reply.
    let calls = client.calls;
    let (entries, _, eof) = client.readdir(&src, 0, 0, 4096, false).unwrap();
    assert_eq!(client.calls - calls, 1, "one READDIR for the directory");
    assert!(eof);
    assert_eq!(entries.len(), names.len(), "READDIR lists the same names");
    // Every serial that came with a handle holds exactly one reference, and
    // two names of one inode share it.
    let named: BTreeSet<u64> = entries
        .iter()
        .filter(|entry| entry.name != b"." && entry.name != b"..")
        .map(|entry| entry.fileid)
        .collect();
    assert_eq!(
        server.held().lookups,
        held + named.len(),
        "one reference per listed inode"
    );
    let serials = server.held().lookups;
    let released = server.stop();
    assert_eq!(released.lookups, serials, "every reference given back");
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0, "every Store reader was given back");
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn readdirplus_reports_a_local_directorys_own_rows() {
    let base = temp("local");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    // Names this export created live in the Workspace, not in its base: their
    // attributes come from the local inode row the listing already read.
    client.mkdir(&root, b"made", 0o755).unwrap();
    let (made, _) = client.lookup(&root, b"made").unwrap();
    let file = client.touch(&made, b"entry", 0o644).unwrap();
    client.write(&file.handle, 0, b"local row\n").unwrap();
    let (file_dot, file_attr) = client.lookup(&made, b"entry").unwrap();

    let (entries, _, eof, directory) = client
        .readdir_with_directory(&made, 0, 0, 4096, true)
        .unwrap();
    assert!(eof);
    let entry = entries
        .iter()
        .find(|entry| entry.name == b"entry")
        .expect("the created name");
    assert_eq!(entry.handle.clone(), Some(file_dot), "the same handle");
    assert_eq!(entry.attr.clone(), Some(file_attr), "the same attributes");
    let made_attr = client.getattr(&made).unwrap();
    assert_eq!(
        directory,
        Some(made_attr),
        "the created directory's own row"
    );
    // The reference the listing hands out is the one the name's removal
    // gives back, so it is a real engine reference and not a second one.
    let held = server.held().lookups;
    client.remove(&made, b"entry").unwrap();
    assert_eq!(
        server.held().lookups,
        held - 1,
        "the listed reference is given back"
    );
    let released = server.stop();
    assert_eq!(released.lookups, held - 1, "every reference given back");
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_large_directory_keeps_its_names_and_its_references() {
    let base = temp("large");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, true);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let (many, _) = client.lookup(&root, b"many").unwrap();
    let (entries, calls) = client.list(&many, 64 * 1024, true);
    assert!(calls > 1, "a budget spreads one directory over replies");
    let names: BTreeSet<Vec<u8>> = entries.iter().map(|entry| entry.name.clone()).collect();
    assert_eq!(names.len(), entries.len(), "no name twice");
    assert_eq!(names.len(), MANY + 2, "every name, once");
    let cookies: Vec<u64> = entries.iter().map(|entry| entry.cookie).collect();
    assert!(
        cookies.windows(2).all(|pair| pair[0] < pair[1]),
        "cookies advance"
    );
    // One reference per handed-out handle, none for a repeated reply.
    let held = server.held().lookups;
    assert_eq!(held, 1 + MANY, "the directory and one reference per entry");
    let (again, _) = client.list(&many, 64 * 1024, true);
    assert_eq!(again.len(), entries.len(), "the same names again");
    assert_eq!(
        server.held().lookups,
        held,
        "no second reference for a listed name"
    );
    let released = server.stop();
    assert_eq!(released.lookups, held, "every reference given back");
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn cookies_refusals_and_stale_handles() {
    let base = temp("protocol");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.serve(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let (many, _) = client.lookup(&root, b"many").unwrap();

    // A small budget spreads one enumeration over many replies; every name
    // appears exactly once, in both procedures.
    for plus in [false, true] {
        let (entries, calls) = client.list(&many, 1024, plus);
        let names: BTreeSet<Vec<u8>> = entries.iter().map(|entry| entry.name.clone()).collect();
        assert_eq!(names.len(), entries.len(), "no duplicate (plus={plus})");
        assert_eq!(entries.len(), MANY + 2, "plus={plus}");
        assert!(calls > 10, "{calls} calls (plus={plus})");
    }
    // A verifier the export never issued, and a budget too small for one entry.
    assert_eq!(
        client.readdir(&many, 5, 0xdead_beef, 4096, false),
        Err(BAD_COOKIE)
    );
    // Missing names, a foreign handle and every changing procedure.
    assert_eq!(client.lookup(&root, b"absent").map(|_| ()), Err(NOENT));
    // Both LOOKUP answers carry the directory's attributes.
    let root_attr = client.getattr(&root).unwrap();
    let (status, found, directory) = client.lookup_with_directory(&root, b"many");
    assert_eq!(
        (status, found.map(|(handle, _)| handle)),
        (0, Some(many.clone()))
    );
    assert_eq!(directory, Some(root_attr.clone()));
    let (status, found, directory) = client.lookup_with_directory(&root, b"absent");
    assert_eq!((status, found), (NOENT, None));
    assert_eq!(directory, Some(root_attr));
    // Not a handle this export issues: wrong version or length. A version-1
    // handle of another engine mount is stale (its key is gone); a forged
    // tag under this mount is refused (see protocol.rs).
    assert_eq!(client.getattr(&[2; 25]), Err(BADHANDLE));
    assert_eq!(client.getattr(&[1; 10]), Err(BADHANDLE));
    assert_eq!(client.getattr(&[1; 25]), Err(STALE));
    for procedure in [2, 7, 8, 9, 10, 11, 12, 13, 14, 15, 21] {
        let mut arguments = Writer::new();
        arguments.opaque(&root);
        assert_eq!(client.nfs(procedure, &arguments).0, ROFS, "{procedure}");
    }
    // A read-only export grants reads only, whatever the mode allows.
    let (readme, readme_attr): (Vec<u8>, Attr) = client.lookup(&root, b"README.md").unwrap();
    let host = fs::metadata(tree.join("README.md")).unwrap().mode() & 0o7777;
    assert_eq!(readme_attr.mode, host);
    assert_eq!(host & 0o444, 0o444, "the fixture is readable by everyone");
    assert_eq!(client.access(&readme, 0x3f), Ok(0x01));
    let mut other = Client::connect(server.local_addr(), 1234, 1234);
    assert_eq!(other.access(&readme, 0x3f), Ok(0x01), "other: r--");
    let (run, _) = client.lookup(&root, b"run.sh").unwrap();
    assert_eq!(client.access(&run, 0x3f), Ok(0x01 | 0x20));
    // Reads beyond the end, and windows that are not page aligned.
    let (big, big_attr) = client.lookup(&root, b"big.bin").unwrap();
    assert_eq!(
        client.read(&big, big_attr.size + 10, 4096),
        Ok((vec![], true))
    );
    let (window, eof) = client.read(&big, 1000, 70_000).unwrap();
    assert!(!eof);
    assert_eq!(
        window,
        (1000..71_000).map(|n| (n % 251) as u8).collect::<Vec<_>>()
    );

    // A restarted export is a new engine mount: earlier handles are stale.
    drop(client);
    drop(other);
    let released = server.stop();
    rig.remount();
    let server = rig.serve(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    assert_eq!(client.getattr(&readme), Err(STALE));
    assert_eq!(client.getattr(&root), Err(STALE));
    let fresh = client.mnt("/");
    assert!(client.getattr(&fresh).is_ok());
    drop(client);
    let released_again = server.stop();
    assert!(released.lookups > 0);
    let (_, outstanding) = rig.stop(released_again);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}
