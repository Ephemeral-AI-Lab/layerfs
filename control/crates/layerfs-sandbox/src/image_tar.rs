//! The flat root filesystem tar a Sandbox image is copied as: `docker
//! export`'s output with the files `docker create`'s init layer adds taken
//! out (`.dockerenv`, `etc/hosts`, `etc/hostname`, `etc/resolv.conf`,
//! `etc/mtab`, everything under `dev/`), and every device node and FIFO,
//! which the unpacker refuses. Those are the bytes that differ between two
//! exports of one image; what is left is the image's own tree. The filter
//! streams: it holds at most one entry's extended headers, never a file's
//! bytes.
use std::io::{self, Read};

const BLOCK: usize = 512;
/// The largest pax or GNU extended header kept while its entry is decided.
const EXTENDED_MAX: u64 = 1 << 20;

/// Paths the init layer of `docker create` adds or replaces.
const INIT_FILES: [&str; 5] = [
    ".dockerenv",
    "etc/hosts",
    "etc/hostname",
    "etc/resolv.conf",
    "etc/mtab",
];

/// Whether an entry at `path` of type `flag` is left out of the copy.
pub fn dropped(path: &str, flag: u8) -> bool {
    let path = path.trim_start_matches("./").trim_start_matches('/');
    let path = path.trim_end_matches('/');
    matches!(flag, b'3' | b'4' | b'6')
        || INIT_FILES.contains(&path)
        || path
            .strip_prefix("dev/")
            .is_some_and(|rest| !rest.is_empty())
}

/// A reader of `input` (a tar stream) that yields the same tar without the
/// [`dropped`] entries.
pub struct Filtered<R> {
    input: R,
    /// Bytes ready to hand out.
    out: Vec<u8>,
    at: usize,
    /// Bytes of the current kept entry's data (with padding) still to copy.
    copying: u64,
    done: bool,
}

impl<R: Read> Filtered<R> {
    /// Filters `input`.
    pub fn new(input: R) -> Self {
        Self {
            input,
            out: Vec::new(),
            at: 0,
            copying: 0,
            done: false,
        }
    }

    /// Reads exactly `buffer.len()` bytes, or none at a clean end.
    fn block(&mut self, buffer: &mut [u8]) -> io::Result<bool> {
        let mut read = 0;
        while read < buffer.len() {
            let n = self.input.read(&mut buffer[read..])?;
            if n == 0 {
                if read == 0 {
                    return Ok(false);
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the image tar ends inside a block",
                ));
            }
            read += n;
        }
        Ok(true)
    }

    /// Skips `count` bytes of input.
    fn skip(&mut self, count: u64) -> io::Result<()> {
        let copied = io::copy(&mut (&mut self.input).take(count), &mut io::sink())?;
        if copied != count {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }

    /// Decides the next entry: fills `out` with its headers when it is kept
    /// (its data then follows through `copying`), or skips it.
    fn next_entry(&mut self) -> io::Result<()> {
        let mut pending: Vec<u8> = Vec::new();
        let mut long_name: Option<String> = None;
        loop {
            let mut header = [0u8; BLOCK];
            if !self.block(&mut header)? {
                // a stream without its end blocks: end it here
                self.out.extend_from_slice(&[0u8; BLOCK * 2]);
                self.done = true;
                return Ok(());
            }
            if header.iter().all(|b| *b == 0) {
                // the end of the archive: two zero blocks, nothing after
                self.out.extend_from_slice(&[0u8; BLOCK * 2]);
                self.done = true;
                return Ok(());
            }
            let size = octal(&header[124..136])?;
            let padded = size.div_ceil(BLOCK as u64) * BLOCK as u64;
            let flag = header[156];
            if matches!(flag, b'x' | b'L' | b'K' | b'g') {
                if padded > EXTENDED_MAX {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "the image tar has an extended header over 1 MiB",
                    ));
                }
                let mut data = vec![0u8; padded as usize];
                if !self.block(&mut data)? {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                let body = &data[..size as usize];
                match flag {
                    b'x' => {
                        if let Some(path) = pax_path(body) {
                            long_name = Some(path);
                        }
                    }
                    b'L' => long_name = Some(text(body)),
                    _ => {}
                }
                pending.extend_from_slice(&header);
                pending.extend_from_slice(&data);
                continue;
            }
            let path = long_name.take().unwrap_or_else(|| ustar_name(&header));
            // a link or directory carries no data, whatever its size says
            let data = if matches!(flag, b'1' | b'2' | b'3' | b'4' | b'5' | b'6') {
                0
            } else {
                padded
            };
            if dropped(&path, flag) {
                return self.skip(data);
            }
            self.out.extend_from_slice(&pending);
            self.out.extend_from_slice(&header);
            self.copying = data;
            return Ok(());
        }
    }
}

impl<R: Read> Read for Filtered<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at < self.out.len() {
                let n = (self.out.len() - self.at).min(buffer.len());
                buffer[..n].copy_from_slice(&self.out[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            self.out.clear();
            self.at = 0;
            if self.copying > 0 {
                let wanted = (buffer.len() as u64).min(self.copying) as usize;
                let n = self.input.read(&mut buffer[..wanted])?;
                if n == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                self.copying -= n as u64;
                return Ok(n);
            }
            if self.done || buffer.is_empty() {
                return Ok(0);
            }
            self.next_entry()?;
        }
    }
}

/// A header's name: ustar `prefix/name`, or the name alone.
fn ustar_name(header: &[u8; BLOCK]) -> String {
    let name = text(&header[0..100]);
    if &header[257..262] == b"ustar" {
        let prefix = text(&header[345..500]);
        if !prefix.is_empty() {
            return format!("{prefix}/{name}");
        }
    }
    name
}

/// The `path` record of a pax header.
fn pax_path(body: &[u8]) -> Option<String> {
    let mut rest = body;
    while !rest.is_empty() {
        let space = rest.iter().position(|b| *b == b' ')?;
        let length: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        if length <= space || length > rest.len() {
            return None;
        }
        let record = &rest[space + 1..length];
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        if let Some(path) = record.strip_prefix(b"path=") {
            return Some(String::from_utf8_lossy(path).into_owned());
        }
        rest = &rest[length..];
    }
    None
}

/// Bytes up to the first NUL, as text.
fn text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// An octal header number, or GNU's base-256 form.
fn octal(field: &[u8]) -> io::Result<u64> {
    if field[0] & 0x80 != 0 {
        let mut n: u64 = u64::from(field[0] & 0x7f);
        for b in &field[1..] {
            n = n
                .checked_mul(256)
                .and_then(|n| n.checked_add(u64::from(*b)))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "tar size"))?;
        }
        return Ok(n);
    }
    let digits: String = field
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as char)
        .collect();
    let digits = digits.trim();
    if digits.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(digits, 8)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "tar header number"))
}
