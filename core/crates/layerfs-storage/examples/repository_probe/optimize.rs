//! Concrete external importer optimizations and their registered comparisons.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Instant;

use super::{metrics::Span, packs::Packer, Result};
use layerfs_content::{
    construct_bytes, construct_stream, ConstructionPolicy, ContentResult, FinalizedConsumer,
    FinalizedObject, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use rusqlite::Connection;

pub fn sized_bytes<R: Read>(source: &mut R, size: usize) -> Result<Vec<u8>> {
    if size >= 131072 {
        return Err("small input capacity".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size)?;
    bytes.resize(size, 0);
    source.read_exact(&mut bytes)?;
    let mut eof = [0_u8];
    if source.read(&mut eof)? != 0 {
        return Err("small source trailing bytes".into());
    }
    Ok(bytes)
}
struct Meter {
    file: File,
    reads: Span,
}
impl Read for Meter {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let point = Instant::now();
        let result = self.file.read(out);
        self.reads
            .add(point, result.as_ref().copied().unwrap_or(0) as u64);
        result
    }
}
#[derive(Default)]
struct Discard {
    calls: u64,
    bytes: u64,
    ns: u128,
}
impl FinalizedConsumer for Discard {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let point = Instant::now();
        self.calls += 1;
        self.bytes += object.canonical_len() as u64;
        drop(object);
        self.ns += point.elapsed().as_nanos();
        Ok(())
    }
}

pub fn small_sources(reference: &Path, fixture: &Path, output: &Path, sized: bool) -> Result<()> {
    let started = Instant::now();
    fs::create_dir(output)?;
    let db = Connection::open(reference)?;
    db.execute_batch("PRAGMA query_only=ON;PRAGMA cache_size=-512;PRAGMA mmap_size=0;")?;
    let engine: String = db.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let mut query=db.prepare("SELECT path,size,content_root FROM entries WHERE kind='file' AND size>0 AND size<131072 ORDER BY path")?;
    let mut rows = query.query([])?;
    let policy = ConstructionPolicy::default();
    let capacities = policy.capacities();
    let (mut opens, mut reads, mut closes, mut construction, mut enumeration) = (
        Span::default(),
        Span::default(),
        Span::default(),
        Span::default(),
        Span::default(),
    );
    let mut consumer = Discard::default();
    let (mut files, mut logical, mut allocation) = (0_u64, 0_u64, 0_u64);
    loop {
        let point = Instant::now();
        let record = match rows.next()? {
            Some(r) => Some((
                r.get::<_, Vec<u8>>(0)?,
                usize::try_from(r.get::<_, i64>(1)?)?,
                r.get::<_, Vec<u8>>(2)?,
            )),
            None => None,
        };
        enumeration.add(point, 0);
        let Some((path, size, root)) = record else {
            break;
        };
        let point = Instant::now();
        let mut input = Meter {
            file: File::open(fixture.join(std::ffi::OsStr::from_bytes(&path)))?,
            reads: Span::default(),
        };
        opens.add(point, 0);
        let point = Instant::now();
        let built = if sized {
            let bytes = sized_bytes(&mut input, size)?;
            let (result, _) = Timing::disabled("small.sized", |scope| {
                construct_bytes(
                    policy,
                    &capacities,
                    &bytes,
                    &mut consumer,
                    scope.child("file"),
                )
            });
            result?
        } else {
            let (result, _) = Timing::disabled("small.stream", |scope| {
                construct_stream(
                    policy,
                    &capacities,
                    &mut input,
                    &mut consumer,
                    scope.child("file"),
                )
            });
            result?
        };
        construction.add(point, size as u64);
        if built.logical_len != size as u64 || built.root != ObjectId::from_bytes(&root)? {
            return Err("small canonical/size mismatch".into());
        }
        reads.ns += input.reads.ns;
        reads.calls += input.reads.calls;
        reads.bytes += input.reads.bytes;
        let point = Instant::now();
        drop(input);
        closes.add(point, 0);
        files += 1;
        logical += size as u64;
        allocation += if sized { size as u64 } else { 131072 };
    }
    drop(rows);
    drop(query);
    drop(db);
    let wall = started.elapsed().as_nanos();
    fs::write(output.join("result.json"),format!("{{\"mechanism\":\"small-input\",\"sqlite_version\":\"{engine}\",\"sized\":{sized},\"files\":{files},\"logical_bytes\":{logical},\"whole_ns\":{wall},\"open\":{},\"read\":{},\"close\":{},\"enumeration\":{},\"construction_inclusive\":{},\"consumer_calls\":{},\"consumer_bytes\":{},\"consumer_ns\":{},\"cumulative_requested_input_capacity\":{allocation},\"root_equality\":\"PASS\",\"cache_verdict\":\"INELIGIBLE\",\"performance_claim\":false}}\n",opens.json(),reads.json(),closes.json(),enumeration.json(),construction.json(),consumer.calls,consumer.bytes,consumer.ns))?;
    Ok(())
}

pub fn root_updates(master: &Path, reference: &Path, output: &Path, cached: bool) -> Result<()> {
    let started = Instant::now();
    fs::create_dir(output)?;
    fs::create_dir(output.join("packs"))?;
    fs::copy(
        master.join("manifest.sqlite"),
        output.join("catalog.sqlite"),
    )?;
    let mut packer = Packer::new(output)?;
    let db = Connection::open(reference)?;
    db.execute_batch("PRAGMA query_only=ON;PRAGMA cache_size=-512;PRAGMA mmap_size=0;")?;
    let engine: String = db.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let mut query =
        db.prepare("SELECT path,content_root FROM entries WHERE kind='file' ORDER BY path")?;
    let mut rows = query.query([])?;
    let mut count = 0_u64;
    let mut enumeration = Span::default();
    loop {
        let point = Instant::now();
        let record = match rows.next()? {
            Some(r) => Some((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?)),
            None => None,
        };
        enumeration.add(point, 0);
        let Some((path, root)) = record else {
            break;
        };
        count += 1;
        packer.update_file_mode(&path, ObjectId::from_bytes(&root)?, count, cached)?;
    }
    packer.finish()?;
    let metrics = packer.metrics.json(packer.probe_ns());
    drop(rows);
    drop(query);
    drop(db);
    drop(packer);
    let wall = started.elapsed().as_nanos();
    fs::write(output.join("result.json"),format!("{{\"mechanism\":\"root-update\",\"sqlite_version\":\"{engine}\",\"cached\":{cached},\"files\":{count},\"whole_ns\":{wall},\"enumeration\":{},\"metrics\":{metrics},\"cache_verdict\":\"INELIGIBLE\",\"performance_claim\":false}}\n",enumeration.json()))?;
    Ok(())
}

pub fn selfcheck() -> Result<()> {
    for size in [0, 1, 8192, 131071] {
        let data = vec![73; size];
        let output = sized_bytes(&mut std::io::Cursor::new(&data), size)?;
        if output != data {
            return Err("sized bytes equality".into());
        }
    }
    if sized_bytes(&mut std::io::Cursor::new(b"short"), 6).is_ok() {
        return Err("short input accepted".into());
    }
    if sized_bytes(&mut std::io::Cursor::new(b"trailing"), 7).is_ok() {
        return Err("trailing input accepted".into());
    }
    if sized_bytes(&mut std::io::Cursor::new(Vec::<u8>::new()), 131072).is_ok() {
        return Err("capacity accepted".into());
    }
    std::io::stdout().write_all(b"sized input boundaries/short/trailing/capacity PASS\n")?;
    Ok(())
}
