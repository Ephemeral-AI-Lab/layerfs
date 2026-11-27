//! Cache refresh after an install (B): the daemon gets the paths of the
//! inodes it names, and marks one inode with the mode its refresh chmod asks
//! right before sending it. A mode-only SETATTR asking the marked mode is
//! answered with the current attributes and changes nothing, once; a client
//! SETATTR asking another mode is applied even while the inode is marked,
//! and the refresh chmod that follows does not undo it; every other SETATTR
//! still changes the inode; the Commit holds only real changes.
use crate::client;
use crate::rig;
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
fn a_refresh_chmod_changes_nothing_and_a_user_chmod_still_applies() {
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
    // Paths of the serials the client knows, the root's empty, each with its
    // serial; an unknown serial is left out. Nothing is marked yet.
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
            (file_attr.fileid, PathBuf::from("a/b/file")),
            (a_attr.fileid, PathBuf::from("a")),
            (root_id, PathBuf::new()),
            (top_attr.fileid, PathBuf::from("top.txt")),
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
    let refresher = server.refresher();
    // An unmarked inode: a chmod to its own mode is an ordinary SETATTR.
    assert_eq!(
        mode(client.setattr(&top, chmod(0o644), None)),
        Ok(Some(0o644))
    );
    // The refresh chmod asks the mode the client still shows (0o600, from
    // before an install that made it 0o644): answered, nothing reverts.
    assert!(refresher.arm(file_attr.fileid, 0o600));
    assert_eq!(
        mode(client.setattr(&file, chmod(0o600), None)),
        Ok(Some(0o644))
    );
    assert_eq!(client.getattr(&file).unwrap().mode & 0o7777, 0o644);
    // The mark is taken once: the next chmod is real.
    assert_eq!(
        mode(client.setattr(&file, chmod(0o600), None)),
        Ok(Some(0o600))
    );
    // A user chmod inside the mark window asks another mode: it is applied,
    // and the refresh chmod that follows (asking the marked mode) is answered
    // without a change, so it never undoes the user's newer mode.
    assert!(refresher.arm(a_attr.fileid, 0o755));
    assert_eq!(
        mode(client.setattr(&a, chmod(0o700), None)),
        Ok(Some(0o700))
    );
    assert_eq!(
        mode(client.setattr(&a, chmod(0o755), None)),
        Ok(Some(0o700))
    );
    assert_eq!(client.getattr(&a).unwrap().mode & 0o7777, 0o700);
    // A SETATTR that is not mode-only is never taken for a refresh.
    assert!(refresher.arm(top_attr.fileid, 0o640));
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
    assert_eq!(at(&mut reader, "a").mode & 0o7777, 0o700);
    let top_view = at(&mut reader, "top.txt");
    assert_eq!((top_view.mode & 0o7777, top_view.size), (0o640, 0));
    drop(reader);
    let _released = viewer.stop();
    rig.close_view(view);
    drop(client);
    let served = server.stop();
    // A stopped export takes no mark: the refresh thread stops.
    assert!(!refresher.arm(file_attr.fileid, 0o600));
    let (_, outstanding) = rig.stop(served);
    assert_eq!(outstanding, 0);
    for path in [&source, &work] {
        drop(fs::remove_dir_all(Path::new(path)));
    }
}
