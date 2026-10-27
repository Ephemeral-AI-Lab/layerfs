//! Cache refresh after an install (B): the inodes the daemon names are
//! marked and their paths returned; a mode-only SETATTR on a marked inode is
//! answered with its current attributes and changes nothing, once; every
//! other SETATTR still changes the inode; the Commit holds only real changes.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use client::{Client, Set};
use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const UID: u32 = 501;
const GID: u32 = 20;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-r{}-{label}", std::process::id()))
}

#[test]
fn a_refresh_chmod_changes_nothing_and_other_setattrs_still_do() {
    let source = temp("src");
    let work = temp("work");
    drop(fs::remove_dir_all(&source));
    drop(fs::remove_dir_all(&work));
    fs::create_dir_all(source.join("a/b")).unwrap();
    fs::write(source.join("a/b/file"), b"inner\n").unwrap();
    fs::write(source.join("top.txt"), b"top\n").unwrap();
    for (path, mode) in [("a/b/file", 0o644), ("top.txt", 0o644), ("a", 0o755)] {
        fs::set_permissions(source.join(path), fs::Permissions::from_mode(mode)).unwrap();
    }
    let rig = rig::Rig::new(&source, &work, Duration::from_secs(60));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let (file, file_attr) = client.walk_to(&root, "a/b/file").unwrap();
    let (a, a_attr) = client.lookup(&root, b"a").unwrap();
    let (top, top_attr) = client.lookup(&root, b"top.txt").unwrap();
    let root_id = client.getattr(&root).unwrap().fileid;
    // Paths of the serials the client knows, the root's empty; an unknown serial is left out.
    let paths = server.refresh([
        file_attr.fileid,
        a_attr.fileid,
        root_id,
        u64::MAX - 7,
        top_attr.fileid,
    ]);
    assert_eq!(
        paths,
        vec![
            PathBuf::from("a/b/file"),
            PathBuf::from("a"),
            PathBuf::new(),
            PathBuf::from("top.txt"),
        ]
    );
    let mode = |set: Result<client::Wcc, (u32, client::Wcc)>| {
        set.map(|wcc| wcc.after.map(|after| after.mode & 0o7777))
            .map_err(|(status, _)| status)
    };
    let chmod = |mode: u32| Set {
        mode: Some(mode),
        ..Set::default()
    };
    // A marked inode: the mode-only SETATTR is answered, nothing changes.
    assert_eq!(
        mode(client.setattr(&file, chmod(0o600), None)),
        Ok(Some(0o644))
    );
    assert_eq!(
        mode(client.setattr(&a, chmod(0o700), None)),
        Ok(Some(0o755))
    );
    assert_eq!(client.getattr(&file).unwrap().mode & 0o7777, 0o644);
    // The mark is taken once: the next chmod is real.
    assert_eq!(
        mode(client.setattr(&file, chmod(0o600), None)),
        Ok(Some(0o600))
    );
    // A SETATTR that is not mode-only is never taken for a refresh.
    let truncate = Set {
        mode: Some(0o640),
        size: Some(0),
        ..Set::default()
    };
    let after = client.setattr(&top, truncate, None).unwrap().after.unwrap();
    assert_eq!((after.mode & 0o7777, after.size), (0o640, 0));
    // The Commit holds the real changes only.
    server.close_files();
    assert!(rig.commit().history.is_some());
    let view = rig.view([71; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut reader = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = reader.mnt("/");
    let at = |reader: &mut Client, path: &str| reader.walk_to(&view_root, path).unwrap().1;
    assert_eq!(at(&mut reader, "a/b/file").mode & 0o7777, 0o600);
    assert_eq!(at(&mut reader, "a").mode & 0o7777, 0o755);
    let top_view = at(&mut reader, "top.txt");
    assert_eq!((top_view.mode & 0o7777, top_view.size), (0o640, 0));
    drop(reader);
    let _released = viewer.stop();
    rig.close_view(view);
    drop(client);
    let served = server.stop();
    let (_, outstanding) = rig.stop(served);
    assert_eq!(outstanding, 0);
    for path in [&source, &work] {
        drop(fs::remove_dir_all(Path::new(path)));
    }
}
