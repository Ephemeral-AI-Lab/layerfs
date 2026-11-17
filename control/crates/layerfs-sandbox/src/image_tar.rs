//! The flat root filesystem tar a Sandbox image is copied as: `docker
//! export`'s output with the files `docker create`'s init layer adds taken
//! out (`.dockerenv`, `etc/hosts`, `etc/hostname`, `etc/resolv.conf`,
//! `etc/mtab`, everything under `dev/`), every device node and FIFO (which
//! the unpacker refuses), hardlinks targeting dropped entries refused clearly,
//! sparse entries expanded to regular files, and directory modification times
//! normalized to 0 with recomputed header checksums. Those are the bytes that
//! differ between two exports of one image; what is left is the image's own
//! tree. The filter streams: it holds at most one entry's extended headers and
//! sparse chunk map, never a file's bytes.
use std::collections::HashSet;
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

fn clean_path(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_owned()
}

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

/// One sparse tar entry expanded into a regular file (`b'0'`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseReport {
    /// Entry path with `./` and leading `/` removed.
    pub path: String,
    /// Expanded logical size in bytes.
    pub expanded_bytes: u64,
    /// Stored sparse payload bytes read from the source tar.
    pub stored_bytes: u64,
}

/// Report of dropped entries and expanded sparse files seen while filtering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TarReport {
    /// Paths of dropped FIFO (`b'6'`) entries.
    pub fifos: Vec<String>,
    /// Paths of dropped character/block device (`b'3'`, `b'4'`) entries.
    pub devices: Vec<String>,
    /// Paths of dropped container init entries (`.dockerenv`, `/etc/hosts`, `dev/*`, ...).
    pub init: Vec<String>,
    /// Details of every sparse entry expanded while streaming.
    pub sparse_files: Vec<SparseReport>,
    /// Total expanded logical bytes across all sparse entries.
    pub sparse_expanded_bytes: u64,
}

impl TarReport {
    /// Number of dropped FIFO entries.
    pub fn fifo_count(&self) -> usize {
        self.fifos.len()
    }

    /// Number of dropped device entries.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Number of dropped init/runtime entries.
    pub fn init_count(&self) -> usize {
        self.init.len()
    }
}

struct SparseStream {
    chunks: Vec<(u64, u64)>,
    chunk_idx: usize,
    chunk_left: u64,
    pos: u64,
    realsize: u64,
    out_pad: u64,
    input_skip: u64,
}

/// A reader of `input` (a tar stream) that yields the same tar without the
/// [`dropped`] entries and with sparse entries expanded to regular files.
pub struct Filtered<R> {
    input: R,
    /// Bytes ready to hand out.
    out: Vec<u8>,
    at: usize,
    /// Bytes of the current kept entry's data (with padding) still to copy.
    copying: u64,
    sparse: Option<SparseStream>,
    dropped_paths: HashSet<String>,
    report: TarReport,
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
            sparse: None,
            dropped_paths: HashSet::new(),
            report: TarReport::default(),
            done: false,
        }
    }

    /// Report of entries dropped or sparse files expanded so far.
    pub fn report(&self) -> &TarReport {
        &self.report
    }

    /// Consumes the filter and returns its [`TarReport`].
    pub fn into_report(self) -> TarReport {
        self.report
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
        if count == 0 {
            return Ok(());
        }
        let copied = io::copy(&mut (&mut self.input).take(count), &mut io::sink())?;
        if copied != count {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }

    /// Reads GNU oldgnu extended sparse header blocks that follow a `b'S'` header.
    fn read_gnu_sparse_map(&mut self, header: &[u8; BLOCK]) -> io::Result<Vec<(u64, u64)>> {
        let mut chunks = Vec::new();
        for i in 0..4 {
            let base = 386 + i * 24;
            let off = octal(&header[base..base + 12])?;
            let len = octal(&header[base + 12..base + 24])?;
            if len > 0 {
                chunks.push((off, len));
            }
        }
        let mut extended = header[482] != 0;
        while extended {
            let mut ext = [0u8; BLOCK];
            if !self.block(&mut ext)? {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            for slot in 0..21 {
                let base = slot * 24;
                let off = octal(&ext[base..base + 12])?;
                let len = octal(&ext[base + 12..base + 24])?;
                if len > 0 {
                    chunks.push((off, len));
                }
            }
            extended = ext[504] != 0;
        }
        Ok(chunks)
    }

    /// Reads the PAX `1.0` inline newline-delimited sparse map (512-byte padded)
    /// from the beginning of the entry's payload, returning `(chunks, map_bytes)`.
    fn read_pax1_sparse_map(&mut self, max_bytes: u64) -> io::Result<(Vec<(u64, u64)>, u64)> {
        let mut raw = Vec::new();
        let mut num_chunks: Option<usize> = None;
        let mut needed_lines: usize = 1;
        loop {
            if raw.len() as u64 + BLOCK as u64 > max_bytes.min(EXTENDED_MAX) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "PAX sparse map exceeds entry size or extended header limit",
                ));
            }
            let mut blk = [0u8; BLOCK];
            if !self.block(&mut blk)? {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            raw.extend_from_slice(&blk);
            if num_chunks.is_none() {
                if let Some(nl) = raw.iter().position(|b| *b == b'\n') {
                    let count: usize = std::str::from_utf8(&raw[..nl])
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid PAX sparse chunk count",
                            )
                        })?;
                    needed_lines =
                        1usize.checked_add(count.saturating_mul(2)).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "sparse map overflow")
                        })?;
                    num_chunks = Some(count);
                }
            }
            if num_chunks.is_some() {
                let newlines = raw.iter().filter(|b| **b == b'\n').count();
                if newlines >= needed_lines {
                    break;
                }
            }
        }
        let map_bytes = raw.len() as u64;
        let count = num_chunks.unwrap_or(0);
        let mut lines = raw
            .split(|b| *b == b'\n')
            .take(needed_lines)
            .map(|l| std::str::from_utf8(l).map(str::trim));
        let _ = lines.next();
        let mut chunks = Vec::with_capacity(count);
        for _ in 0..count {
            let off: u64 = lines
                .next()
                .transpose()
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid PAX sparse offset")
                })?;
            let len: u64 = lines
                .next()
                .transpose()
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid PAX sparse length")
                })?;
            if len > 0 {
                chunks.push((off, len));
            }
        }
        Ok((chunks, map_bytes))
    }

    /// Decides the next entry: fills `out` with its headers when it is kept
    /// (its data then follows through `copying` or `sparse`), or skips it.
    fn next_entry(&mut self) -> io::Result<()> {
        let mut pending: Vec<([u8; BLOCK], Vec<u8>)> = Vec::new();
        let mut long_name: Option<String> = None;
        let mut long_link: Option<String> = None;
        let mut pax = PaxMeta::default();
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
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected EOF inside extended tar header",
                    ));
                }
                let body = &data[..size as usize];
                match flag {
                    b'x' => pax.parse_into(body)?,
                    b'L' => long_name = Some(text(body)),
                    b'K' => long_link = Some(text(body)),
                    _ => {}
                }
                pending.push((header, data));
                continue;
            }
            let gnu_chunks = if flag == b'S' {
                Some(self.read_gnu_sparse_map(&header)?)
            } else {
                None
            };
            let pax_path = pax.path.take();
            let pax_link = pax.linkpath.take();
            let sparse_name = pax.sparse_name.take();
            let path = sparse_name
                .clone()
                .or(pax_path)
                .or_else(|| long_name.take())
                .unwrap_or_else(|| ustar_name(&header));
            // a link or directory carries no data, whatever its size says
            let data = if matches!(flag, b'1' | b'2' | b'3' | b'4' | b'5' | b'6') {
                0
            } else {
                padded
            };
            if dropped(&path, flag) {
                let norm = clean_path(&path);
                if matches!(flag, b'6') {
                    self.report.fifos.push(norm.clone());
                } else if matches!(flag, b'3' | b'4') {
                    self.report.devices.push(norm.clone());
                } else {
                    self.report.init.push(norm.clone());
                }
                self.dropped_paths.insert(norm);
                return self.skip(data);
            }
            if flag == b'1' {
                let link_target = pax_link
                    .or_else(|| long_link.take())
                    .unwrap_or_else(|| text(&header[157..257]));
                let target_norm = clean_path(&link_target);
                if self.dropped_paths.contains(&target_norm) || dropped(&target_norm, b'0') {
                    let entry_norm = clean_path(&path);
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("hardlink `{entry_norm}` points to dropped target `{target_norm}`"),
                    ));
                }
            }
            let is_sparse = flag == b'S' || (matches!(flag, b'0' | b'\0') && pax.has_sparse);
            if is_sparse {
                let realsize = if flag == b'S' {
                    pax.sparse_realsize.unwrap_or(octal(&header[483..495])?)
                } else {
                    pax.sparse_realsize.ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "PAX sparse file missing realsize",
                        )
                    })?
                };
                let (raw_chunks, map_bytes) =
                    if pax.sparse_major == Some(1) && pax.sparse_minor == Some(0) {
                        self.read_pax1_sparse_map(size)?
                    } else if let Some(chunks) = gnu_chunks {
                        (chunks, 0)
                    } else {
                        (pax.sparse_chunks, 0)
                    };
                let (chunks, stored_bytes) = validate_sparse_chunks(raw_chunks, realsize)?;
                let payload_needed = map_bytes.checked_add(stored_bytes).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "sparse payload overflow")
                })?;
                if payload_needed > size {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "sparse chunks exceed tar entry size",
                    ));
                }
                let input_skip = padded - payload_needed;
                let override_path = sparse_name.as_deref();
                emit_sparse_pax_headers(&mut pending, override_path, &mut self.out)?;
                let is_oldgnu = flag == b'S' || &header[257..263] == b"ustar ";
                header[156] = b'0';
                if is_oldgnu {
                    header[257..263].copy_from_slice(b"ustar\0");
                    header[263..265].copy_from_slice(b"00");
                    header[329..512].fill(0);
                }
                write_size_field(&mut header[124..136], realsize)?;
                recompute_checksum(&mut header);
                self.out.extend_from_slice(&header);
                self.report.sparse_files.push(SparseReport {
                    path: clean_path(&path),
                    expanded_bytes: realsize,
                    stored_bytes,
                });
                self.report.sparse_expanded_bytes =
                    self.report.sparse_expanded_bytes.saturating_add(realsize);
                let out_pad = (BLOCK as u64 - (realsize % BLOCK as u64)) % BLOCK as u64;
                if realsize == 0 {
                    self.skip(input_skip)?;
                    return Ok(());
                }
                self.sparse = Some(SparseStream {
                    chunks,
                    chunk_idx: 0,
                    chunk_left: 0,
                    pos: 0,
                    realsize,
                    out_pad,
                    input_skip,
                });
                return Ok(());
            }
            if flag == b'5' {
                for (mut ext_hdr, ext_data) in pending {
                    if ext_hdr[156] == b'x' {
                        if let Some((mut h, d)) = strip_pax_times(ext_hdr, ext_data)? {
                            h[136..148].copy_from_slice(b"00000000000\0");
                            recompute_checksum(&mut h);
                            self.out.extend_from_slice(&h);
                            self.out.extend_from_slice(&d);
                        }
                    } else {
                        if matches!(ext_hdr[156], b'L' | b'K') {
                            ext_hdr[136..148].copy_from_slice(b"00000000000\0");
                            recompute_checksum(&mut ext_hdr);
                        }
                        self.out.extend_from_slice(&ext_hdr);
                        self.out.extend_from_slice(&ext_data);
                    }
                }
                // `docker create`'s init layer touches directories such as `dev/`
                // and `etc/` at container creation time; normalize every kept
                // directory's mtime so exports >=2 s apart stay byte-identical.
                header[136..148].copy_from_slice(b"00000000000\0");
                recompute_checksum(&mut header);
            } else {
                for (ext_hdr, ext_data) in pending {
                    self.out.extend_from_slice(&ext_hdr);
                    self.out.extend_from_slice(&ext_data);
                }
            }
            self.out.extend_from_slice(&header);
            self.copying = data;
            return Ok(());
        }
    }

    fn read_sparse(
        &mut self,
        st: &mut SparseStream,
        buffer: &mut [u8],
    ) -> io::Result<Option<usize>> {
        loop {
            if st.chunk_left > 0 {
                let want = (buffer.len() as u64).min(st.chunk_left) as usize;
                let n = self.input.read(&mut buffer[..want])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected EOF in sparse tar chunk",
                    ));
                }
                st.chunk_left -= n as u64;
                st.pos += n as u64;
                if st.chunk_left == 0 {
                    st.chunk_idx += 1;
                }
                return Ok(Some(n));
            }
            if st.pos < st.realsize {
                let hole_end = st
                    .chunks
                    .get(st.chunk_idx)
                    .map_or(st.realsize, |(off, _)| *off);
                if st.pos < hole_end {
                    let zeros = (hole_end - st.pos).min(buffer.len() as u64) as usize;
                    buffer[..zeros].fill(0);
                    st.pos += zeros as u64;
                    if st.pos == hole_end {
                        if let Some((_, len)) = st.chunks.get(st.chunk_idx) {
                            st.chunk_left = *len;
                        }
                    }
                    return Ok(Some(zeros));
                }
                if let Some((_, len)) = st.chunks.get(st.chunk_idx) {
                    st.chunk_left = *len;
                    continue;
                }
            }
            let to_skip = std::mem::take(&mut st.input_skip);
            if to_skip > 0 {
                self.skip(to_skip)?;
            }
            if st.out_pad > 0 {
                let zeros = st.out_pad.min(buffer.len() as u64) as usize;
                buffer[..zeros].fill(0);
                st.out_pad -= zeros as u64;
                return Ok(Some(zeros));
            }
            return Ok(None);
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
            if buffer.is_empty() {
                return Ok(0);
            }
            if self.copying > 0 {
                let wanted = (buffer.len() as u64).min(self.copying) as usize;
                let n = self.input.read(&mut buffer[..wanted])?;
                if n == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                self.copying -= n as u64;
                return Ok(n);
            }
            if let Some(mut st) = self.sparse.take() {
                match self.read_sparse(&mut st, buffer)? {
                    Some(n) => {
                        self.sparse = Some(st);
                        return Ok(n);
                    }
                    None => continue,
                }
            }
            if self.done {
                return Ok(0);
            }
            self.next_entry()?;
        }
    }
}

#[derive(Default)]
struct PaxMeta {
    path: Option<String>,
    linkpath: Option<String>,
    has_sparse: bool,
    sparse_major: Option<u64>,
    sparse_minor: Option<u64>,
    sparse_name: Option<String>,
    sparse_realsize: Option<u64>,
    sparse_chunks: Vec<(u64, u64)>,
}

fn walk_pax_records(
    body: &[u8],
    mut visit: impl FnMut(&[u8], &[u8], &[u8]) -> io::Result<()>,
) -> io::Result<bool> {
    let mut rest = body;
    while !rest.is_empty() {
        let Some(space) = rest.iter().position(|b| *b == b' ') else {
            return Ok(false);
        };
        let Some(length) = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|len| *len > space + 1 && *len <= rest.len())
        else {
            return Ok(false);
        };
        let record = &rest[space + 1..length];
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        let (key, val) = match record.iter().position(|b| *b == b'=') {
            Some(eq) => (&record[..eq], &record[eq + 1..]),
            None => (record, &b""[..]),
        };
        visit(&rest[..length], key, val)?;
        rest = &rest[length..];
    }
    Ok(true)
}

impl PaxMeta {
    fn parse_into(&mut self, body: &[u8]) -> io::Result<()> {
        let mut pending_offset: Option<u64> = None;
        let _ = walk_pax_records(body, |_, key, val| {
            let val_str = String::from_utf8_lossy(val);
            let parse_u64 = |label: &str| {
                val_str.trim().parse::<u64>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("invalid pax {label}"))
                })
            };
            match key {
                b"path" => self.path = Some(val_str.into_owned()),
                b"linkpath" => self.linkpath = Some(val_str.into_owned()),
                b"GNU.sparse.major" => {
                    self.has_sparse = true;
                    self.sparse_major = Some(parse_u64("GNU.sparse.major")?);
                }
                b"GNU.sparse.minor" => {
                    self.has_sparse = true;
                    self.sparse_minor = Some(parse_u64("GNU.sparse.minor")?);
                }
                b"GNU.sparse.name" => {
                    self.has_sparse = true;
                    self.sparse_name = Some(val_str.into_owned());
                }
                b"GNU.sparse.realsize" | b"GNU.sparse.size" => {
                    self.has_sparse = true;
                    self.sparse_realsize = Some(parse_u64("GNU.sparse.realsize")?);
                }
                b"GNU.sparse.offset" => {
                    self.has_sparse = true;
                    pending_offset = Some(parse_u64("GNU.sparse.offset")?);
                }
                b"GNU.sparse.numbytes" => {
                    self.has_sparse = true;
                    let off = pending_offset.take().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "GNU.sparse.numbytes without offset",
                        )
                    })?;
                    let len = parse_u64("GNU.sparse.numbytes")?;
                    if len > 0 {
                        self.sparse_chunks.push((off, len));
                    }
                }
                b"GNU.sparse.map" => {
                    self.has_sparse = true;
                    let mut parts = val_str.split(',').filter(|s| !s.is_empty());
                    while let Some(o_str) = parts.next() {
                        let l_str = parts.next().ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "odd item count in GNU.sparse.map",
                            )
                        })?;
                        let off: u64 = o_str.trim().parse().map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid sparse map offset")
                        })?;
                        let len: u64 = l_str.trim().parse().map_err(|_| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid sparse map len")
                        })?;
                        if len > 0 {
                            self.sparse_chunks.push((off, len));
                        }
                    }
                }
                _ => {
                    if key.starts_with(b"GNU.sparse.") {
                        self.has_sparse = true;
                    }
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn validate_sparse_chunks(
    chunks: Vec<(u64, u64)>,
    realsize: u64,
) -> io::Result<(Vec<(u64, u64)>, u64)> {
    let mut prev_end = 0u64;
    let mut stored_bytes = 0u64;
    let mut valid = Vec::with_capacity(chunks.len());
    for (off, len) in chunks {
        if off > realsize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse chunk starts past realsize",
            ));
        }
        if len == 0 {
            continue;
        }
        if off < prev_end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse chunks overlap or are out of order",
            ));
        }
        let end = off.checked_add(len).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "sparse chunk end overflow")
        })?;
        if end > realsize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse chunk extends past realsize",
            ));
        }
        stored_bytes = stored_bytes.checked_add(len).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "sparse byte count overflow")
        })?;
        prev_end = end;
        valid.push((off, len));
    }
    Ok((valid, stored_bytes))
}

fn emit_sparse_pax_headers(
    pending: &mut Vec<([u8; BLOCK], Vec<u8>)>,
    override_path: Option<&str>,
    out: &mut Vec<u8>,
) -> io::Result<()> {
    let mut wrote_path = false;
    let last_pax = pending.iter().rposition(|(h, _)| h[156] == b'x');
    for (idx, (mut ext_hdr, ext_data)) in std::mem::take(pending).into_iter().enumerate() {
        if ext_hdr[156] != b'x' {
            out.extend_from_slice(&ext_hdr);
            out.extend_from_slice(&ext_data);
            continue;
        }
        let size = octal(&ext_hdr[124..136])? as usize;
        let body = &ext_data[..size.min(ext_data.len())];
        let mut kept = strip_pax_keys(body, |k| {
            k.starts_with(b"GNU.sparse.")
                || k == b"size"
                || (override_path.is_some() && k == b"path")
        })?;
        if last_pax == Some(idx) {
            if let Some(real_path) = override_path {
                kept.extend_from_slice(&pax_kv("path", real_path));
                wrote_path = true;
            }
        }
        if kept.is_empty() {
            continue;
        }
        write_octal(&mut ext_hdr[124..136], kept.len() as u64);
        recompute_checksum(&mut ext_hdr);
        let padded = kept.len().div_ceil(BLOCK) * BLOCK;
        kept.resize(padded, 0);
        out.extend_from_slice(&ext_hdr);
        out.extend_from_slice(&kept);
    }
    if let Some(real_path) = override_path.filter(|_| !wrote_path) {
        let mut body = pax_kv("path", real_path);
        let mut hdr = [0u8; BLOCK];
        hdr[0..11].copy_from_slice(b"PaxHeaders/");
        write_octal(&mut hdr[100..108], 0o644);
        write_octal(&mut hdr[124..136], body.len() as u64);
        hdr[156] = b'x';
        hdr[257..263].copy_from_slice(b"ustar\0");
        hdr[263..265].copy_from_slice(b"00");
        recompute_checksum(&mut hdr);
        let padded = body.len().div_ceil(BLOCK) * BLOCK;
        body.resize(padded, 0);
        out.extend_from_slice(&hdr);
        out.extend_from_slice(&body);
    }
    Ok(())
}

fn pax_kv(key: &str, value: &str) -> Vec<u8> {
    let body_len = 1 + key.len() + 1 + value.len() + 1;
    let mut digits = 1usize;
    loop {
        let total = digits + body_len;
        let d = total.to_string().len();
        if d == digits {
            return format!("{total} {key}={value}\n").into_bytes();
        }
        digits = d;
    }
}

fn strip_pax_keys(body: &[u8], mut drop_key: impl FnMut(&[u8]) -> bool) -> io::Result<Vec<u8>> {
    let mut kept = Vec::new();
    walk_pax_records(body, |raw, key, _| {
        if !drop_key(key) {
            kept.extend_from_slice(raw);
        }
        Ok(())
    })?;
    Ok(kept)
}

/// A header's name: ustar `prefix/name`, or the name alone.
fn ustar_name(header: &[u8; BLOCK]) -> String {
    let name = text(&header[0..100]);
    if &header[257..263] == b"ustar\0" {
        let prefix = text(&header[345..500]);
        if !prefix.is_empty() {
            return format!("{prefix}/{name}");
        }
    }
    name
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

/// Drops `mtime`, `atime`, and `ctime` records from a pax extended header
/// block (`flag == b'x'`), rebuilding its header and padded body only if any
/// time record was removed. Returns `None` when no records remain.
fn strip_pax_times(
    mut header: [u8; BLOCK],
    data: Vec<u8>,
) -> io::Result<Option<([u8; BLOCK], Vec<u8>)>> {
    let size = octal(&header[124..136])? as usize;
    let body = &data[..size.min(data.len())];
    let Some(mut kept) = filter_pax_records(body) else {
        return Ok(Some((header, data)));
    };
    if kept.is_empty() {
        return Ok(None);
    }
    write_octal(&mut header[124..136], kept.len() as u64);
    recompute_checksum(&mut header);
    let padded = kept.len().div_ceil(BLOCK) * BLOCK;
    kept.resize(padded, 0);
    Ok(Some((header, kept)))
}

/// Filters `mtime`, `atime`, and `ctime` records out of a pax extended header
/// body; returns `None` when no time record was present or parsing fails.
fn filter_pax_records(body: &[u8]) -> Option<Vec<u8>> {
    let mut kept = Vec::new();
    let mut changed = false;
    let complete = walk_pax_records(body, |raw, key, _| {
        if matches!(key, b"mtime" | b"atime" | b"ctime") {
            changed = true;
        } else {
            kept.extend_from_slice(raw);
        }
        Ok(())
    })
    .ok()?;
    (complete && changed).then_some(kept)
}

fn write_size_field(field: &mut [u8], value: u64) -> io::Result<()> {
    if value <= 0o77_777_777_777 {
        write_octal(field, value);
        return Ok(());
    }
    if field.len() != 12 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "tar size field"));
    }
    let be = value.to_be_bytes();
    field[0] = 0x80;
    field[1..4].fill(0);
    field[4..12].copy_from_slice(&be);
    Ok(())
}

fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let text = format!("{value:0width$o}\0");
    field.copy_from_slice(&text.as_bytes()[..field.len()]);
}

fn recompute_checksum(header: &mut [u8; BLOCK]) {
    header[148..156].fill(b' ');
    let sum: u64 = header.iter().map(|b| u64::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(text.as_bytes());
}
