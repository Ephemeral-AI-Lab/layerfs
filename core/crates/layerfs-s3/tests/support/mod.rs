//! External service fixtures and controlled reply interception.
#![allow(dead_code)]
use layerfs_s3::S3Config;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn config(tag: &str) -> S3Config {
    let mut config =
        S3Config::from_env().expect("owned MinIO environment required; no skipped service tests");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    config.prefix = format!(
        "{}/{tag}-{}-{}-{nonce}",
        config.prefix,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    config
}
fn header(reader: &mut BufReader<TcpStream>) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).unwrap();
        assert!(!line.is_empty());
        assert!(bytes.len() + line.len() <= 16 * 1024);
        let end = line == b"\r\n";
        bytes.extend(line);
        if end {
            return bytes;
        }
    }
}
fn length(header: &[u8]) -> usize {
    std::str::from_utf8(header)
        .unwrap()
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap_or(0)
}
#[derive(Clone, Copy)]
pub enum Fault {
    DropReply,
    MalformedLength,
}
pub fn proxy(config: &S3Config, fault: Fault) -> (S3Config, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut proxied = config.clone();
    proxied.endpoint = format!("http://{}", listener.local_addr().unwrap());
    let upstream = config.endpoint.strip_prefix("http://").unwrap().to_owned();
    let thread = std::thread::spawn(move || {
        let (client, _) = listener.accept().unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut client = BufReader::new(client);
        let upstream = TcpStream::connect(upstream).unwrap();
        upstream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        upstream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut upstream = BufReader::new(upstream);
        let request = header(&mut client);
        upstream.get_mut().write_all(&request).unwrap();
        let expect = std::str::from_utf8(&request)
            .unwrap()
            .to_ascii_lowercase()
            .contains("expect: 100-continue");
        if expect {
            let interim = header(&mut upstream);
            assert!(interim.starts_with(b"HTTP/1.1 100 "));
            client.get_mut().write_all(&interim).unwrap();
        }
        let n = length(&request);
        let copied = std::io::copy(&mut (&mut client).take(n as u64), upstream.get_mut()).unwrap();
        assert_eq!(copied, n as u64);
        let response = header(&mut upstream);
        assert!(response.starts_with(b"HTTP/1.1 200 "));
        match fault {
            Fault::DropReply => {
                let n = length(&response);
                let copied =
                    std::io::copy(&mut (&mut upstream).take(n as u64), &mut std::io::sink())
                        .unwrap();
                assert_eq!(copied, n as u64);
            }
            Fault::MalformedLength => {
                let bad = std::str::from_utf8(&response)
                    .unwrap()
                    .split_inclusive("\r\n")
                    .map(|line| {
                        if line.to_ascii_lowercase().starts_with("content-length:") {
                            "Content-Length: invalid\r\n"
                        } else {
                            line
                        }
                    })
                    .collect::<String>();
                let _ = client.get_mut().write_all(bad.as_bytes());
                let n = length(&response);
                let _ = std::io::copy(&mut (&mut upstream).take(n as u64), client.get_mut());
            }
        }
    });
    (proxied, thread)
}
