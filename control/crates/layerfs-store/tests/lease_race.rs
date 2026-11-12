//! A holder can release its lease after stale inspection and before removal.
#![cfg(unix)]
use layerfs_home::home::{self, paths};
use layerfs_store::{lease, record};
use nix::sys::stat::Mode;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn stale_lease_released_after_inspection_is_already_gone() {
    let home = std::env::temp_dir().join(format!("layerfs-lease-race-{}", std::process::id()));
    let directory = paths::lease_dir(&home, "shop");
    home::private_dir(&directory).unwrap();
    let path = directory.join("4294967295");
    nix::unistd::mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
    let (send, recv) = mpsc::channel();
    let root = home.clone();
    let reader = std::thread::spawn(move || {
        let _lock = record::lock(&root, "shop").unwrap();
        send.send(lease::holders(&root, "shop")).unwrap();
    });
    // Opening the FIFO for writing succeeds only after holders opened its read
    // descriptor. Unlink before supplying EOF: its snapshot then names a lease
    // already released by its holder, without timing guesses or product hooks.
    let until = Instant::now() + Duration::from_secs(5);
    let mut writer = loop {
        match OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => break file,
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                assert!(Instant::now() < until, "lease inspection never opened FIFO");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("open lease FIFO: {error}"),
        }
    };
    std::fs::remove_file(&path).unwrap();
    writer
        .write_all(b"0 0 00000000000000000000000000000000\n")
        .unwrap();
    drop(writer);
    let result = recv.recv_timeout(Duration::from_secs(5)).unwrap();
    reader.join().unwrap();
    assert!(result.unwrap().is_empty());
    assert!(
        !home.join("log/leases.jsonl").exists(),
        "no removal was performed to log"
    );
    std::fs::remove_dir_all(home).unwrap();
}
