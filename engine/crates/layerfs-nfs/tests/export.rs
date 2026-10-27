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
