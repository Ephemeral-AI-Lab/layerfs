//! E2c: the framing ladder across the real container<->host boundary. std only.
use std::io::{IoSlice, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;
const FRAME: usize = 16 * KIB;
const RECORD: usize = 256 * KIB;
const TOTAL: u64 = 1024 * MIB as u64;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Variant {
    Plain16k,
    Record2Setsockopt,
    Record2,
    Writev16k,
    Record256k,
}

impl Variant {
    fn parse(s: &str) -> Variant {
        match s {
            "plain16k" => Variant::Plain16k,
            "record2sockopt" => Variant::Record2Setsockopt,
            "record2" => Variant::Record2,
            "writev16k" => Variant::Writev16k,
            "record256k" => Variant::Record256k,
            other => panic!("unknown variant {other}"),
        }
    }
    fn unit(&self) -> usize {
        match self {
            Variant::Record256k => RECORD,
            _ => FRAME,
        }
    }
}

struct Counted(Arc<AtomicU64>);
impl Counted {
    fn bump(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// Send TOTAL bytes with the variant's write pattern. Returns the transfer seconds.
fn send_all(s: &mut TcpStream, variant: Variant, calls: &Counted) -> f64 {
    let unit = variant.unit();
    let units = (TOTAL / unit as u64) as usize;
    let data = vec![0x5au8; unit];
    let started = Instant::now();
    match variant {
        Variant::Record2Setsockopt | Variant::Record2 => {
            for _ in 0..units {
                if variant == Variant::Record2Setsockopt {
                    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.bump();
                }
                s.write_all(&(unit as u32).to_be_bytes()).unwrap();
                calls.bump();
                if variant == Variant::Record2Setsockopt {
                    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.bump();
                }
                s.write_all(&data).unwrap();
                calls.bump();
            }
        }
        Variant::Writev16k | Variant::Record256k => {
            s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            let chunks = std::cmp::max(1, unit / (64 * KIB));
            let chunk = unit / chunks;
            for _ in 0..units {
                let head = (unit as u32).to_be_bytes();
                let mut storage = vec![IoSlice::new(&head)];
                for _ in 0..chunks {
                    storage.push(IoSlice::new(&data[..chunk]));
                }
                let mut slices = &mut storage[..];
                while !slices.is_empty() {
                    let n = s.write_vectored(slices).unwrap();
                    calls.bump();
                    if n == 0 {
                        panic!("write zero");
                    }
                    IoSlice::advance_slices(&mut slices, n);
                }
            }
        }
        Variant::Plain16k => {
            s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            for _ in 0..units {
                s.write_all(&data).unwrap();
                calls.bump();
            }
        }
    }
    started.elapsed().as_secs_f64()
}

/// Receive TOTAL bytes with the variant's read pattern. Returns the transfer seconds.
fn recv_all(s: &mut TcpStream, variant: Variant, calls: &Counted) -> f64 {
    let unit = variant.unit();
    let units = (TOTAL / unit as u64) as usize;
    let mut buf = vec![0u8; unit + 4];
    let started = Instant::now();
    match variant {
        Variant::Record2Setsockopt | Variant::Record2 => {
            for _ in 0..units {
                let mut h = [0u8; 4];
                if variant == Variant::Record2Setsockopt {
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.bump();
                }
                s.read_exact(&mut h).unwrap();
                calls.bump();
                if variant == Variant::Record2Setsockopt {
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.bump();
                }
                s.read_exact(&mut buf[..unit]).unwrap();
                calls.bump();
            }
        }
        Variant::Plain16k => {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            for _ in 0..units {
                s.read_exact(&mut buf[..unit]).unwrap();
                calls.bump();
            }
        }
        Variant::Writev16k | Variant::Record256k => {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            for _ in 0..units {
                s.read_exact(&mut buf[..unit + 4]).unwrap();
                calls.bump();
            }
        }
    }
    started.elapsed().as_secs_f64()
}

fn report(role: &str, variant: Variant, secs: f64, calls: u64, units: f64) {
    println!(
        "{{\"role\":\"{role}\",\"variant\":\"{variant:?}\",\"seconds\":{secs:.4},\"GBps\":{:.3},\"us_per_unit\":{:.2},\"calls_per_unit\":{:.2}}}",
        (TOTAL as f64 / 1e9) / secs,
        secs * 1e6 / units,
        calls as f64 / units
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let role = args.get(1).cloned().unwrap_or_default();
    if role == "server" || role == "server-send" {
        let send = role == "server-send";
        let port: u16 = args.get(2).map(|p| p.parse().unwrap()).unwrap_or(0);
        let variants: Vec<Variant> = args[3..].iter().map(|s| Variant::parse(s)).collect();
        let listener = TcpListener::bind(("0.0.0.0", port)).unwrap();
        println!("LISTEN {}", listener.local_addr().unwrap().port());
        std::io::stdout().flush().unwrap();
        for variant in variants {
            let calls = Arc::new(AtomicU64::new(0));
            let (mut s, _) = listener.accept().unwrap();
            s.set_nodelay(true).unwrap();
            let mut go = [0u8; 1];
            s.read_exact(&mut go).unwrap();
            s.write_all(b"g").unwrap();
            let secs = if send {
                send_all(&mut s, variant, &Counted(Arc::clone(&calls)))
            } else {
                recv_all(&mut s, variant, &Counted(Arc::clone(&calls)))
            };
            let units = (TOTAL / variant.unit() as u64) as f64;
            report(if send { "host-server-send" } else { "host-server-recv" }, variant, secs, calls.load(Ordering::Relaxed), units);
            std::io::stdout().flush().unwrap();
        }
    } else if role == "client" || role == "client-recv" {
        let recv = role == "client-recv";
        let addr = args.get(2).expect("addr:port").clone();
        let variants: Vec<Variant> = args[3..].iter().map(|s| Variant::parse(s)).collect();
        for variant in variants {
            let calls = Arc::new(AtomicU64::new(0));
            let sock = addr.to_socket_addrs().unwrap().next().unwrap();
            let mut s = TcpStream::connect(sock).unwrap();
            s.set_nodelay(true).unwrap();
            s.write_all(b"r").unwrap();
            let mut go = [0u8; 1];
            s.read_exact(&mut go).unwrap();
            let secs = if recv {
                recv_all(&mut s, variant, &Counted(Arc::clone(&calls)))
            } else {
                send_all(&mut s, variant, &Counted(Arc::clone(&calls)))
            };
            let units = (TOTAL / variant.unit() as u64) as f64;
            report(if recv { "container-client-recv" } else { "container-client-send" }, variant, secs, calls.load(Ordering::Relaxed), units);
            std::io::stdout().flush().unwrap();
        }
    } else {
        eprintln!("usage: phase1std server|server-send <port|0> <variant>... | client|client-recv <addr:port> <variant>...");
    }
}
