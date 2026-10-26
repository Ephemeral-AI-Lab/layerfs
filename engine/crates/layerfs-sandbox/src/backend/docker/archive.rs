//! Canonical small deployment tar with streamed executable bytes and private config.
use super::Sandbox;
use crate::{RuntimeError, WireFailure};
use std::{
    fs::File,
    io::{self, Read, Seek, Write},
};
/// Original archive preparation/encoding work, distinct from positive socket transfer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadWork {
    pub executable_bytes: u64,
    pub tar_bytes: u64,
    pub executable_read: u64,
    pub tar_encoded: u64,
}
/// Partial extraction may exist even when no complete success acknowledgement arrived.
#[derive(Debug)]
pub struct UploadFailure {
    pub work: UploadWork,
    pub transfer: Box<WireFailure>,
    pub pending_source: crate::PendingRequest,
}
const DIRECTORIES: [(&str, u32); 4] = [
    ("layerfs-local/", 0o700),
    ("layerfs-local/config/", 0o700),
    ("layerfs-local/overlay/", 0o700),
    ("workspaces/", 0o755),
];
impl Sandbox {
    /// Injects config0600 and executable0755 while stopped, once. Caller retains the original input file/config.
    /// Archive extraction is incremental; failure is never treated as an atomic rollback.
    pub fn upload(
        &mut self,
        executable: &mut File,
        config: &[u8],
    ) -> Result<UploadWork, Box<UploadFailure>> {
        let mut work = UploadWork::default();
        let mut pending_source = crate::PendingRequest::default();
        let result = (|| {
            if self.upload_attempted
                || self.start_attempted
                || self.stop_attempted
                || self.delete_attempted
            {
                return Err(self.invalid("original upload admission"));
            }
            let metadata = executable
                .metadata()
                .map_err(|e| self.selected(super::http::failure(false, 0, None, e.into(), None)))?;
            if !metadata.is_file()
                || metadata.len() == 0
                || executable.stream_position().map_err(|e| {
                    self.selected(super::http::failure(false, 0, None, e.into(), None))
                })? != 0
                || config.is_empty()
                || config.len() > 8192
            {
                return Err(self.invalid("deployment executable/config metadata"));
            }
            work.executable_bytes = metadata.len();
            let length = 512u64
                .checked_mul(DIRECTORIES.len() as u64 + 2)
                .and_then(|v| v.checked_add(padded(metadata.len())?))
                .and_then(|v| v.checked_add(padded(config.len() as u64)?))
                .and_then(|v| v.checked_add(1024))
                .ok_or_else(|| self.invalid("deployment tar length width"))?;
            work.tar_bytes = length;
            // POSIX ustar numeric format bound; not a runtime file or stream cap.
            if metadata.len() > 0o77777777777 {
                return Err(self.invalid("deployment ustar executable size"));
            }
            self.upload_attempted = true;
            let path = format!(
                "/v1.54/containers/{}/archive?path=/&noOverwriteDirNonDir=true&copyUIDGID=true",
                self.id
            );
            let mut response = self
                .docker
                .exchange_body("PUT", &path, false, "application/x-tar", length, |out| {
                    encode(
                        executable,
                        metadata.len(),
                        config,
                        out,
                        &mut work,
                        &mut pending_source,
                    )
                })
                .map_err(|f| self.selected(f))?;
            if response.status != 200 {
                return Err(self.selected(response.fail(RuntimeError::Http(response.status), None)));
            }
            let mut one = [0];
            match response.body.read(&mut one) {
                Ok(0) => (),
                Ok(_) => {
                    return Err(self.selected(response.fail(
                        RuntimeError::Protocol("unexpected archive reply body"),
                        None,
                    )))
                }
                Err(e) => return Err(self.selected(response.fail(e.into(), None))),
            }
            self.uploaded = true;
            Ok(())
        })();
        result.map(|()| work).map_err(|transfer| {
            Box::new(UploadFailure {
                work,
                transfer,
                pending_source,
            })
        })
    }
}
fn encode(
    executable: &mut File,
    length: u64,
    config: &[u8],
    out: &mut dyn Write,
    work: &mut UploadWork,
    pending: &mut crate::PendingRequest,
) -> io::Result<()> {
    let mut out = Encoded {
        inner: out,
        count: &mut work.tar_encoded,
    };
    for (path, mode) in DIRECTORIES {
        out.write_all(&header(path, mode, 0, true)?)?;
    }
    out.write_all(&header(
        "layerfs-local/config/daemon.setup",
        0o600,
        config.len() as u64,
        false,
    )?)?;
    out.write_all(config)?;
    padding(&mut out, config.len() as u64)?;
    out.write_all(&header(
        "usr/local/bin/layerfs-daemon",
        0o755,
        length,
        false,
    )?)?;
    let mut bytes = [0; 8192];
    let mut left = length;
    while left != 0 {
        let wanted = left.min(bytes.len() as u64) as usize;
        let n = executable.read(&mut bytes[..wanted])?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        work.executable_read += n as u64;
        let before = *out.count;
        if let Err(error) = out.write_all(&bytes[..n]) {
            let encoded = (*out.count - before) as usize;
            pending.bytes.extend_from_slice(&bytes[encoded..n]);
            return Err(error);
        }
        left -= n as u64;
    }
    if executable.read(&mut bytes[..1])? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "deployment executable grew",
        ));
    }
    padding(&mut out, length)?;
    out.write_all(&[0; 1024])
}

fn padded(n: u64) -> Option<u64> {
    n.checked_add(511).map(|v| v / 512 * 512)
}
fn padding(out: &mut impl Write, n: u64) -> io::Result<()> {
    let count = ((512 - n % 512) % 512) as usize;
    out.write_all(&[0; 512][..count])
}
fn header(path: &str, mode: u32, size: u64, directory: bool) -> io::Result<[u8; 512]> {
    let mut bytes = [0; 512];
    if path.len() > 100 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "deployment tar path",
        ));
    }
    bytes[..path.len()].copy_from_slice(path.as_bytes());
    for (range, value) in [
        (100..108, u64::from(mode)),
        (108..116, 0),
        (116..124, 0),
        (124..136, size),
        (136..148, 0),
    ] {
        let text = format!("{:0width$o}\0", value, width = range.len() - 1);
        if text.len() != range.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ustar numeric width",
            ));
        }
        bytes[range].copy_from_slice(text.as_bytes());
    }
    bytes[148..156].fill(b' ');
    bytes[156] = if directory { b'5' } else { b'0' };
    bytes[257..263].copy_from_slice(b"ustar\0");
    bytes[263..265].copy_from_slice(b"00");
    let sum: u32 = bytes.iter().map(|b| u32::from(*b)).sum();
    bytes[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    Ok(bytes)
}
struct Encoded<'a> {
    inner: &'a mut dyn Write,
    count: &'a mut u64,
}
impl Write for Encoded<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        *self.count += n as u64;
        Ok(n)
    }
    fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let n = self.write(bytes)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
