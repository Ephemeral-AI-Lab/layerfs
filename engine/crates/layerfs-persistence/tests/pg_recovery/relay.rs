//! One plaintext session. Arm a single protocol boundary after Store open.
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

pub enum Cut {
    BeforeCommit,
    AfterDelete,
    PartialCommit,
    AfterUpdate(Box<dyn FnOnce() + Send>),
}

pub struct Relay {
    pub port: u16,
    cut: Arc<Mutex<Option<Cut>>>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Relay {
    pub fn new(url: &str) -> Self {
        let config: postgres::Config = url.parse().unwrap();
        let [postgres::config::Host::Tcp(host)] = config.get_hosts() else {
            panic!("expected a plaintext TCP fixture");
        };
        let upstream = (
            host.clone(),
            config.get_ports().first().copied().unwrap_or(5432),
        );
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let cut = Arc::new(Mutex::new(None));
        let action = Arc::clone(&cut);
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = closed.clone();
        // Subsequent connections (recovery and the lane holder) are transparent.
        thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { break };
                if stop.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let server = TcpStream::connect(&upstream).unwrap();
                let action = action.clone();
                thread::spawn(move || {
                    let _ = forward(client, server, action);
                });
            }
        });
        Self { port, cut, closed }
    }
    pub fn arm(&self, cut: Cut) {
        assert!(self.cut.lock().unwrap().replace(cut).is_none());
    }
    pub fn fired(&self) {
        assert!(
            self.cut.lock().unwrap().is_none(),
            "fault boundary was not reached"
        );
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn packet(reader: &mut TcpStream) -> io::Result<(u8, [u8; 4], Vec<u8>)> {
    let mut header = [0; 5];
    reader.read_exact(&mut header)?;
    let length: [u8; 4] = header[1..].try_into().unwrap();
    let len = u32::from_be_bytes(length)
        .checked_sub(4)
        .ok_or(io::ErrorKind::InvalidData)?;
    let mut body = vec![0; len as usize];
    reader.read_exact(&mut body)?;
    Ok((header[0], length, body))
}

fn forward(
    mut client: TcpStream,
    mut server: TcpStream,
    cut: Arc<Mutex<Option<Cut>>>,
) -> io::Result<()> {
    client.set_nodelay(true)?;
    server.set_nodelay(true)?;
    // The first (startup) message has a length but no type byte.
    let mut len = [0; 4];
    client.read_exact(&mut len)?;
    server.write_all(&len)?;
    io::copy(
        &mut (&mut client).take((u32::from_be_bytes(len) - 4).into()),
        &mut server,
    )?;
    let mut requests = client.try_clone()?;
    let mut destination = server.try_clone()?;
    let request_cut = cut.clone();
    thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            loop {
                let (tag, length, body) = packet(&mut requests)?;
                if tag == b'Q' && body.ends_with(b"COMMIT\0") {
                    let mut slot = request_cut.lock().unwrap();
                    if matches!(*slot, Some(Cut::BeforeCommit)) {
                        slot.take();
                        break;
                    }
                }
                destination.write_all(&[tag])?;
                destination.write_all(&length)?;
                destination.write_all(&body)?;
            }
            Ok(())
        })();
        let _ = destination.shutdown(Shutdown::Both);
        let _ = requests.shutdown(Shutdown::Both);
        result
    });
    let result = (|| -> io::Result<()> {
        loop {
            let (tag, length, body) = packet(&mut server)?;
            if tag == b'C' {
                let mut slot = cut.lock().unwrap();
                if matches!(*slot, Some(Cut::AfterDelete)) && body.starts_with(b"DELETE ") {
                    slot.take();
                    break;
                }
                if matches!(*slot, Some(Cut::PartialCommit)) && body == b"COMMIT\0" {
                    slot.take();
                    client.write_all(&[tag])?;
                    client.write_all(&length)?;
                    client.write_all(&body[..1])?;
                    break;
                }
                // Hold the completed write's reply until the lane is gone. The
                // main session still pins the row and has yet to send its fence.
                if body == b"UPDATE 1\0" && matches!(*slot, Some(Cut::AfterUpdate(_))) {
                    if let Some(Cut::AfterUpdate(action)) = slot.take() {
                        drop(slot);
                        action();
                    }
                }
            }
            client.write_all(&[tag])?;
            client.write_all(&length)?;
            client.write_all(&body)?;
        }
        Ok(())
    })();
    let _ = server.shutdown(Shutdown::Both);
    let _ = client.shutdown(Shutdown::Both);
    result
}
