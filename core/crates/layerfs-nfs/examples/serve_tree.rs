//! Serves a host tree, committed as a Branch, over loopback NFSv3.
//!
//! usage: serve_tree <source-dir> <work-dir> <port> [uid gid] [write]
//!
//! Prints `READY <address>` once the export accepts calls. With `write` the
//! export is writable, and whenever `<work-dir>.commit` appears the export
//! closes its implicit file descriptors, the Workspace is committed with the
//! product Commit (`COMMITTED …`), and a second, independent Workspace bound
//! to the committed Branch is served read-only on `<port>+1` (`VIEW …`);
//! the trigger file is then removed. It serves until `<work-dir>.stop`
//! exists; it then prints per-procedure call counts and time, gives back
//! every engine reference, revokes the engine mounts and removes
//! `<work-dir>`. Mount on Linux with, for example:
//! `mount -t nfs -o vers=3,proto=tcp,port=P,mountport=P,mountproto=tcp,nolock HOST:/ DIR`.
#[allow(dead_code)]
#[path = "../tests/support/rig.rs"]
mod rig;
use std::{net::TcpListener, path::PathBuf, time::Duration};

fn main() {
    let mut arguments: Vec<String> = std::env::args().collect();
    let writable = arguments.last().is_some_and(|last| last == "write");
    if writable {
        arguments.pop();
    }
    if arguments.len() != 4 && arguments.len() != 6 {
        eprintln!("usage: serve_tree <source-dir> <work-dir> <port> [uid gid] [write]");
        std::process::exit(2);
    }
    let source = PathBuf::from(&arguments[1]);
    let work = PathBuf::from(&arguments[2]);
    let port: u16 = arguments[3].parse().expect("port");
    let (uid, gid) = if arguments.len() == 6 {
        (arguments[4].parse().unwrap(), arguments[5].parse().unwrap())
    } else {
        (501, 20)
    };
    let stop = PathBuf::from(format!("{}.stop", work.display()));
    let commit = PathBuf::from(format!("{}.commit", work.display()));
    let started = std::time::Instant::now();
    let rig = rig::Rig::new(&source, &work, Duration::from_secs(600));
    println!(
        "INIT entries={} seconds={:.2}",
        rig.entries,
        started.elapsed().as_secs_f64()
    );
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    let server = rig.export(listener, uid, gid, !writable);
    println!("READY {} writable={writable}", server.local_addr());
    let mut views = Vec::new();
    while !stop.exists() {
        if writable && commit.exists() {
            let closed = server.close_files();
            let at = std::time::Instant::now();
            let done = rig.commit();
            println!(
                "COMMITTED closed_files={closed} ms={} history={:?} replaced={:?}",
                at.elapsed().as_millis(),
                done.history,
                done.replaced
            );
            drop(done);
            let mut authority = [80; 32];
            authority[0] = views.len() as u8;
            let view = rig.view(authority);
            let listener =
                TcpListener::bind(("127.0.0.1", port + 1 + views.len() as u16)).expect("bind");
            let viewer = rig.serve_view(listener, &view, uid, gid);
            println!("VIEW {}", viewer.local_addr());
            views.push((viewer, view));
            drop(std::fs::remove_file(&commit));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let mut total = (0, 0);
    for count in server.counters() {
        total.0 += count.calls;
        total.1 += count.nanoseconds;
        println!(
            "PROC {:<12} calls={:<8} total_ms={:<10.1} avg_us={:.1}",
            count.procedure,
            count.calls,
            count.nanoseconds as f64 / 1e6,
            count.nanoseconds as f64 / 1e3 / count.calls as f64
        );
    }
    println!(
        "TOTAL calls={} server_ms={:.1}",
        total.0,
        total.1 as f64 / 1e6
    );
    for (viewer, view) in views {
        viewer.stop();
        rig.close_view(view);
    }
    let released = server.stop();
    println!("RELEASED {released:?}");
    let (_, outstanding) = rig.stop(released);
    println!("STOPPED outstanding_readers={outstanding}");
    drop(std::fs::remove_file(stop));
}
