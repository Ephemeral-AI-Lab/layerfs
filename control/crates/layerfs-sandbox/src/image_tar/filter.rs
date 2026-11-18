//! Streaming tar entry filter (`Filtered`) and filter report (`TarReport`),
//! structured in three phases:
//! 1. Extended-header accumulation and size-independent validation (`x`, `L`, `K`, `g`).
//! 2. Entry-header resolution, entry-size-bounded descriptor construction, and checks.
//! 3. Archive end-block / clean EOF termination and streaming payload output.

use layerfs_bridge::control::SdkValue;
use std::collections::HashSet;
use std::io::{self, Read};

use super::header::{
    invalid, octal, padded_size, recompute_checksum, text, ustar_name, write_size_field,
    zero_header_mtime, BLOCK,
};
use super::pax::PendingHeaders;
use super::sparse::{
    read_block, read_gnu_sparse_map, read_pax1_sparse_map, skip_bytes, validate_sparse_chunks,
    SparseStream,
};

/// Maximum logical expanded size (`realsize` and cumulative sparse output) permitted.
pub const SPARSE_MAX: u64 = 16 << 30;

/// Paths the init layer of `docker create` adds or replaces.
const INIT_FILES: [&str; 5] = [
    ".dockerenv",
    "etc/hosts",
    "etc/hostname",
    "etc/resolv.conf",
    "etc/mtab",
];

/// The directories `docker create`'s init layer creates or adds entries to:
/// the root, `dev`, `etc`, `proc` and `sys`. Their modification times are the
/// container's creation time, not the image's; every other directory keeps its
/// own.
const INIT_DIRS: [&str; 6] = ["", ".", "dev", "etc", "proc", "sys"];

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

    /// `machine.load`'s `dropped` (`fifos`, `devices` and `init` paths left
    /// out) and `sparse` (each expanded file and the bytes in all) fields.
    pub fn to_value(&self) -> SdkValue {
        let paths = |list: &[String]| SdkValue::List(list.iter().map(SdkValue::text).collect());
        let int = |n: u64| SdkValue::Int(n.min(i64::MAX as u64) as i64);
        let files = self.sparse_files.iter().map(|file| {
            SdkValue::map([
                ("path", SdkValue::text(&file.path)),
                ("expanded_bytes", int(file.expanded_bytes)),
                ("stored_bytes", int(file.stored_bytes)),
            ])
        });
        SdkValue::map([
            (
                "dropped",
                SdkValue::map([
                    ("fifos", paths(&self.fifos)),
                    ("devices", paths(&self.devices)),
                    ("init", paths(&self.init)),
                ]),
            ),
            (
                "sparse",
                SdkValue::map([
                    ("files", SdkValue::List(files.collect())),
                    ("expanded_bytes", int(self.sparse_expanded_bytes)),
                ]),
            ),
        ])
    }
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
    sparse_max: u64,
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
            sparse_max: SPARSE_MAX,
            dropped_paths: HashSet::new(),
            report: TarReport::default(),
            done: false,
        }
    }

    /// Sets the maximum expanded sparse byte budget (per entry and cumulative).
    pub fn with_sparse_max(mut self, max: u64) -> Self {
        self.sparse_max = max;
        self
    }

    /// Report of entries dropped or sparse files expanded so far.
    pub fn report(&self) -> &TarReport {
        &self.report
    }

    /// Consumes the filter and returns its [`TarReport`].
    pub fn into_report(self) -> TarReport {
        self.report
    }

    /// Decides the next entry: fills `out` with its headers when it is kept
    /// (its data then follows through `copying` or `sparse`), or skips it.
    fn next_entry(&mut self) -> io::Result<()> {
        let mut pending = PendingHeaders::default();

        // Stage 1 & Stage 3: consume extended headers (`x`, `L`, `K`, `g`),
        // running size-independent validation and caps on arrival, until
        // either archive end / clean EOF (Stage 3) or a real entry header.
        let (mut header, ustar_size, ustar_padded, flag) = loop {
            let mut header = [0u8; BLOCK];
            if !read_block(&mut self.input, &mut header)? {
                pending.finish_at_end()?;
                // Stage 3a: clean EOF without end blocks (any bad sparse PAX in
                // `pending` already errored during Stage 1, and final sparse
                // `size` state is checked above).
                self.out.extend_from_slice(&[0u8; BLOCK * 2]);
                self.done = true;
                return Ok(());
            }
            if header.iter().all(|b| *b == 0) {
                pending.finish_at_end()?;
                // Stage 3b: two zero end blocks, nothing after (same Stage-1
                // and final sparse `size` invariants as clean EOF).
                self.out.extend_from_slice(&[0u8; BLOCK * 2]);
                self.done = true;
                return Ok(());
            }
            let size = octal(&header[124..136])?;
            let padded = padded_size(size)?;
            let flag = header[156];
            if matches!(flag, b'x' | b'L' | b'K' | b'g') {
                pending.admit_header(padded)?;
                let mut data = vec![0u8; padded as usize];
                if !read_block(&mut self.input, &mut data)? {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "unexpected EOF inside extended tar header",
                    ));
                }
                pending.push(header, data, size)?;
                continue;
            }
            break (header, size, padded, flag);
        };

        // Stage 2: entry header arrived — effective entry size is now known.
        let (size, padded) = pending.resolve_entry_size(ustar_size, ustar_padded, flag)?;
        pending.ensure_chunk_count_within(size)?;
        let gnu_chunks = if flag == b'S' {
            Some(read_gnu_sparse_map(&mut self.input, &header, size)?)
        } else {
            None
        };

        let pax_path = pending.pax.path.take();
        let pax_link = pending.pax.linkpath.take();
        let sparse_name = pending.pax.sparse_name.take();
        let path = sparse_name
            .clone()
            .or(pax_path)
            .or_else(|| pending.long_name.take())
            .unwrap_or_else(|| ustar_name(&header));
        // A link or directory carries no data, whatever its size says.
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
            return skip_bytes(&mut self.input, data);
        }

        if flag == b'1' {
            let link_target = pax_link
                .or_else(|| pending.long_link.take())
                .unwrap_or_else(|| text(&header[157..257]));
            let target_norm = clean_path(&link_target);
            if self.dropped_paths.contains(&target_norm) || dropped(&target_norm, b'0') {
                let entry_norm = clean_path(&path);
                return Err(invalid(format!(
                    "hardlink `{entry_norm}` points to dropped target `{target_norm}`"
                )));
            }
        }

        let is_sparse = flag == b'S' || (matches!(flag, b'0' | b'\0') && pending.pax.has_sparse);
        if is_sparse {
            let realsize = if flag == b'S' {
                pending
                    .pax
                    .sparse_realsize
                    .unwrap_or(octal(&header[483..495])?)
            } else {
                pending
                    .pax
                    .sparse_realsize
                    .ok_or_else(|| invalid("PAX sparse file missing realsize"))?
            };
            if realsize > self.sparse_max
                || self
                    .report
                    .sparse_expanded_bytes
                    .checked_add(realsize)
                    .is_none_or(|total| total > self.sparse_max)
            {
                return Err(invalid("sparse entry realsize exceeds maximum"));
            }
            let (raw_chunks, map_bytes) =
                if pending.pax.sparse_major == Some(1) && pending.pax.sparse_minor == Some(0) {
                    read_pax1_sparse_map(&mut self.input, size)?
                } else if let Some(chunks) = gnu_chunks {
                    (chunks, 0)
                } else {
                    (pending.collect_sparse_chunks(size)?, 0)
                };
            let (chunks, stored_bytes) = validate_sparse_chunks(raw_chunks, realsize)?;
            let payload_needed = map_bytes
                .checked_add(stored_bytes)
                .ok_or_else(|| invalid("sparse payload overflow"))?;
            if payload_needed != size {
                return Err(invalid("sparse payload size does not match tar entry size"));
            }
            let input_skip = padded - payload_needed;
            pending.emit_for_sparse(sparse_name.as_deref(), &mut self.out)?;
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
                skip_bytes(&mut self.input, input_skip)?;
                return Ok(());
            }
            self.sparse = Some(SparseStream::new(chunks, realsize, out_pad, input_skip));
            return Ok(());
        }

        if flag == b'5' && INIT_DIRS.contains(&clean_path(&path).as_str()) {
            // the init layer stamps these at container creation: their mtime is
            // set to 0 so exports >=2 s apart stay byte-identical
            pending.emit_for_directory(&mut self.out)?;
            zero_header_mtime(&mut header);
        } else {
            pending.emit_passthrough(&mut self.out);
        }
        self.out.extend_from_slice(&header);
        self.copying = data;
        Ok(())
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
                match st.read_chunk(&mut self.input, buffer)? {
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
