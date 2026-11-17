//! PAX and GNU extended header accumulation, Stage-1 size-independent
//! validation, Stage-2 entry-sized descriptor extraction, and header emission.

use std::io;

use super::header::{
    invalid, octal, padded_size, recompute_checksum, text, write_octal, zero_header_mtime, BLOCK,
};

/// The largest single pax or GNU extended header kept while its entry is decided.
pub(super) const EXTENDED_MAX: u64 = 1 << 20;
/// Maximum total bytes (`512 + padded` per header) buffered across all extended
/// headers (`x`, `L`, `K`, `g`) of a single tar entry.
///
/// 16 MiB accommodates deep container paths/linkpaths plus large `SCHILY.xattr.*`
/// sets while bounding buffered memory on header floods (even zero-length `x`
/// headers) and exceeding the ~10 MiB PAX text of `SPARSE_CHUNKS_MAX` descriptors.
pub const EXTENDED_TOTAL_MAX: u64 = 16 << 20;
/// Hard cap on parsed sparse chunk descriptors across extended headers.
pub(super) const SPARSE_CHUNKS_MAX: usize = 1 << 20;

#[derive(Default)]
pub(super) struct PaxScan {
    pub(super) path: Option<String>,
    pub(super) linkpath: Option<String>,
    pub(super) pax_size: Option<u64>,
    pub(super) pax_size_bad: bool,
    pub(super) saw_malformed: bool,
    pub(super) has_sparse: bool,
    pub(super) sparse_major: Option<u64>,
    pub(super) sparse_minor: Option<u64>,
    pub(super) sparse_name: Option<String>,
    pub(super) sparse_realsize: Option<u64>,
    pub(super) sparse_chunk_count: usize,
}

#[derive(Default)]
pub(super) struct PendingHeaders {
    raw: Vec<([u8; BLOCK], Vec<u8>)>,
    total_bytes: u64,
    pub(super) long_name: Option<String>,
    pub(super) long_link: Option<String>,
    pub(super) pax: PaxScan,
}

impl PendingHeaders {
    /// Stage 1 admission check: enforces per-header 1 MiB and per-entry 16 MiB
    /// extended-header budgets before allocating the extended header payload.
    pub(super) fn admit_header(&mut self, padded: u64) -> io::Result<()> {
        if padded > EXTENDED_MAX {
            return Err(invalid("the image tar has an extended header over 1 MiB"));
        }
        let next = self
            .total_bytes
            .checked_add((BLOCK as u64) + padded)
            .ok_or_else(|| invalid("the image tar extended headers exceed 16 MiB"))?;
        if next > EXTENDED_TOTAL_MAX {
            return Err(invalid("the image tar extended headers exceed 16 MiB"));
        }
        self.total_bytes = next;
        Ok(())
    }

    /// Stage 1 ingestion: runs size-independent PAX/sparse validation immediately
    /// as each extended header arrives, without building chunk tables yet.
    pub(super) fn push(&mut self, header: [u8; BLOCK], data: Vec<u8>, size: u64) -> io::Result<()> {
        let body = &data[..size as usize];
        match header[156] {
            b'x' => self.pax.scan_header(body)?,
            b'L' => self.long_name = Some(text(body)),
            b'K' => self.long_link = Some(text(body)),
            _ => {}
        }
        self.raw.push((header, data));
        Ok(())
    }

    /// Stage 2 entry size resolution: applies PAX `size` when present (ignoring
    /// bad PAX `size` on plain entries, matching baseline) and checks 512B padding.
    pub(super) fn resolve_entry_size(
        &self,
        ustar_size: u64,
        ustar_padded: u64,
        flag: u8,
    ) -> io::Result<(u64, u64)> {
        if self.pax.pax_size_bad && (flag == b'S' || self.pax.has_sparse) {
            return Err(invalid("invalid pax size"));
        }
        match self.pax.pax_size {
            Some(sz) => Ok((sz, padded_size(sz)?)),
            None => Ok((ustar_size, ustar_padded)),
        }
    }

    /// Stage 2 chunk budget check: verifies the cross-header descriptor count
    /// fits within `min(entry_size, SPARSE_CHUNKS_MAX)` once entry size is known.
    pub(super) fn ensure_chunk_count_within(&self, entry_size: u64) -> io::Result<()> {
        if (self.pax.sparse_chunk_count as u64) > entry_size.min(SPARSE_CHUNKS_MAX as u64) {
            return Err(invalid(
                "PAX sparse chunk count exceeds entry size or limit",
            ));
        }
        Ok(())
    }

    /// Stage 2 descriptor materialization: extracts validated `(offset, len)`
    /// descriptors from buffered `x` headers, bounded by `min(entry_size, SPARSE_CHUNKS_MAX)`.
    pub(super) fn collect_sparse_chunks(&self, entry_size: u64) -> io::Result<Vec<(u64, u64)>> {
        self.ensure_chunk_count_within(entry_size)?;
        let limit = entry_size.min(SPARSE_CHUNKS_MAX as u64);
        let mut chunks = Vec::with_capacity(self.pax.sparse_chunk_count);
        for (ext_hdr, ext_data) in &self.raw {
            if ext_hdr[156] != b'x' {
                continue;
            }
            let ext_size = octal(&ext_hdr[124..136])? as usize;
            let body = &ext_data[..ext_size.min(ext_data.len())];
            let mut pending_offset: Option<u64> = None;
            walk_pax_records(body, |_, key, val| {
                let val_str = String::from_utf8_lossy(val);
                match key {
                    b"GNU.sparse.offset" => {
                        pending_offset = Some(
                            val_str
                                .trim()
                                .parse::<u64>()
                                .map_err(|_| invalid("invalid pax GNU.sparse.offset"))?,
                        );
                    }
                    b"GNU.sparse.numbytes" => {
                        let off = pending_offset
                            .take()
                            .ok_or_else(|| invalid("GNU.sparse.numbytes without offset"))?;
                        let len = val_str
                            .trim()
                            .parse::<u64>()
                            .map_err(|_| invalid("invalid pax GNU.sparse.numbytes"))?;
                        push_chunk(&mut chunks, off, len, limit)?;
                    }
                    b"GNU.sparse.map" => {
                        let mut parts = val_str.split(',').filter(|s| !s.is_empty());
                        while let Some(o_str) = parts.next() {
                            let l_str = parts
                                .next()
                                .ok_or_else(|| invalid("odd item count in GNU.sparse.map"))?;
                            let off: u64 = o_str
                                .trim()
                                .parse()
                                .map_err(|_| invalid("invalid sparse map offset"))?;
                            let len: u64 = l_str
                                .trim()
                                .parse()
                                .map_err(|_| invalid("invalid sparse map len"))?;
                            push_chunk(&mut chunks, off, len, limit)?;
                        }
                    }
                    _ => {}
                }
                Ok(())
            })?;
        }
        Ok(chunks)
    }

    pub(super) fn emit_for_sparse(
        &mut self,
        override_path: Option<&str>,
        out: &mut Vec<u8>,
    ) -> io::Result<()> {
        let mut wrote_path = false;
        let last_pax = self.raw.iter().rposition(|(h, _)| h[156] == b'x');
        for (idx, (mut ext_hdr, ext_data)) in std::mem::take(&mut self.raw).into_iter().enumerate()
        {
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

    pub(super) fn emit_for_directory(self, out: &mut Vec<u8>) -> io::Result<()> {
        for (mut ext_hdr, ext_data) in self.raw {
            if ext_hdr[156] == b'x' {
                if let Some((mut h, d)) = strip_pax_times(ext_hdr, ext_data)? {
                    zero_header_mtime(&mut h);
                    out.extend_from_slice(&h);
                    out.extend_from_slice(&d);
                }
            } else {
                if matches!(ext_hdr[156], b'L' | b'K') {
                    zero_header_mtime(&mut ext_hdr);
                }
                out.extend_from_slice(&ext_hdr);
                out.extend_from_slice(&ext_data);
            }
        }
        Ok(())
    }

    pub(super) fn emit_passthrough(self, out: &mut Vec<u8>) {
        for (ext_hdr, ext_data) in self.raw {
            out.extend_from_slice(&ext_hdr);
            out.extend_from_slice(&ext_data);
        }
    }
}

impl PaxScan {
    fn count_chunk(&mut self, len: u64) -> io::Result<()> {
        if len > 0 {
            if self.sparse_chunk_count >= SPARSE_CHUNKS_MAX {
                return Err(invalid(
                    "PAX sparse chunk count exceeds entry size or limit",
                ));
            }
            self.sparse_chunk_count += 1;
        }
        Ok(())
    }

    /// Scans one local `x` PAX header body in Stage 1: parses `path`/`linkpath`,
    /// validates sparse syntax/numbers/pairing, counts non-zero descriptors
    /// against `SPARSE_CHUNKS_MAX`, and records `size` without propagating plain
    /// entry `size` parse failures.
    fn scan_header(&mut self, body: &[u8]) -> io::Result<()> {
        let mut pending_offset: Option<u64> = None;
        let complete = walk_pax_records(body, |_, key, val| {
            let val_str = String::from_utf8_lossy(val);
            let parse_u64 = |label: &str| {
                val_str
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| invalid(format!("invalid pax {label}")))
            };
            match key {
                b"path" => self.path = Some(val_str.into_owned()),
                b"linkpath" => self.linkpath = Some(val_str.into_owned()),
                b"size" => match val_str.trim().parse::<u64>() {
                    Ok(sz) => {
                        self.pax_size = Some(sz);
                        self.pax_size_bad = false;
                    }
                    Err(_) => {
                        self.pax_size = None;
                        self.pax_size_bad = true;
                    }
                },
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
                    if pending_offset.is_some() {
                        return Err(invalid("GNU.sparse.offset without numbytes"));
                    }
                    pending_offset = Some(parse_u64("GNU.sparse.offset")?);
                }
                b"GNU.sparse.numbytes" => {
                    self.has_sparse = true;
                    let _off = pending_offset
                        .take()
                        .ok_or_else(|| invalid("GNU.sparse.numbytes without offset"))?;
                    let len = parse_u64("GNU.sparse.numbytes")?;
                    self.count_chunk(len)?;
                }
                b"GNU.sparse.map" => {
                    self.has_sparse = true;
                    let mut parts = val_str.split(',').filter(|s| !s.is_empty());
                    while let Some(o_str) = parts.next() {
                        let l_str = parts
                            .next()
                            .ok_or_else(|| invalid("odd item count in GNU.sparse.map"))?;
                        let _off: u64 = o_str
                            .trim()
                            .parse()
                            .map_err(|_| invalid("invalid sparse map offset"))?;
                        let len: u64 = l_str
                            .trim()
                            .parse()
                            .map_err(|_| invalid("invalid sparse map len"))?;
                        self.count_chunk(len)?;
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
        if !complete {
            self.saw_malformed = true;
        }
        if self.has_sparse && self.saw_malformed {
            return Err(invalid("malformed PAX record after GNU.sparse.* key"));
        }
        if pending_offset.is_some() {
            return Err(invalid("GNU.sparse.offset without numbytes"));
        }
        if self.has_sparse && self.pax_size_bad {
            return Err(invalid("invalid pax size"));
        }
        if self.has_sparse {
            if let Some(sz) = self.pax_size {
                padded_size(sz)?;
            }
        }
        Ok(())
    }
}

fn push_chunk(chunks: &mut Vec<(u64, u64)>, off: u64, len: u64, limit: u64) -> io::Result<()> {
    if len > 0 {
        if (chunks.len() as u64) >= limit {
            return Err(invalid(
                "PAX sparse chunk count exceeds entry size or limit",
            ));
        }
        chunks.push((off, len));
    }
    Ok(())
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
