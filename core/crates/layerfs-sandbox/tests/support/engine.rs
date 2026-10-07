//! External bounded local Engine peer, through ordinary public Unix endpoint configuration.
use layerfs_sandbox::backend::docker::Docker;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Peer {
    directory: PathBuf,
    worker: Option<JoinHandle<Vec<Vec<u8>>>>,
}
impl Peer {
    pub fn new(responses: Vec<Vec<u8>>, split: usize) -> (Docker, Self) {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-engine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(error) => panic!("bounded peer admission: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = Vec::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    assert!(!line.is_empty());
                    request.extend_from_slice(line.as_bytes());
                    assert!(request.len() < 32768);
                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                        length = value.trim().parse().unwrap();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                assert!(length < 4 * 1024 * 1024);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                request.extend_from_slice(&body);
                requests.push(request);
                let mut sent = 0usize;
                'response: for bytes in response.chunks(split.max(1)) {
                    let mut position = 0;
                    while position < bytes.len() {
                        match stream.write(&bytes[position..]) {
                            Ok(0) => {
                                eprintln!("EXTERNAL_PEER_WRITE_ZERO sent={sent}");
                                break 'response;
                            }
                            Ok(n) => {
                                position += n;
                                sent += n;
                            }
                            Err(error) => {
                                eprintln!("EXTERNAL_PEER_WRITE_ERROR sent={sent} expected={} cause={error:?}",response.len());
                                break 'response;
                            }
                        }
                    }
                }
            }
            requests
        });
        (
            Docker::new(path, Duration::from_secs(2)).unwrap(),
            Self {
                directory,
                worker: Some(worker),
            },
        )
    }
    pub fn finish(mut self) -> Vec<Vec<u8>> {
        let result = self.worker.take().unwrap().join().unwrap();
        fs::remove_dir_all(&self.directory).unwrap();
        result
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}
pub fn fixed(status: u16, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 {status} test\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
pub fn upgrade(payload: &[u8]) -> Vec<u8> {
    let mut bytes =
        b"HTTP/1.1 101 UPGRADED\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n".to_vec();
    bytes.extend_from_slice(payload);
    bytes
}
pub fn frame(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![tag, 0, 0, 0];
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
