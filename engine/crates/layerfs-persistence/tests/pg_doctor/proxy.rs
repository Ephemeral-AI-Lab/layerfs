//! Independent wire-byte oracle for malformed-payload extraction.
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
pub struct Proxy {
    pub port: u16,
    pub received: Arc<AtomicUsize>,
    done: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Proxy {
    pub fn new(port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let own_port = listener.local_addr().unwrap().port();
        let done = Arc::new(AtomicBool::new(false));
        let received = Arc::new(AtomicUsize::new(0));
        let ended = done.clone();
        let bytes = received.clone();
        let thread = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(5);
            let client = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if ended.load(Ordering::Relaxed) || Instant::now() >= until {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(e) => panic!("owned proxy accept: {e}"),
                }
            };
            client.set_nonblocking(false).unwrap();
            let server =
                TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2))
                    .unwrap();
            for socket in [&client, &server] {
                socket
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
            }
            let source = client.try_clone().unwrap();
            let target = server.try_clone().unwrap();
            let end = ended.clone();
            let forward = std::thread::spawn(move || copy(source, target, end, None));
            copy(server, client, ended, Some(bytes));
            forward.join().unwrap();
        });
        Self {
            port: own_port,
            received,
            done,
            thread: Some(thread),
        }
    }
    pub fn bytes(&self) -> usize {
        self.received.load(Ordering::Relaxed)
    }
}
fn copy(
    mut source: TcpStream,
    mut target: TcpStream,
    done: Arc<AtomicBool>,
    count: Option<Arc<AtomicUsize>>,
) {
    let mut buf = [0; 16384];
    while !done.load(Ordering::Relaxed) {
        match source.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(count) = &count {
                    count.fetch_add(n, Ordering::Relaxed);
                }
                if target.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    done.store(true, Ordering::Relaxed);
    let _ = source.shutdown(Shutdown::Both);
    let _ = target.shutdown(Shutdown::Both);
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
