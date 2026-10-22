//! Reference collection (A3, plan A): an export configured with a small
//! reference bound walks a tree far larger than the bound, repeatedly, and
//! keeps both its own holdings and the engine's kernel entries bounded.
//! Handles of collected serials keep working (resolved again by their name);
//! a handle whose name now reaches another inode is stale; the product
//! Commit afterwards holds exactly what the client made.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
use client::{Client, OK, STALE};
use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

const UID: u32 = 501;
const GID: u32 = 20;
const BOUND: usize = 64;
const DIRECTORIES: usize = 8;
const FILES: usize = 120;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-c{}-{label}", std::process::id()))
}
fn content(directory: usize, file: usize) -> Vec<u8> {
    format!("d{directory}/f{file}\n")
        .repeat(file % 7 + 1)
        .into_bytes()
}
fn kernel_entries(rig: &rig::Rig) -> u64 {
    let operation = rig.bound.operation().unwrap();
    operation.overlay().resources().unwrap().open.kernel_entries
}

#[test]
fn references_stay_bounded_and_collected_handles_resolve() {
    let source = temp("src");
    let work = temp("work");
    drop(fs::remove_dir_all(&source));
    drop(fs::remove_dir_all(&work));
    for directory in 0..DIRECTORIES {
        let path = source.join(format!("d{directory}/inner"));
        fs::create_dir_all(&path).unwrap();
        for file in 0..FILES {
            fs::write(
                source.join(format!("d{directory}/f{file}")),
                content(directory, file),
            )
            .unwrap();
        }
        fs::write(path.join("leaf"), b"leaf\n").unwrap();
    }
    let mut rig = rig::Rig::new(&source, &work, Duration::from_secs(60));
    rig.lookups_held = BOUND;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut client = Client::connect(server.local_addr(), UID, GID);
    let root = client.mnt("/");
    let total = DIRECTORIES * (FILES + 3);
    // The bound is soft: the most recently used serials, the directories a
    // held serial was reached through and serials holding a descriptor are
    // never collected.
    let ceiling = BOUND + layerfs_nfs::kernel::RECENT_KEPT as usize + DIRECTORIES * 2 + 8;
    let mut first: Vec<(Vec<u8>, u64, usize, usize)> = Vec::new();
    let mut peak_held = 0;
    let mut peak_engine = 0;
    for round in 0..6 {
        for directory in 0..DIRECTORIES {
            let (dir, _) = client
                .lookup(&root, format!("d{directory}").as_bytes())
                .unwrap();
            for file in 0..FILES {
                let (handle, attr) = client.lookup(&dir, format!("f{file}").as_bytes()).unwrap();
                if round == 0 && file % 15 == 0 {
                    first.push((handle, attr.fileid, directory, file));
                }
            }
            let (inner, _) = client.lookup(&dir, b"inner").unwrap();
            client.lookup(&inner, b"leaf").unwrap();
            // Churn: a file made and removed in every directory, every round.
            let made = format!("tmp{round}");
            client.touch(&dir, made.as_bytes(), 0o644).unwrap();
            client.remove(&dir, made.as_bytes()).unwrap();
            peak_held = peak_held.max(server.held().lookups);
            peak_engine = peak_engine.max(kernel_entries(&rig));
        }
        println!(
            "ROUND {round} held={:?} engine_kernel_entries={}",
            server.held(),
            kernel_entries(&rig)
        );
    }
    assert!(peak_held <= ceiling, "export held {peak_held} > {ceiling}");
    // The engine counts the export's references (plus the root's).
    assert!(
        peak_engine <= ceiling as u64 + 2,
        "engine kernel entries {peak_engine} > {ceiling}"
    );
    assert!(
        server.held().collected as usize >= total * 5,
        "{:?}",
        server.held()
    );
    // Every early handle was collected long ago; each resolves to its file.
    for (handle, fileid, directory, file) in &first {
        let attr = client.getattr(handle).unwrap();
        assert_eq!(attr.fileid, *fileid);
        assert_eq!(client.read_all(handle, 4096), content(*directory, *file));
    }
    let resolved = server.held().resolved;
    assert!(resolved >= first.len() as u64, "{:?}", server.held());
    // A collected handle whose name was replaced by another file is stale.
    let (victim, victim_fileid, directory, file) = first[1].clone();
    for _ in 0..3 {
        // collect it again
        for other in 0..FILES {
            let (dir, _) = client.lookup(&root, b"d7").unwrap();
            client.lookup(&dir, format!("f{other}").as_bytes()).unwrap();
        }
    }
    let (dir, _) = client
        .lookup(&root, format!("d{directory}").as_bytes())
        .unwrap();
    let replacement = client.touch(&dir, b"replacement", 0o644).unwrap();
    client
        .rename(&dir, b"replacement", &dir, format!("f{file}").as_bytes())
        .unwrap();
    assert_eq!(client.getattr(&victim).map(|attr| attr.fileid), Err(STALE));
    let (now, attr) = client.lookup(&dir, format!("f{file}").as_bytes()).unwrap();
    assert_ne!(attr.fileid, victim_fileid);
    assert_eq!(now, replacement.handle);
    // `..` of a directory whose parent was collected still answers.
    let (d3, _) = client.lookup(&root, b"d3").unwrap();
    let (inner, _) = client.lookup(&d3, b"inner").unwrap();
    for other in 0..FILES {
        let (dir, _) = client.lookup(&root, b"d6").unwrap();
        client.lookup(&dir, format!("f{other}").as_bytes()).unwrap();
    }
    let (up, up_attr) = client.lookup(&inner, b"..").unwrap();
    assert_eq!(up_attr.fileid, client.getattr(&d3).unwrap().fileid);
    assert_eq!(client.getattr(&up).map(|_| OK), Ok(OK));
    // The Commit holds what the client made.
    let closed = server.close_files();
    let done = rig.commit();
    assert!(done.history.is_some());
    println!("COMMITTED closed={closed} held={:?}", server.held());
    let view = rig.view([93; 32]);
    let viewer = rig.serve_view(TcpListener::bind("127.0.0.1:0").unwrap(), &view, UID, GID);
    let mut reader = Client::connect(viewer.local_addr(), UID, GID);
    let view_root = reader.mnt("/");
    let (replaced, _) = reader
        .walk_to(&view_root, &format!("d{directory}/f{file}"))
        .unwrap();
    assert!(reader.read_all(&replaced, 4096).is_empty());
    let (kept, _) = reader.walk_to(&view_root, "d5/f119").unwrap();
    assert_eq!(reader.read_all(&kept, 4096), content(5, 119));
    assert_eq!(
        reader.lookup(&view_root, b"tmp0").map(|_| ()),
        Err(client::NOENT)
    );
    drop(reader);
    viewer.stop();
    rig.close_view(view);
    drop(client);
    let released = server.stop();
    println!("RELEASED {released:?}");
    assert!(released.lookups <= ceiling);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(&source).unwrap();
}
