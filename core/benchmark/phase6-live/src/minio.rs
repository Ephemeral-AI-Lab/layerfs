use crate::minio_failure::UploadFailure;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct Minio {
    pub authority: String,
    pub bucket: String,
    pub access: String,
    pub secret: String,
    pub stats: Arc<Mutex<crate::minio_stats::Statistics>>,
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut out, "{b:02x}").unwrap();
    }
    out
}
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn hmac(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut normalized = [0u8; 64];
    if key.len() > 64 {
        normalized[..32].copy_from_slice(&digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(normalized.map(|b| b ^ 0x36));
    inner.update(value);
    let mut outer = Sha256::new();
    outer.update(normalized.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}
fn date() -> String {
    let mut time = 0;
    let mut broken = std::mem::MaybeUninit::<libc::tm>::uninit();
    unsafe {
        libc::time(&mut time);
        libc::gmtime_r(&time, broken.as_mut_ptr());
    }
    let t = unsafe { broken.assume_init() };
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.tm_year + 1900,
        t.tm_mon + 1,
        t.tm_mday,
        t.tm_hour,
        t.tm_min,
        t.tm_sec
    )
}
impl Minio {
    pub fn statistics(&self) -> Result<crate::minio_stats::Statistics, String> {
        self.stats
            .lock()
            .map(|s| *s)
            .map_err(|_| "MinIO statistics owner".into())
    }
    // The isolated local experiment deliberately has no TLS/provider fallback.
    pub fn call(
        &self,
        method: &str,
        key: &str,
        body: &[u8],
        conditional: bool,
    ) -> Result<(u16, Vec<u8>), String> {
        self.call_classified(method, key, body, conditional)
            .map_err(|e| e.to_string())
    }
    pub fn call_classified(
        &self,
        method: &str,
        key: &str,
        body: &[u8],
        conditional: bool,
    ) -> Result<(u16, Vec<u8>), UploadFailure> {
        if !key.bytes().all(|b| b.is_ascii_hexdigit()) || key.len() > 64 {
            return Err(UploadFailure::Definite("invalid object key".into()));
        }
        {
            let mut stats = self
                .stats
                .lock()
                .map_err(|_| UploadFailure::Definite("MinIO statistics owner".into()))?;
            match method {
                "PUT" => {
                    stats.put_calls += 1;
                    stats.put_requested_bytes =
                        stats.put_requested_bytes.saturating_add(body.len() as u64)
                }
                "GET" => stats.get_calls += 1,
                _ => {}
            }
        }
        let path = if key.is_empty() {
            format!("/{}", self.bucket)
        } else {
            format!("/{}/{}", self.bucket, key)
        };
        let now = date();
        let hash = hex(&digest(body));
        let headers = format!(
            "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            self.authority, hash, now
        );
        let signed = "host;x-amz-content-sha256;x-amz-date";
        let canonical = format!("{method}\n{path}\n\n{headers}\n{signed}\n{hash}");
        let scope = format!("{}/us-east-1/s3/aws4_request", &now[..8]);
        let to_sign = format!(
            "AWS4-HMAC-SHA256\n{now}\n{scope}\n{}",
            hex(&digest(canonical.as_bytes()))
        );
        let k = hmac(
            format!("AWS4{}", self.secret).as_bytes(),
            now[..8].as_bytes(),
        );
        let k = hmac(&k, b"us-east-1");
        let k = hmac(&k, b"s3");
        let k = hmac(&k, b"aws4_request");
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={}",
            self.access,
            hex(&hmac(&k, to_sign.as_bytes()))
        );
        let mut stream = TcpStream::connect(&self.authority)
            .map_err(|e| UploadFailure::Definite(e.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| UploadFailure::Definite(e.to_string()))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| UploadFailure::Definite(e.to_string()))?;
        let condition = if conditional {
            "If-None-Match: *\r\n"
        } else {
            ""
        };
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: {}\r\nX-Amz-Content-Sha256: {hash}\r\nX-Amz-Date: {now}\r\nAuthorization: {authorization}\r\n{condition}Content-Length: {}\r\nConnection: close\r\n\r\n",self.authority,body.len()).map_err(|e| UploadFailure::Unknown(e.to_string()))?;
        stream
            .write_all(body)
            .map_err(|e| UploadFailure::Unknown(e.to_string()))?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        (&mut reader)
            .take(16384)
            .read_line(&mut line)
            .map_err(|e| UploadFailure::Unknown(e.to_string()))?;
        let status: u16 = line
            .split_whitespace()
            .nth(1)
            .ok_or("HTTP status")?
            .parse()
            .map_err(|_| "HTTP status")?;
        let mut length = None;
        let mut header_bytes = line.len();
        loop {
            line.clear();
            let n = (&mut reader)
                .take(16384)
                .read_line(&mut line)
                .map_err(|e| UploadFailure::Unknown(e.to_string()))?;
            header_bytes += n;
            if n == 0 || header_bytes > 16384 {
                return Err("HTTP header bound/EOF".into());
            }
            if line == "\r\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                if k.eq_ignore_ascii_case("content-length") {
                    length = Some(v.trim().parse::<usize>().map_err(|_| "HTTP length")?);
                }
                if k.eq_ignore_ascii_case("transfer-encoding") {
                    return Err("unsupported HTTP transfer encoding".into());
                }
            }
        }
        let n = length.ok_or("missing HTTP content length")?;
        if n > 1024 * 1024 {
            return Err("HTTP body bound".into());
        }
        let mut response = vec![0; n];
        reader
            .read_exact(&mut response)
            .map_err(|e| UploadFailure::Unknown(e.to_string()))?;
        if method == "GET" {
            let mut stats = self.stats.lock().map_err(|_| "MinIO statistics owner")?;
            stats.get_received_bytes = stats
                .get_received_bytes
                .saturating_add(response.len() as u64);
        }
        Ok((status, response))
    }
    pub fn create_bucket(&self) -> Result<(), String> {
        let (status, _) = self.call("PUT", "", &[], false)?;
        if status != 200 {
            return Err(format!("bucket creation HTTP {status}"));
        }
        Ok(())
    }
    pub fn put(&self, key: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
        self.put_classified(key, bytes).map_err(|e| e.to_string())
    }
    pub fn put_classified(&self, key: &[u8; 32], bytes: &[u8]) -> Result<(), UploadFailure> {
        let (status, _) = self.call_classified("PUT", &hex(key), bytes, true)?;
        match status {
            200 => Ok(()),
            412 => {
                let existing = self.get(key).map_err(UploadFailure::Definite)?;
                if existing == bytes {
                    Ok(())
                } else {
                    Err(UploadFailure::Definite("immutable key collision".into()))
                }
            }
            400..=499 => Err(UploadFailure::Definite(format!(
                "object upload HTTP {status}"
            ))),
            other => Err(UploadFailure::Unknown(format!(
                "object upload HTTP {other}"
            ))),
        }
    }
    pub fn get(&self, key: &[u8; 32]) -> Result<Vec<u8>, String> {
        let (status, bytes) = self.call("GET", &hex(key), &[], false)?;
        if status != 200 {
            return Err(format!("object read HTTP {status}"));
        }
        if digest(&bytes) != *key {
            return Err("pack digest mismatch".into());
        }
        Ok(bytes)
    }
}
