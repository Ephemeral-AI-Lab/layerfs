//! External transport-only workload for the committed issue192 tdx1 specification.
//! No Store, service handler, C1/C2 algorithm, spool or product fallback lives here.
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, connect, Connection, Peer},
        pipe::key,
        protocol::{Frame, Kind},
    },
    contract::FRAME_BYTES,
};
use std::{
    error::Error,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{mpsc, Arc, Barrier},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const BYTES: u64 = 1024 * 1024 * 1024;
const SEED: u64 = 0x1922_0260_9200_0000;
enum Wire {
    Noise(Box<Connection>),
    Tcp(TcpStream),
}
impl Wire {
    fn deadline(&mut self, seconds: u64) -> Result<()> {
        match self {
            Self::Noise(c) => {
                let end = Instant::now() + Duration::from_secs(seconds);
                c.send.deadline(end);
                c.receive.deadline(end);
            }
            Self::Tcp(s) => {
                s.set_nodelay(true)?;
                s.set_read_timeout(Some(Duration::from_secs(5)))?;
                s.set_write_timeout(Some(Duration::from_secs(5)))?;
            }
        }
        Ok(())
    }
    fn control(&mut self, kind: Kind, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Noise(c) => c.send.write(&Frame {
                kind,
                id: 1,
                bytes: bytes.to_vec(),
            })?,
            Self::Tcp(s) => {
                s.write_all(&[kind as u8])?;
                s.write_all(bytes)?;
            }
        }
        Ok(())
    }
    fn expect(&mut self, kind: Kind, expected: &[u8]) -> Result<()> {
        let actual = match self {
            Self::Noise(c) => {
                let f = c.receive.read()?;
                assert_eq!(f.kind, kind);
                assert_eq!(f.id, 1);
                f.bytes
            }
            Self::Tcp(s) => {
                let mut tag = [0];
                s.read_exact(&mut tag)?;
                assert_eq!(tag[0], kind as u8);
                let mut b = vec![0; expected.len()];
                s.read_exact(&mut b)?;
                b
            }
        };
        assert_eq!(actual, expected);
        Ok(())
    }
    fn upload(&mut self, index: u8, end: Instant) -> Result<()> {
        let mut buffer = vec![0; FRAME_BYTES];
        for offset in (0..BYTES).step_by(FRAME_BYTES) {
            assert!(Instant::now() < end, "transfer deadline");
            fill(&mut buffer, index, offset);
            match self {
                Self::Noise(c) => c.send.write(&Frame {
                    kind: Kind::Body,
                    id: 1,
                    bytes: buffer.clone(),
                })?,
                Self::Tcp(s) => s.write_all(&buffer)?,
            }
        }
        self.control(Kind::EndInput, &BYTES.to_be_bytes())?;
        self.expect(Kind::Success, &BYTES.to_be_bytes())
    }
    fn download(&mut self, index: u8, verify: bool, end: Instant) -> Result<()> {
        let mut buffer = vec![0; FRAME_BYTES];
        for offset in (0..BYTES).step_by(FRAME_BYTES) {
            assert!(Instant::now() < end, "transfer deadline");
            match self {
                Self::Noise(c) => {
                    let f = c.receive.read()?;
                    assert_eq!(f.kind, Kind::Body);
                    assert_eq!(f.id, 1);
                    assert_eq!(f.bytes.len(), FRAME_BYTES);
                    buffer = f.bytes;
                }
                Self::Tcp(s) => s.read_exact(&mut buffer)?,
            }
            if verify {
                assert!(
                    valid(&buffer, index, offset),
                    "payload mismatch at {offset}"
                );
            }
        }
        self.expect(Kind::EndInput, &BYTES.to_be_bytes())?;
        self.control(Kind::Success, &BYTES.to_be_bytes())
    }
}
fn fill(buffer: &mut [u8], index: u8, offset: u64) {
    for (word, bytes) in buffer.chunks_exact_mut(8).enumerate() {
        bytes.copy_from_slice(
            &SEED
                .wrapping_add(u64::from(index))
                .wrapping_add(offset / 8 + word as u64)
                .to_le_bytes(),
        );
    }
}
fn valid(buffer: &[u8], index: u8, offset: u64) -> bool {
    buffer.chunks_exact(8).enumerate().all(|(word, bytes)| {
        u64::from_le_bytes(bytes.try_into().unwrap())
            == SEED
                .wrapping_add(u64::from(index))
                .wrapping_add(offset / 8 + word as u64)
    })
}
fn env_key(name: &str) -> Result<[u8; 32]> {
    Ok(key(&std::env::var(name)?)?)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("self-check") {
        let mut b = [0; 32];
        fill(&mut b, 0, 0);
        assert!(valid(&b, 0, 0));
        use std::fmt::Write as _;
        let mut hex = String::with_capacity(b.len() * 2);
        for byte in b {
            write!(hex, "{byte:02x}").expect("hex formatting into a String");
        }
        println!("{hex}");
        b[17] ^= 1;
        assert!(!valid(&b, 0, 0));
        return Ok(());
    }
    assert_eq!(
        args.len(),
        6,
        "role carrier direction streams/index perf/verify"
    );
    let role = args[1].as_str();
    let carrier = args[2].as_str();
    let direction = args[3].as_str();
    let count: u8 = args[4].parse()?;
    let verify = args[5] == "verify";
    assert!(matches!(carrier, "tcp" | "noise"));
    assert!(matches!(direction, "upload" | "download"));
    assert!(matches!(args[5].as_str(), "perf" | "verify"));
    let seconds = if verify { 45 } else { 10 };
    if role == "server" {
        assert!((1..=2).contains(&count));
        let listener = TcpListener::bind("0.0.0.0:0")?;
        println!("LISTEN {}", listener.local_addr()?.port());
        io::stdout().flush()?;
        let private = env_key("LAYERFS_PRIVATE_KEY")?;
        let public = env_key("LAYERFS_PEER_KEY")?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);
        let barrier = Arc::new(Barrier::new(count as usize + 1));
        let mut workers = Vec::new();
        for index in 0..count {
            let (socket, _) = listener.accept()?;
            let ready = ready_tx.clone();
            let begin = Arc::clone(&barrier);
            let noise = carrier == "noise";
            let receive = direction == "upload";
            workers.push(thread::Builder::new().stack_size(2 * 1024 * 1024).spawn(
                move || -> Result<()> {
                    let peers = [Peer {
                        selector: u32::from(index) + 1,
                        public,
                        expires_unix: u64::MAX,
                    }];
                    let mut wire = if noise {
                        Wire::Noise(Box::new(accept(socket, &private, &peers)?))
                    } else {
                        Wire::Tcp(socket)
                    };
                    wire.deadline(5)?;
                    wire.expect(Kind::Hello, &[0, index])?;
                    ready.send(())?;
                    begin.wait();
                    wire.deadline(seconds)?;
                    let end = Instant::now() + Duration::from_secs(seconds);
                    wire.control(Kind::Hello, b"go")?;
                    if receive {
                        wire.download(index, verify, end)
                    } else {
                        wire.upload(index, end)
                    }
                },
            )?);
            // Host launches the next workload only after this stream is ready.
            ready_rx.recv_timeout(Duration::from_secs(5))?;
            println!("READY {index}");
            io::stdout().flush()?;
        }
        let mut start = String::new();
        io::stdin().read_line(&mut start)?;
        assert_eq!(start.trim(), "go");
        let now = Instant::now();
        barrier.wait();
        for worker in workers {
            worker
                .join()
                .map_err(|_| io::Error::other("stream failed"))??;
        }
        let elapsed = now.elapsed().as_nanos();
        println!("{{\"carrier\":\"{carrier}\",\"direction\":\"{direction}\",\"streams\":{count},\"payload_bytes\":{},\"transfer_ns\":{elapsed},\"payload_chunks\":{},\"authentication_handshakes\":{},\"verified_bytes\":{},\"admission_eligible\":false}}", BYTES*u64::from(count), (BYTES/FRAME_BYTES as u64)*u64::from(count), if carrier=="noise" {count} else {0}, if verify {BYTES*u64::from(count)} else {0});
    } else {
        assert_eq!(role, "client");
        assert!(count < 2);
        let address = std::env::var("LAYERFS_ENDPOINT")?
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::other("endpoint"))?;
        let mut wire = if carrier == "noise" {
            Wire::Noise(Box::new(connect(
                address,
                u32::from(count) + 1,
                &env_key("LAYERFS_PRIVATE_KEY")?,
                &env_key("LAYERFS_SERVER_KEY")?,
            )?))
        } else {
            Wire::Tcp(TcpStream::connect_timeout(
                &address,
                Duration::from_secs(5),
            )?)
        };
        wire.deadline(5)?;
        wire.control(Kind::Hello, &[0, count])?;
        wire.expect(Kind::Hello, b"go")?;
        wire.deadline(seconds)?;
        let end = Instant::now() + Duration::from_secs(seconds);
        if direction == "upload" {
            wire.upload(count, end)?;
        } else {
            wire.download(count, verify, end)?;
        }
    }
    Ok(())
}
