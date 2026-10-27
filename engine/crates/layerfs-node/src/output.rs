//! A foreground exec's stdout and stderr, complete and in bounded memory.
//!
//! Each stream is kept in memory up to [`INLINE`] bytes; a result whose
//! streams both fit carries them inline as it always did. A stream that grows
//! past that spills, kept bytes first, to an unlinked file in the host temp
//! directory (or every non-empty stream does, when the caller asks to spool).
//! The result then names the spooled output, which `execOutput` reads back
//! into the caller's page of at most [`PAGE`] bytes until
//! `workspace.exec_release` closes it. The Sandbox backend reads the command's stream only as fast as
//! this sink takes it (an 8 KiB window), so a slow disk holds the command
//! back. No total is capped: a spooled stream is bounded by free space.
use crate::job::{failed, refused, Outcome};
use crate::registry::{lock, text};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use napi::bindgen_prelude::Buffer;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::sync::{Arc, Mutex, OnceLock};

/// Bytes per stream a result carries inline.
pub(crate) const INLINE: usize = 1 << 20;
/// Largest page `execOutput` fills.
const PAGE: usize = 1 << 20;
/// Write buffer of a spooled stream.
const BUFFER: usize = 64 * 1024;

/// One stream: kept in memory up to its limit, then spooled to a file.
pub(crate) struct Spool {
    kept: Vec<u8>,
    file: Option<BufWriter<File>>,
    total: u64,
    limit: usize,
}

impl Spool {
    /// Kept inline up to [`INLINE`] bytes, or spooled from the first byte.
    pub(crate) fn new(spool: bool) -> Self {
        Self {
            kept: Vec::new(),
            file: None,
            total: 0,
            limit: if spool { 0 } else { INLINE },
        }
    }
    /// The kept bytes; empty once the stream spilled to its file.
    pub(crate) fn kept(&self) -> &[u8] {
        &self.kept
    }
    fn spill(&mut self) -> io::Result<()> {
        let name = std::env::temp_dir().join(format!(
            "layerfs-exec-{}",
            layerfs_bridge::sdk_values::hex(
                &crate::registry::random().map_err(|refusal| io::Error::other(refusal.message))?
                    [..12]
            )
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&name)?;
        // Unlinked at once: the open file is the only name, and closing it
        // (or the process ending) frees its space.
        std::fs::remove_file(&name)?;
        let mut file = BufWriter::with_capacity(BUFFER, file);
        file.write_all(&self.kept)?;
        self.kept = Vec::new();
        self.file = Some(file);
        Ok(())
    }
    fn finish(self) -> io::Result<Stream> {
        match self.file {
            None => Ok(Stream::Inline(self.kept)),
            Some(file) => Ok(Stream::Spooled(Arc::new(
                file.into_inner().map_err(|error| error.into_error())?,
            ))),
        }
    }
}

impl Write for Spool {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.file.is_none() && self.kept.len() + data.len() > self.limit {
            self.spill()?;
        }
        match &mut self.file {
            Some(file) => file.write_all(data)?,
            None => self.kept.extend_from_slice(data),
        }
        self.total += data.len() as u64;
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum Stream {
    Inline(Vec<u8>),
    Spooled(Arc<File>),
}

/// The spooled streams of one result, by stream name, with their sizes.
#[derive(Default)]
struct Spooled {
    streams: Vec<(&'static str, Arc<File>, u64)>,
}

fn outputs() -> &'static Mutex<HashMap<String, Spooled>> {
    static OUTPUTS: OnceLock<Mutex<HashMap<String, Spooled>>> = OnceLock::new();
    OUTPUTS.get_or_init(Default::default)
}

/// A finished command's result fields: each stream inline as text, or null
/// when spooled; with any spooled, the output's id and both sizes follow.
pub(crate) fn fields(
    stdout: Spool,
    stderr: Spool,
) -> Result<Vec<(&'static str, SdkValue)>, SdkRefusal> {
    let sizes = [stdout.total, stderr.total];
    let mut spooled = Spooled::default();
    let mut shown = Vec::new();
    for ((name, stream), size) in [("stdout", stdout), ("stderr", stderr)]
        .into_iter()
        .zip(sizes)
    {
        match stream.finish().map_err(failed)? {
            Stream::Inline(bytes) => shown.push((
                name,
                SdkValue::Text(String::from_utf8_lossy(&bytes).into_owned()),
            )),
            Stream::Spooled(file) => {
                spooled.streams.push((name, file, size));
                shown.push((name, SdkValue::Null));
            }
        }
    }
    if spooled.streams.is_empty() {
        return Ok(shown);
    }
    let id = format!(
        "out_{}",
        layerfs_bridge::sdk_values::hex(&crate::registry::random()?[..12])
    );
    lock(outputs())?.insert(id.clone(), spooled);
    let size = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    shown.push(("output", SdkValue::Text(id)));
    shown.push(("stdoutBytes", size(sizes[0])));
    shown.push(("stderrBytes", size(sizes[1])));
    Ok(shown)
}

/// `execOutput({ output, stream, offset }, page)`: fills `page` (1..=1 MiB,
/// the caller's reused window) with the spooled stream's bytes from `offset`
/// and answers how many it wrote and whether they end the stream. Reading
/// into the caller's buffer leaves no per-page allocation behind on either
/// side, so a stream of any size passes through one page of memory.
pub(crate) fn page(args: &SdkValue, mut page: Buffer) -> Outcome {
    let id = text(args, "output")?;
    let stream = text(args, "stream")?;
    let offset = args
        .get_int("offset")
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| refused("InvalidInput", "offset must be a nonnegative integer"))?;
    if !(1..=PAGE).contains(&page.len()) {
        return Err(refused(
            "InvalidInput",
            format!("a page is 1..={PAGE} bytes"),
        ));
    }
    // The file is shared out of the table, so a page read holds no lock.
    let (file, size) = lock(outputs())?
        .get(id)
        .and_then(|output| output.streams.iter().find(|(name, ..)| *name == stream))
        .map(|(_, file, size)| (Arc::clone(file), *size))
        .ok_or_else(|| refused("InvalidInput", format!("no spooled {stream} in {id}")))?;
    if offset > size {
        return Err(refused("InvalidInput", format!("offset past {size} bytes")));
    }
    let length = page
        .len()
        .min(usize::try_from(size - offset).unwrap_or(usize::MAX));
    file.read_exact_at(&mut page[..length], offset)
        .map_err(failed)?;
    let end = offset + length as u64 == size;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("length", SdkValue::Int(length as i64)),
        ("end", SdkValue::Bool(end)),
    ]))
}

/// `workspace.exec_release({ output })`: closes a spooled output's files.
pub(crate) fn release(args: &SdkValue) -> Outcome {
    let id = text(args, "output")?;
    let released = lock(outputs())?.remove(id).is_some();
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("released", SdkValue::Bool(released)),
    ]))
}
