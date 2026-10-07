//! One-shot external application control; never runtime publication authority.
use crate::digest::{hex, sha256};
use std::{
    cell::Cell,
    error::Error,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const CONFIG_WINDOW: usize = 8192;
const BODY_WINDOW: usize = 16384;
const BINDING_WINDOW: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Host,
    Consumer,
}
#[derive(Debug)]
pub struct Failure {
    pub phase: &'static str,
    pub path: PathBuf,
    pub acknowledged_bytes: u64,
    pub primary: io::Error,
    pub secondary_close: Option<io::Error>,
    /// close(2) was attempted once; its failure leaves descriptor disposition unknown.
    pub close_disposition_unknown: bool,
}
impl Failure {
    fn at(phase: &'static str, path: &Path, primary: io::Error) -> Self {
        Self {
            phase,
            path: path.to_owned(),
            acknowledged_bytes: 0,
            primary,
            secondary_close: None,
            close_disposition_unknown: false,
        }
    }
    fn invalid(path: &Path, reason: &'static str) -> Self {
        Self::at("validation", path, io::Error::other(reason))
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.primary)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FinalBinding {
    pub correlation: u64,
    pub bytes: Vec<u8>,
    pub sha256: String,
}
impl FinalBinding {
    pub fn new(correlation: u64, bytes: &[u8]) -> Result<Self, Failure> {
        if correlation == 0 || bytes.is_empty() || bytes.len() > BINDING_WINDOW {
            return Err(Failure::invalid(
                Path::new(""),
                "final Binding control window/identity",
            ));
        }
        Ok(Self {
            correlation,
            bytes: bytes.to_vec(),
            sha256: hex(&sha256(bytes)),
        })
    }
}
#[derive(Debug)]
pub struct Receipt {
    pub run_id: String,
    pub correlation: u64,
    pub binding_sha256: String,
    pub payload_path: PathBuf,
    pub payload_bytes: u64,
    pub payload_sha256: String,
    pub marker_path: PathBuf,
}
#[derive(Debug)]
pub struct Ready {
    pub binding: FinalBinding,
    pub receipt: Receipt,
}
#[derive(Debug)]
pub struct Config {
    pub run_id: String,
    pub directory: PathBuf,
    pub source_path: PathBuf,
    pub source_bytes: u64,
    pub source_sha256: String,
    role: Role,
    published: Cell<bool>,
    consumed: Cell<bool>,
    failed: Cell<bool>,
}
impl Config {
    pub fn read(path: &Path, role: Role) -> Result<Self, Failure> {
        let source_path = path
            .canonicalize()
            .map_err(|e| Failure::at("config-path", path, e))?;
        let bytes = read_file(&source_path, CONFIG_WINDOW)?;
        let fields = lines(&bytes, &source_path, 5)?;
        if fields[0] != "layerfs-e04-disposal-control-v1"
            || !digest_text(fields[1])
            || fields[4] != "explicit-host-fence-v1"
            || !Path::new(fields[2]).is_absolute()
            || !Path::new(fields[3]).is_absolute()
        {
            return Err(Failure::invalid(
                path,
                "sealed disposal configuration differs",
            ));
        }
        let directory = PathBuf::from(fields[if role == Role::Host { 2 } else { 3 }]);
        let meta = std::fs::symlink_metadata(&directory)
            .map_err(|e| Failure::at("control-directory", &directory, e))?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || directory
                .canonicalize()
                .map_err(|e| Failure::at("control-directory", &directory, e))?
                != directory
        {
            return Err(Failure::invalid(
                &directory,
                "control directory must be canonical and owned",
            ));
        }
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|e| Failure::at("control-directory-inventory", &directory, e))?;
        if let Some(entry) = entries.next() {
            entry.map_err(|e| Failure::at("control-directory-inventory", &directory, e))?;
            return Err(Failure::invalid(
                &directory,
                "control directory is not fresh before endpoint start",
            ));
        }
        Ok(Self {
            run_id: fields[1].to_owned(),
            directory,
            source_path,
            source_bytes: bytes.len() as u64,
            source_sha256: hex(&sha256(&bytes)),
            role,
            published: Cell::new(false),
            consumed: Cell::new(false),
            failed: Cell::new(false),
        })
    }
    pub fn publish_ready(&self, binding: &FinalBinding) -> Result<Receipt, Failure> {
        self.binding_valid(binding)?;
        let body = format!(
            "layerfs-e04-disposal-ready-v1\n{}\n{}\n{}\n",
            self.run_id,
            binding.correlation,
            bytes_hex(&binding.bytes)
        );
        self.publish(Role::Consumer, "consumer-ready", binding, body.as_bytes())
    }
    pub fn try_ready(&self) -> Result<Option<Ready>, Failure> {
        self.check(Role::Host, false)?;
        let result = (|| {
            let Some(bytes) = self.receive("consumer-ready")? else {
                return Ok(None);
            };
            let path = self.directory.join("consumer-ready.body");
            let fields = self.fields(&bytes, &path, "layerfs-e04-disposal-ready-v1")?;
            let correlation = correlation(fields[2], &path)?;
            let binding = FinalBinding::new(correlation, &decode_hex(fields[3], &path)?)?;
            let receipt = self.receipt("consumer-ready", &binding, &bytes);
            Ok(Some(Ready { binding, receipt }))
        })();
        self.mark(result)
    }
    pub fn publish_ack(&self, ready: &Ready) -> Result<Receipt, Failure> {
        if !self.consumed.get() || ready.receipt.run_id != self.run_id {
            self.failed.set(true);
            return Err(Failure::invalid(
                &self.directory,
                "ack lacks original received ready control",
            ));
        }
        self.binding_valid(&ready.binding)?;
        let body = format!(
            "layerfs-e04-disposal-ack-v1\n{}\n{}\n{}\n",
            self.run_id, ready.binding.correlation, ready.binding.sha256
        );
        self.publish(Role::Host, "host-ack", &ready.binding, body.as_bytes())
    }
    pub fn wait_ack(&self, binding: &FinalBinding, deadline: Instant) -> Result<Receipt, Failure> {
        self.check(Role::Consumer, false)?;
        if !self.published.get() {
            self.failed.set(true);
            return Err(Failure::invalid(
                &self.directory,
                "ack wait precedes original ready publication",
            ));
        }
        let result = (|| loop {
            if Instant::now() >= deadline {
                return Err(Failure::at(
                    "ack-readiness",
                    &self.directory,
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "original host acknowledgment deadline",
                    ),
                ));
            }
            if let Some(bytes) = self.receive("host-ack")? {
                let path = self.directory.join("host-ack.body");
                let fields = self.fields(&bytes, &path, "layerfs-e04-disposal-ack-v1")?;
                if correlation(fields[2], &path)? != binding.correlation
                    || fields[3] != binding.sha256
                {
                    return Err(Failure::invalid(
                        &path,
                        "ack differs from original final Binding",
                    ));
                }
                return Ok(self.receipt("host-ack", binding, &bytes));
            }
            std::thread::sleep(Duration::from_millis(1));
        })();
        self.mark(result)
    }
    fn fields<'a>(
        &self,
        bytes: &'a [u8],
        path: &Path,
        schema: &str,
    ) -> Result<Vec<&'a str>, Failure> {
        let fields = lines(bytes, path, 4)?;
        if fields[0] != schema || fields[1] != self.run_id {
            return Err(Failure::invalid(path, "control schema/run mismatch"));
        }
        Ok(fields)
    }
    fn binding_valid(&self, binding: &FinalBinding) -> Result<(), Failure> {
        if !digest_text(&self.run_id)
            || binding.correlation == 0
            || binding.bytes.is_empty()
            || binding.bytes.len() > BINDING_WINDOW
            || !digest_text(&binding.sha256)
            || binding.sha256 != hex(&sha256(&binding.bytes))
        {
            self.failed.set(true);
            return Err(Failure::invalid(
                &self.directory,
                "original bounded Binding identity",
            ));
        }
        Ok(())
    }
    fn check(&self, role: Role, publish: bool) -> Result<(), Failure> {
        if self.role != role
            || self.failed.get()
            || if publish {
                self.published.get()
            } else {
                self.consumed.get()
            }
        {
            return Err(Failure::invalid(
                &self.directory,
                "original control owner is terminal or already attempted",
            ));
        }
        Ok(())
    }
    fn mark<T>(&self, result: Result<T, Failure>) -> Result<T, Failure> {
        if result.is_err() {
            self.failed.set(true);
        }
        result
    }
    fn publish(
        &self,
        role: Role,
        name: &str,
        binding: &FinalBinding,
        bytes: &[u8],
    ) -> Result<Receipt, Failure> {
        self.check(role, true)?;
        self.published.set(true);
        let result = (|| {
            if bytes.len() > BODY_WINDOW
                || binding.correlation == 0
                || binding.bytes.is_empty()
                || binding.bytes.len() > BINDING_WINDOW
                || binding.sha256 != hex(&sha256(&binding.bytes))
            {
                return Err(Failure::invalid(
                    &self.directory,
                    "control publication window/Binding identity",
                ));
            }
            let receipt = self.receipt(name, binding, bytes);
            match std::fs::symlink_metadata(&receipt.marker_path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => {
                    return Err(Failure::at(
                        "complete-precondition",
                        &receipt.marker_path,
                        error,
                    ))
                }
                Ok(_) => {
                    return Err(Failure::at(
                        "complete-precondition",
                        &receipt.marker_path,
                        io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "original complete marker already exists",
                        ),
                    ))
                }
            }
            write_file(&receipt.payload_path, bytes)?;
            // Directory creation is the atomic complete-publication event. No
            // receiver ever parses a marker file while its bytes are being written.
            std::fs::create_dir(&receipt.marker_path).map_err(|e| {
                let mut original = Failure::at("complete-publication", &receipt.marker_path, e);
                original.acknowledged_bytes = receipt.payload_bytes;
                original
            })?;
            Ok(receipt)
        })();
        self.mark(result)
    }
    fn receive(&self, name: &str) -> Result<Option<Vec<u8>>, Failure> {
        let marker = self.directory.join(format!("{name}.complete"));
        match std::fs::symlink_metadata(&marker) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Failure::at("complete-readiness", &marker, e)),
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(Failure::invalid(
                    &marker,
                    "complete publication marker is not an owned directory",
                ))
            }
            Ok(_) => (),
        }
        self.consumed.set(true);
        read_file(&self.directory.join(format!("{name}.body")), BODY_WINDOW).map(Some)
    }
    fn receipt(&self, name: &str, binding: &FinalBinding, bytes: &[u8]) -> Receipt {
        Receipt {
            run_id: self.run_id.clone(),
            correlation: binding.correlation,
            binding_sha256: binding.sha256.clone(),
            payload_path: self.directory.join(format!("{name}.body")),
            payload_bytes: bytes.len() as u64,
            payload_sha256: hex(&sha256(bytes)),
            marker_path: self.directory.join(format!("{name}.complete")),
        }
    }
}

fn lines<'a>(bytes: &'a [u8], path: &Path, count: usize) -> Result<Vec<&'a str>, Failure> {
    if !bytes.is_ascii() || !bytes.ends_with(b"\n") || bytes.contains(&b'\r') {
        return Err(Failure::invalid(
            path,
            "control requires complete ASCII lines",
        ));
    }
    let fields: Vec<_> = std::str::from_utf8(bytes)
        .expect("ASCII checked")
        .lines()
        .collect();
    if fields.len() != count {
        return Err(Failure::invalid(path, "control field cardinality"));
    }
    Ok(fields)
}
fn digest_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn correlation(value: &str, path: &Path) -> Result<u64, Failure> {
    value
        .parse::<u64>()
        .ok()
        .filter(|v| *v != 0 && v.to_string() == value)
        .ok_or_else(|| Failure::invalid(path, "original correlation is not canonical/nonzero"))
}
pub fn bytes_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[usize::from(byte >> 4)] as char);
        result.push(HEX[usize::from(byte & 15)] as char);
    }
    result
}
fn decode_hex(value: &str, path: &Path) -> Result<Vec<u8>, Failure> {
    if value.is_empty()
        || value.len() > BINDING_WINDOW * 2
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(Failure::invalid(path, "original Binding byte encoding"));
    }
    (0..value.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&value[i..i + 2], 16)
                .map_err(|_| Failure::invalid(path, "original Binding byte encoding"))
        })
        .collect()
}

#[cfg(unix)]
fn close_file(file: File) -> io::Result<()> {
    use std::os::fd::IntoRawFd;
    extern "C" {
        fn close(fd: std::os::raw::c_int) -> std::os::raw::c_int;
    }
    // Consume the descriptor exactly once. EINTR and other errors are retained;
    // no retry or reconstructed File guesses whether the kernel closed it.
    let result = unsafe { close(file.into_raw_fd()) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn close_file(file: File) -> io::Result<()> {
    drop(file);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native control close requires Unix",
    ))
}
fn complete_io<T>(
    file: File,
    path: &Path,
    phase: &'static str,
    count: u64,
    result: io::Result<T>,
) -> Result<T, Failure> {
    let closed = close_file(file);
    match (result, closed) {
        (Ok(value), Ok(())) => Ok(value),
        (result, closed) => {
            let unknown = closed.is_err();
            let (primary, secondary_close) = match (result, closed) {
                (Err(primary), closed) => (primary, closed.err()),
                (Ok(_), Err(primary)) => (primary, None),
                _ => unreachable!("success handled"),
            };
            Err(Failure {
                phase,
                path: path.to_owned(),
                acknowledged_bytes: count,
                primary,
                secondary_close,
                close_disposition_unknown: unknown,
            })
        }
    }
}
fn read_file(path: &Path, maximum: usize) -> Result<Vec<u8>, Failure> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| Failure::at("input-stat", path, e))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Failure::invalid(
            path,
            "control input must be an ordinary file",
        ));
    }
    let mut file = File::open(path).map_err(|e| Failure::at("input-open", path, e))?;
    let mut bytes = Vec::new();
    let result = (|| {
        let mut window = [0; 1024];
        loop {
            let count = file.read(&mut window)?;
            if count == 0 {
                return Ok(());
            }
            bytes.extend_from_slice(&window[..count]);
            if bytes.len() > maximum {
                return Err(io::Error::other("control input byte window"));
            }
        }
    })();
    complete_io(file, path, "input-read-close", bytes.len() as u64, result)?;
    Ok(bytes)
}
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Failure::at("output-open", path, e))?;
    let mut count = 0;
    let result = (|| {
        while count < bytes.len() {
            let n = file.write(&bytes[count..])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "original control output",
                ));
            }
            count += n;
        }
        Ok(())
    })();
    complete_io(file, path, "output-write-close", count as u64, result)
}
