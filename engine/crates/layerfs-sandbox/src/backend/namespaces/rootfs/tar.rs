//! The tar format as far as an image archive needs it: one header at a time,
//! the entry's data read through the same handle, everything else skipped by
//! seeking. Paths are checked here, so nothing downstream sees one that could
//! leave the root.
use super::{refuse, PATH_MAX};
use crate::RuntimeError;
use std::{
    ffi::OsStr,
    io::{self, Read, Seek, SeekFrom},
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

/// A tar block, and the header's own size.
const BLOCK: u64 = 512;
/// The largest pax or GNU extended header accepted.
const EXTENDED_MAX: usize = 1 << 20;

/// One entry of a tar archive, its path already checked to be a plain relative
/// path inside the root.
#[derive(Debug)]
pub(super) struct Entry {
    pub(super) path: PathBuf,
    pub(super) kind: Kind,
    pub(super) mode: u32,
    pub(super) size: u64,
}

/// What an entry places in the root. Every other tar entry type (device
/// nodes, FIFOs, …) is refused when the header is read.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Regular,
    Directory,
    Symlink(PathBuf),
    HardLink(PathBuf),
}

/// A streaming tar reader. Nothing is collected: an entry's bytes are streamed
/// to where they go, and skipped by seeking when the entry is not wanted.
pub(super) struct Tape<R> {
    input: R,
    /// Bytes read from the start of this archive.
    position: u64,
    /// Bytes of the current entry's data not read yet.
    left: u64,
    /// Padding after that data, to the next block.
    padding: u64,
    /// Where the current entry's data starts.
    data_offset: u64,
    /// A pax or GNU name waiting for the entry it belongs to.
    path: Option<PathBuf>,
    /// A pax or GNU link target waiting for the same entry.
    link: Option<PathBuf>,
}

impl<R: Read + Seek> Tape<R> {
    pub(super) fn new(input: R) -> Self {
        Self {
            input,
            position: 0,
            left: 0,
            padding: 0,
            data_offset: 0,
            path: None,
            link: None,
        }
    }

    /// Where the current entry's bytes start.
    pub(super) fn data_offset(&self) -> u64 {
        self.data_offset
    }

    /// The next entry, or `None` at the archive's end. Extended headers (pax
    /// `x`, GNU `L`/`K`) are applied to the entry that follows them; pax
    /// global headers and unknown fixed fields are read past.
    pub(super) fn next(&mut self) -> Result<Option<Entry>, RuntimeError> {
        loop {
            self.skip_data()?;
            let Some(header) = self.header()? else {
                return Ok(None);
            };
            let kind = header[156];
            let size = number(&header[124..136])?;
            self.left = size;
            self.padding = (BLOCK - size % BLOCK) % BLOCK;
            self.data_offset = self.position;
            match kind {
                b'x' => {
                    let (path, link) = pax(&self.data(EXTENDED_MAX)?)?;
                    if path.is_some() {
                        self.path = path;
                    }
                    if link.is_some() {
                        self.link = link;
                    }
                    continue;
                }
                b'g' => continue,
                b'L' => {
                    self.path = Some(long_name(&self.data(PATH_MAX)?)?);
                    continue;
                }
                b'K' => {
                    self.link = Some(long_name(&self.data(PATH_MAX)?)?);
                    continue;
                }
                _ => {}
            }
            let path = match self.path.take() {
                Some(path) => path,
                None => own_name(&header)?,
            };
            let link = match self.link.take() {
                Some(link) => link,
                None => link_name(&header)?,
            };
            let kind = kind_of(kind, link)?;
            return Ok(Some(Entry {
                path,
                kind,
                mode: u32::try_from(number(&header[100..108])?).unwrap_or(0o644),
                size,
            }));
        }
    }

    /// One block of header, or `None` at the end of the archive (an all-zero
    /// block, or nothing left).
    fn header(&mut self) -> Result<Option<[u8; 512]>, RuntimeError> {
        let mut header = [0u8; 512];
        let read = self.fill(&mut header)?;
        if read == 0 || header.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        if read < header.len() {
            return Err(refuse("the image archive's header is truncated"));
        }
        if checksum(&header) != number(&header[148..156])? {
            return Err(refuse("the image archive's header checksum"));
        }
        Ok(Some(header))
    }

    /// Reads the current entry's data (at most `max` bytes) and returns it.
    fn data(&mut self, max: usize) -> Result<Vec<u8>, RuntimeError> {
        if self.left > max as u64 {
            return Err(refuse("the image archive's extended header is too long"));
        }
        let left = self.left;
        let mut bytes = Vec::new();
        let mut source = Read::by_ref(self).take(left);
        source.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Skips whatever is left of the current entry and its padding.
    fn skip_data(&mut self) -> Result<(), RuntimeError> {
        let skip = (self.left)
            .checked_add(self.padding)
            .ok_or_else(|| refuse("the image archive's entry length"))?;
        self.left = 0;
        self.padding = 0;
        if skip == 0 {
            return Ok(());
        }
        let distance =
            i64::try_from(skip).map_err(|_| refuse("the image archive's entry length"))?;
        self.input.seek(SeekFrom::Current(distance))?;
        self.position += skip;
        Ok(())
    }

    /// Fills `buffer` as far as the input allows, returning how many bytes were
    /// read (zero only at the end).
    fn fill(&mut self, buffer: &mut [u8]) -> Result<usize, RuntimeError> {
        let mut filled = 0;
        while filled < buffer.len() {
            let read = self.input.read(&mut buffer[filled..])?;
            if read == 0 {
                break;
            }
            filled += read;
            self.position += read as u64;
        }
        Ok(filled)
    }
}

impl<R: Read> Read for Tape<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let wanted = (buffer.len() as u64).min(self.left) as usize;
        let read = self.input.read(&mut buffer[..wanted])?;
        self.left -= read as u64;
        self.position += read as u64;
        Ok(read)
    }
}

/// The entry's own name: its `name` and, in a ustar archive, its `prefix`.
fn own_name(header: &[u8; 512]) -> Result<PathBuf, RuntimeError> {
    let name = text(&header[0..100]);
    let prefix = if &header[257..262] == b"ustar".as_slice() {
        text(&header[345..500])
    } else {
        &[]
    };
    let mut bytes = Vec::with_capacity(prefix.len() + 1 + name.len());
    if !prefix.is_empty() {
        bytes.extend_from_slice(prefix);
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name);
    checked(&bytes)
}

/// The entry's own link target, for the two kinds that carry one.
fn link_name(header: &[u8; 512]) -> Result<PathBuf, RuntimeError> {
    Ok(PathBuf::from(OsStr::from_bytes(text(&header[157..257]))))
}

/// The entry's kind, refusing every type this runtime does not extract.
fn kind_of(flag: u8, link: PathBuf) -> Result<Kind, RuntimeError> {
    match flag {
        b'0' | b'\0' | b'7' => Ok(Kind::Regular),
        b'5' => Ok(Kind::Directory),
        b'2' => Ok(Kind::Symlink(link)),
        b'1' => Ok(Kind::HardLink(checked(link.as_os_str().as_bytes())?)),
        b'3' | b'4' => Err(refuse(
            "the image archive holds a device node, which a Sandbox root does not extract",
        )),
        b'6' => Err(refuse(
            "the image archive holds a FIFO, which a Sandbox root does not extract",
        )),
        _ => Err(refuse(
            "the image archive holds an entry type it does not extract",
        )),
    }
}

/// A path checked to be plain and relative: no `/` at its start, no `.` or
/// `..` component, at most one kernel path's worth of bytes.
fn checked(bytes: &[u8]) -> Result<PathBuf, RuntimeError> {
    let path = Path::new(OsStr::from_bytes(bytes));
    if bytes.len() > PATH_MAX || bytes.contains(&0) {
        return Err(refuse(
            "the image archive's path is not a safe relative path",
        ));
    }
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => cleaned.push(name),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(refuse(
                    "the image archive's path is absolute or holds a `..` component",
                ))
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        return Err(refuse("the image archive's path is empty"));
    }
    Ok(cleaned)
}

/// A manifest entry's path, from the UTF-8 text the archive's JSON holds.
pub(super) fn path_of(bytes: &[u8]) -> Result<PathBuf, RuntimeError> {
    std::str::from_utf8(bytes)
        .map_err(|_| refuse("the image archive's manifest is not UTF-8"))
        .and_then(|text| checked(text.as_bytes()))
}

/// A pax extended header's `path` and `linkpath` records, if it has them.
fn pax(data: &[u8]) -> Result<(Option<PathBuf>, Option<PathBuf>), RuntimeError> {
    let (mut path, mut link) = (None, None);
    let mut rest = data;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or_else(|| refuse("the image archive's pax record"))?;
        let length: usize = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|text| text.parse().ok())
            .filter(|length: &usize| *length > space + 1 && *length <= rest.len())
            .ok_or_else(|| refuse("the image archive's pax record length"))?;
        let record = &rest[space + 1..length - 1];
        if let Some(equals) = record.iter().position(|byte| *byte == b'=') {
            let (key, value) = (&record[..equals], &record[equals + 1..]);
            match key {
                b"path" => path = Some(checked(value)?),
                b"linkpath" => link = Some(checked(value)?),
                _ => {}
            }
        }
        rest = &rest[length..];
    }
    Ok((path, link))
}

/// A GNU long name's data: the name, up to its first NUL.
fn long_name(data: &[u8]) -> Result<PathBuf, RuntimeError> {
    let name = match data.iter().position(|byte| *byte == 0) {
        Some(end) => &data[..end],
        None => data,
    };
    checked(name)
}

/// A header's NUL- or space-padded octal (or GNU base-256) number.
fn number(field: &[u8]) -> Result<u64, RuntimeError> {
    if let Some(first) = field.first() {
        if first & 0x80 != 0 {
            let mut value = u64::from(first & 0x7f);
            for byte in &field[1..] {
                value = (value)
                    .checked_mul(256)
                    .and_then(|value| value.checked_add(u64::from(*byte)))
                    .ok_or_else(|| refuse("the image archive's number"))?;
            }
            return Ok(value);
        }
    }
    let mut value = 0u64;
    for byte in field {
        match byte {
            b'0'..=b'7' => {
                value = (value)
                    .checked_mul(8)
                    .and_then(|value| value.checked_add(u64::from(*byte - b'0')))
                    .ok_or_else(|| refuse("the image archive's number"))?;
            }
            b' ' | 0 => {}
            _ => return Err(refuse("the image archive's number")),
        }
    }
    Ok(value)
}

/// A header's checksum: the sum of its bytes with the checksum field itself
/// read as spaces.
fn checksum(header: &[u8; 512]) -> u64 {
    (header.iter().enumerate())
        .map(|(index, byte)| match index {
            148..=155 => u64::from(b' '),
            _ => u64::from(*byte),
        })
        .sum()
}

/// `field` up to its first NUL, trimmed of trailing spaces.
fn text(field: &[u8]) -> &[u8] {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    let text = &field[..end];
    let end = (text.iter().rposition(|byte| *byte != b' ')).map_or(0, |index| index + 1);
    &text[..end]
}
