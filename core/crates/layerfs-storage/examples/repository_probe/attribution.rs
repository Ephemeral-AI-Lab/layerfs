//! Count-driven construction diagnostic, with explicitly nested time regions.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::Instant;

use layerfs_content::{construct_stream, ConstructionPolicy};
use layerfs_telemetry::timer::Timing;
use rusqlite::Connection;

use super::{metrics::Span, packs::Packer, Result};
struct MeteredRead {
    input: File,
    reads: Span,
}
impl Read for MeteredRead {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let started = Instant::now();
        let result = self.input.read(output);
        self.reads
            .add(started, result.as_ref().copied().unwrap_or(0) as u64);
        result
    }
}
#[derive(Clone, Copy, Default)]
struct Cohort {
    files: u64,
    bytes: u64,
    read_ns: u128,
    c1_ns: u128,
    consumer_ns: u128,
}
impl Cohort {
    fn json(self, label: &str) -> String {
        format!("{{\"label\":\"{label}\",\"files\":{},\"logical_bytes\":{},\"read_ns\":{},\"c1_inclusive_ns\":{},\"consumer_inclusive_ns\":{}}}",self.files,self.bytes,self.read_ns,self.c1_ns,self.consumer_ns)
    }
}

pub fn diagnose(master: &Path, output: &Path) -> Result<()> {
    let started = Instant::now();
    fs::create_dir(output)?;
    fs::create_dir(output.join("packs"))?;
    let mut copy = Span::default();
    let point = Instant::now();
    let copied = fs::copy(
        master.join("manifest.sqlite"),
        output.join("catalog.sqlite"),
    )?;
    copy.add(point, copied);
    let point = Instant::now();
    let source = Connection::open(master.join("manifest.sqlite"))?;
    source.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0;")?;
    let mut packer = Packer::new(output)?;
    let setup_ns = point.elapsed().as_nanos();
    let policy = ConstructionPolicy::default();
    let capacities = policy.capacities();
    let point = Instant::now();
    let mut query =
        source.prepare("SELECT path,size FROM entries WHERE kind='file' ORDER BY path")?;
    let mut rows = query.query([])?;
    let mut enumeration = Span::default();
    enumeration.add(point, 0);
    let (mut files, mut logical) = (0_u64, 0_u64);
    let (mut open, mut reads, mut close, mut construct) = (
        Span::default(),
        Span::default(),
        Span::default(),
        Span::default(),
    );
    let mut file_updates = Span::default();
    let mut cohorts = [Cohort::default(); 3];
    let mut intervals = File::create(output.join("intervals.csv"))?;
    writeln!(intervals,"files,logical_bytes,elapsed_ns,source_read_ns,c1_inclusive_ns,consumer_inclusive_ns,sql_membership_ns,sql_file_root_ns")?;
    let mut previous = (0_u64, 0_u64, 0_u128, 0_u128, 0_u128, 0_u128, 0_u128, 0_u128);
    loop {
        let point = Instant::now();
        let record = match rows.next()? {
            Some(row) => Some((
                row.get::<_, Vec<u8>>(0)?,
                u64::try_from(row.get::<_, i64>(1)?)?,
            )),
            None => None,
        };
        enumeration.add(point, 0);
        let Some((path, size)) = record else {
            break;
        };
        let point = Instant::now();
        let mut input = MeteredRead {
            input: File::open(master.join("tree").join(std::ffi::OsStr::from_bytes(&path)))?,
            reads: Span::default(),
        };
        open.add(point, 0);
        let consumer_before = packer.metrics.consumer.ns;
        let point = Instant::now();
        let (result, _) = Timing::disabled("repository.construct", |scope| {
            construct_stream(
                policy,
                &capacities,
                &mut input,
                &mut packer,
                scope.child("file"),
            )
        });
        let elapsed = point.elapsed().as_nanos();
        construct.add_elapsed(elapsed, size);
        let built = result?;
        if built.logical_len != size {
            return Err("fixture length changed".into());
        }
        let read = input.reads;
        reads.ns += read.ns;
        reads.calls += read.calls;
        reads.bytes += read.bytes;
        let consumer = packer.metrics.consumer.ns - consumer_before;
        let cohort = &mut cohorts[if size == 0 {
            0
        } else if size < 131072 {
            1
        } else {
            2
        }];
        cohort.files += 1;
        cohort.bytes += size;
        cohort.read_ns += read.ns;
        cohort.c1_ns += elapsed;
        cohort.consumer_ns += consumer;
        let point = Instant::now();
        drop(input);
        close.add(point, 0);
        files += 1;
        logical += size;
        let point = Instant::now();
        packer.update_file(&path, built.root, files)?;
        file_updates.add(point, 0);
        if files % 4096 == 0 {
            let current = (
                files,
                logical,
                started.elapsed().as_nanos(),
                reads.ns,
                construct.ns,
                packer.metrics.consumer.ns,
                packer.metrics.spans[0].ns,
                packer.metrics.spans[5].ns,
            );
            writeln!(
                intervals,
                "{},{},{},{},{},{},{},{}",
                current.0 - previous.0,
                current.1 - previous.1,
                current.2 - previous.2,
                current.3 - previous.3,
                current.4 - previous.4,
                current.5 - previous.5,
                current.6 - previous.6,
                current.7 - previous.7
            )?;
            previous = current;
            eprintln!("diagnostic files={files} logical_bytes={logical}");
        }
    }
    if files != previous.0 {
        let current = (
            files,
            logical,
            started.elapsed().as_nanos(),
            reads.ns,
            construct.ns,
            packer.metrics.consumer.ns,
            packer.metrics.spans[0].ns,
            packer.metrics.spans[5].ns,
        );
        writeln!(
            intervals,
            "{},{},{},{},{},{},{},{}",
            current.0 - previous.0,
            current.1 - previous.1,
            current.2 - previous.2,
            current.3 - previous.3,
            current.4 - previous.4,
            current.5 - previous.5,
            current.6 - previous.6,
            current.7 - previous.7
        )?;
    }
    let point = Instant::now();
    packer.finish()?;
    let validation_point = Instant::now();
    let missing: i64 = packer.db.query_row(
        "SELECT count(*) FROM entries WHERE kind='file' AND content_root IS NULL",
        [],
        |r| r.get(0),
    )?;
    packer.metrics.spans[super::metrics::VALIDATION].add(validation_point, 0);
    if missing != 0 {
        return Err("unconstructed files".into());
    }
    let validation_point = Instant::now();
    let engine: String = packer
        .db
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    packer.metrics.spans[super::metrics::VALIDATION].add(validation_point, 0);
    let finish_ns = point.elapsed().as_nanos();
    let counts=format!("{{\"files\":{files},\"logical_bytes\":{logical},\"unique_objects\":{},\"duplicates\":{},\"packs\":{},\"pack_bytes\":{}}}",packer.objects,packer.duplicates,packer.packs,packer.bytes);
    let metrics = packer.metrics.json(packer.probe_ns());
    let c1_residual = construct
        .ns
        .checked_sub(reads.ns + packer.metrics.consumer.ns)
        .ok_or("C1 nested accounting underflow")?;
    let cohorts = cohorts
        .iter()
        .zip(["empty", "small", "large"])
        .map(|(c, l)| c.json(l))
        .collect::<Vec<_>>()
        .join(",");
    drop(rows);
    drop(query);
    let point = Instant::now();
    drop(source);
    drop(packer);
    drop(intervals);
    let resource_close_ns = point.elapsed().as_nanos();
    let wall_ns = started.elapsed().as_nanos();
    let named = copy.ns
        + setup_ns
        + enumeration.ns
        + open.ns
        + construct.ns
        + close.ns
        + file_updates.ns
        + finish_ns
        + resource_close_ns;
    let outer_residual = wall_ns
        .checked_sub(named)
        .ok_or("outer nested accounting underflow")?;
    let receipt=format!("{{\"schema\":1,\"diagnostic\":\"D-deepseek-construction-attribution-v1\",\"performance_claim\":false,\"cache_verdict\":\"INELIGIBLE\",\"sqlite_version\":\"{engine}\",\"wall_ns\":{wall_ns},\"counts\":{counts},\"copy\":{},\"setup_ns\":{setup_ns},\"enumeration\":{},\"source_open\":{},\"source_read\":{},\"source_close\":{},\"c1_inclusive\":{},\"c1_residual_ns\":{c1_residual},\"file_update_inclusive\":{},\"finalization_ns\":{finish_ns},\"resource_close_ns\":{resource_close_ns},\"outer_residual_ns\":{outer_residual},\"cohorts\":[{cohorts}],\"c2\":{metrics}}}\n",copy.json(),enumeration.json(),open.json(),reads.json(),close.json(),construct.json(),file_updates.json());
    fs::write(output.join("attribution.json"), receipt)?;
    eprintln!("attribution complete: {files} files, {logical} bytes");
    Ok(())
}
