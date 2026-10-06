//! Qualification of the public Session::from_fd timestamp parser/reply boundary.
//! This is an external protocol fixture, never a product proxy or workaround.
use fuser::{
    BsdFileFlags, Config, FileAttr, FileHandle, FileType, Filesystem, INodeNo, ReplyAttr, Request,
    Session, SessionACL, TimeOrNow,
};
use std::{
    io,
    net::Shutdown,
    os::fd::OwnedFd,
    os::unix::net::UnixDatagram,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Probe(Arc<Mutex<Vec<SystemTime>>>);
impl Filesystem for Probe {
    fn setattr(
        &self,
        _: &Request,
        ino: INodeNo,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u64>,
        _: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _: Option<SystemTime>,
        _: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let Some(TimeOrNow::SpecificTime(time)) = mtime else {
            panic!("fixture mtime")
        };
        self.0.lock().unwrap().push(time);
        reply.attr(
            &Duration::ZERO,
            &FileAttr {
                ino,
                size: 0,
                blocks: 0,
                atime: UNIX_EPOCH,
                mtime: time,
                ctime: UNIX_EPOCH,
                crtime: UNIX_EPOCH,
                kind: FileType::RegularFile,
                perm: 0o644,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                flags: 0,
                blksize: 4096,
            },
        );
    }
}
fn u32_at(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}
fn u64_at(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_ne_bytes());
}
fn frame(op: u32, unique: u64, body: usize) -> Vec<u8> {
    let mut b = vec![0; 40 + body];
    let len = b.len() as u32;
    u32_at(&mut b, 0, len);
    u32_at(&mut b, 4, op);
    u64_at(&mut b, 8, unique);
    u64_at(&mut b, 16, 2);
    b
}
fn response(socket: &UnixDatagram, unique: u64) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0; 512];
    let count = socket.recv(&mut bytes)?;
    bytes.truncate(count);
    assert!(count >= 16);
    assert_eq!(u64::from_ne_bytes(bytes[8..16].try_into().unwrap()), unique);
    assert_eq!(i32::from_ne_bytes(bytes[4..8].try_into().unwrap()), 0);
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, server) = UnixDatagram::pair()?;
    let stop = server.try_clone()?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?;
    client.set_write_timeout(Some(Duration::from_secs(2)))?;
    server.set_read_timeout(Some(Duration::from_secs(2)))?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let fs = Probe(seen.clone());
    let worker = thread::spawn(move || {
        Session::from_fd(
            fs,
            OwnedFd::from(server),
            SessionACL::All,
            Config::default(),
        )?
        .run()
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), Box<dyn std::error::Error>> {
            let mut init = frame(26, 1, 64);
            u32_at(&mut init, 40, 7);
            u32_at(&mut init, 44, 38);
            client.send(&init)?;
            response(&client, 1)?;
            for (index, (seconds, nanos)) in [
                (-2_i64, 800_000_000_u32),
                (i64::MIN, 0),
                (i64::MIN, 200_000_000),
                (i64::MIN + 1, 200_000_000),
                (i64::MAX, 999_999_999),
            ]
            .into_iter()
            .enumerate()
            {
                let unique = 2 + index as u64;
                let mut req = frame(4, unique, 88);
                u32_at(&mut req, 40, 1 << 5);
                u64_at(&mut req, 80, seconds as u64);
                u32_at(&mut req, 100, nanos);
                client.send(&req)?;
                let reply = response(&client, unique)?;
                let expected = if seconds < 0 {
                    UNIX_EPOCH
                        .checked_sub(Duration::new(seconds.unsigned_abs(), 0))
                        .unwrap()
                        .checked_add(Duration::new(0, nanos))
                        .unwrap()
                } else {
                    UNIX_EPOCH
                        .checked_add(Duration::new(seconds as u64, nanos))
                        .unwrap()
                };
                let received = *seen.lock().unwrap().last().unwrap();
                let returned_seconds = i64::from_ne_bytes(reply[64..72].try_into().unwrap());
                let returned_nanos = u32::from_ne_bytes(reply[84..88].try_into().unwrap());
                println!("REQUEST_TIME seconds={seconds} nanos={nanos} received={received:?} returned=({returned_seconds},{returned_nanos})");
                assert_eq!(received, expected);
                assert_eq!((returned_seconds, returned_nanos), (seconds, nanos));
            }
            client.send(&frame(38, 99, 0))?;
            response(&client, 99)?;
            Ok(())
        },
    ));
    // Always shut down this owned endpoint, including after a client-side error.
    stop.shutdown(Shutdown::Both)?;
    worker.join().map_err(|_| "session thread panic")??;
    result.map_err(|_| "client fixture panic")??;
    println!("Public Session parser and reply timestamp cases PASS; no kernel/native-mount substitution claimed");
    Ok(())
}
