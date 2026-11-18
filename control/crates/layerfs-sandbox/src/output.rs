//! A foreground exec's stdout and stderr, complete and in bounded memory
//! (call surface 2.0, `workspace.exec`).
//!
//! Each stream is kept in memory up to [`INLINE`] bytes; a result whose
//! stream fits carries it whole. A stream that grows past that is written,
//! kept bytes first, to a named file beside the Sandbox's daemon log
//! (`<run folder>/exec/<workspace id>/<exec id>.stdout`), which
//! `sandbox files` reads and the Workspace's unmount deletes, and only its
//! last [`INLINE`] bytes stay in memory: the result carries that tail and
//! names the file. A spilled stream is also registered here so the side
//! that ran the command can stream all of it to its caller as output
//! events (`route::drain`, `--stream` and the front ends' sinks), one page
//! at a time, before the file is closed. The Sandbox backend reads the
//! command's stream only as fast as this sink takes it (an 8 KiB window),
//! so a slow disk holds the command back. No total is capped: a spilled
//! stream is bounded by free space.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::lock;
use layerfs_home::outcome::{failed, refused};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Bytes per stream a result carries: the whole stream, or its last bytes.
pub const INLINE: usize = 1 << 20;
/// Largest page a drain reads.
const PAGE: usize = 1 << 20;
/// Write buffer of a spilled stream.
const BUFFER: usize = 64 * 1024;

/// Where a command's streams spill: `<run folder>/exec/<workspace id>/`,
/// each stream at `<exec id>.<stream>`.
#[derive(Clone, Debug)]
pub struct Place {
    /// The folder of the Workspace's output files.
    pub folder: PathBuf,
    /// The command's exec id.
    pub exec: String,
}

/// The folder of Workspace `workspace`'s output files in the run folder
/// whose commands' home is `scratch` (the run folder's `tmp`).
pub fn folder(scratch: &str, workspace: &str) -> Option<PathBuf> {
    let run = Path::new(scratch).parent()?;
    Some(run.join("exec").join(workspace))
}

/// One stream: kept in memory up to [`INLINE`] bytes, then written to its
/// file with only its tail kept.
pub struct Spool {
    kept: Vec<u8>,
    file: Option<BufWriter<File>>,
    path: Option<PathBuf>,
    total: u64,
}

impl Spool {
    /// A stream that spills to `path` (or, with none, to an unlinked file
    /// in the host temp directory, which no result names).
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            kept: Vec::new(),
            file: None,
            path,
            total: 0,
        }
    }

    /// The stream named `stream` of the command `place` names.
    pub fn at(place: Option<&Place>, stream: &str) -> Self {
        Self::new(place.map(|place| place.folder.join(format!("{}.{stream}", place.exec))))
    }

    fn spill(&mut self) -> io::Result<()> {
        let file = match &self.path {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(path)?
            }
            None => {
                let name = std::env::temp_dir().join(format!(
                    "layerfs-exec-{}",
                    layerfs_bridge::sdk_values::hex(
                        &layerfs_home::helpers::random()
                            .map_err(|refusal| io::Error::other(refusal.message))?[..12]
                    )
                ));
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&name)?;
                // unlinked at once: the open file is its only name
                std::fs::remove_file(&name)?;
                file
            }
        };
        let mut file = BufWriter::with_capacity(BUFFER, file);
        file.write_all(&self.kept)?;
        self.file = Some(file);
        Ok(())
    }

    fn finish(mut self) -> io::Result<Stream> {
        let Some(file) = self.file else {
            return Ok(Stream::Inline(self.kept));
        };
        let file = file.into_inner().map_err(|error| error.into_error())?;
        let cut = self.kept.len().saturating_sub(INLINE);
        self.kept.drain(..cut);
        // a tail that starts inside a UTF-8 character starts after it
        let partial = (self.kept.iter().take(3))
            .take_while(|byte| **byte & 0xC0 == 0x80)
            .count();
        self.kept.drain(..partial);
        Ok(Stream::Spilled {
            tail: self.kept,
            file: Arc::new(file),
            path: self.path,
        })
    }
}

impl Write for Spool {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.file.is_none() && self.kept.len() + data.len() > INLINE {
            self.spill()?;
        }
        if let Some(file) = &mut self.file {
            file.write_all(data)?;
        }
        self.kept.extend_from_slice(data);
        // once spilled, only the tail stays: cut back to it now and then
        if self.file.is_some() && self.kept.len() > 2 * INLINE {
            let cut = self.kept.len() - INLINE;
            self.kept.drain(..cut);
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
    Spilled {
        tail: Vec<u8>,
        file: Arc<File>,
        path: Option<PathBuf>,
    },
}

/// Bytes as text when they are UTF-8; output that is not stays bytes,
/// never replaced.
fn shown(bytes: Vec<u8>) -> SdkValue {
    String::from_utf8(bytes)
        .map(SdkValue::Text)
        .unwrap_or_else(|error| SdkValue::Bytes(error.into_bytes()))
}

/// The spilled streams of one result, by stream name, with their sizes.
#[derive(Default)]
struct Spooled {
    streams: Vec<(&'static str, Arc<File>, u64)>,
}

fn outputs() -> &'static Mutex<HashMap<String, Spooled>> {
    static OUTPUTS: OnceLock<Mutex<HashMap<String, Spooled>>> = OnceLock::new();
    OUTPUTS.get_or_init(Default::default)
}

/// A finished command's result fields: each stream inline (`stdout`,
/// `stderr`), or, when spilled, null there with its tail
/// (`stdoutTail`) and its file (`stdoutFile`); both sizes
/// (`stdoutBytes`, `stderrBytes`); and, with any spilled, the id its
/// drain reads it by (`output`).
pub fn fields(stdout: Spool, stderr: Spool) -> Result<Vec<(&'static str, SdkValue)>, SdkRefusal> {
    let sizes = [stdout.total, stderr.total];
    let mut spooled = Spooled::default();
    let mut result = Vec::new();
    let names = [
        ("stdout", "stdoutTail", "stdoutFile"),
        ("stderr", "stderrTail", "stderrFile"),
    ];
    for ((names, stream), size) in names.into_iter().zip([stdout, stderr]).zip(sizes) {
        let (name, tail_name, file_name) = names;
        match stream.finish().map_err(failed)? {
            Stream::Inline(bytes) => result.push((name, shown(bytes))),
            Stream::Spilled { tail, file, path } => {
                spooled.streams.push((name, file, size));
                result.push((name, SdkValue::Null));
                result.push((tail_name, shown(tail)));
                let path = path.map(|path| path.display().to_string());
                result.push((file_name, SdkValue::opt_text(path)));
            }
        }
    }
    let size = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    result.push(("stdoutBytes", size(sizes[0])));
    result.push(("stderrBytes", size(sizes[1])));
    if spooled.streams.is_empty() {
        return Ok(result);
    }
    let id = format!(
        "out_{}",
        layerfs_bridge::sdk_values::hex(&layerfs_home::helpers::random()?[..12])
    );
    lock(outputs())?.insert(id.clone(), spooled);
    result.push(("output", SdkValue::Text(id)));
    Ok(result)
}

/// Fills `page` (1..=1 MiB, the caller's reused window) with spilled
/// output `id`'s `stream` bytes from `offset` and answers how many it
/// wrote and whether they end the stream. Reading into the caller's buffer
/// leaves no per-page allocation behind, so a stream of any size passes
/// through one page of memory.
pub fn page(
    id: &str,
    stream: &str,
    offset: u64,
    page: &mut [u8],
) -> Result<(usize, bool), SdkRefusal> {
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
    Ok((length, end))
}

/// Closes spilled output `id`'s open files (a named file stays until its
/// Workspace's unmount); whether it was open.
pub fn release(id: &str) -> Result<bool, SdkRefusal> {
    Ok(lock(outputs())?.remove(id).is_some())
}

/// Removes Workspace `workspace`'s output files in the run folder whose
/// commands' home is `scratch`, as its unmount does. A folder that is not
/// there is no failure.
pub fn remove(scratch: &str, workspace: &str) -> Result<(), SdkRefusal> {
    let Some(folder) = folder(scratch, workspace) else {
        return Ok(());
    };
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}
