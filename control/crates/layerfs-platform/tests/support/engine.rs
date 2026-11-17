//! A stand-in Docker Engine socket: a Unix listener that answers each
//! connection with one fixed reply, for as long as the returned guard lives.
#![allow(dead_code)]
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Engine {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Listens at `path` and answers every request with `reply`.
pub fn engine(path: &Path, reply: &'static str) -> Engine {
    let listener = UnixListener::bind(path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        // bounded: the guard's drop ends it, and it ends on its own after 30 s
        let until = std::time::Instant::now() + Duration::from_secs(30);
        while !stopping.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0u8; 256];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(reply.as_bytes());
                }
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    });
    Engine {
        path: path.to_owned(),
        stop,
        thread: Some(thread),
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
