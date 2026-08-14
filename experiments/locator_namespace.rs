use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const RECORD_BYTES: usize = 160;
const CATALOG_WINDOW_RECORDS: usize = 1_024;
const SEGMENT_SIZES: [usize; 4] = [1, 64, 256, 1_024];

#[derive(Clone, Copy, Default)]
struct Operations {
    vacancy_checks: u64,
    creates: u64,
    opens: u64,
    metadata: u64,
    permission_changes: u64,
    writes: u64,
    reads: u64,
    seeks: u64,
    flushes: u64,
    hard_links: u64,
    removes: u64,
}

#[derive(Clone, Copy)]
struct Measurement {
    segment_size: usize,
    segments: usize,
    publication: Duration,
    point_lookup: Duration,
    ordered_lookup: Duration,
    publication_ops: Operations,
    point_lookup_ops: Operations,
    ordered_lookup_ops: Operations,
}

#[derive(Clone, Copy)]
struct UltraMeasurement {
    publication: Duration,
    point_lookup: Duration,
    ordered_lookup: Duration,
    publication_ops: Operations,
    point_lookup_ops: Operations,
    ordered_lookup_ops: Operations,
}

struct RunDirectory(PathBuf);

impl Drop for RunDirectory {
    fn drop(&mut self) {
        let safe = self
            .0
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("layerfs-locator-experiment-"));
        if safe {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let records = parse_bounded(&args, 1, 6_400, 1, 1_000_000, "records")?;
    let runs = parse_bounded(&args, 2, 3, 1, 20, "runs")?;
    let parent = args.get(3).map(PathBuf::from).unwrap_or_else(env::temp_dir);
    fs::create_dir_all(&parent)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let root = parent.join(format!(
        "layerfs-locator-experiment-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root)?;
    let _cleanup = RunDirectory(root.clone());

    let logical_bytes = records
        .checked_mul(RECORD_BYTES)
        .ok_or_else(|| io::Error::other("logical byte count overflow"))?;
    println!(
        "records={records} record_bytes={RECORD_BYTES} logical_bytes={logical_bytes} runs={runs} root={}",
        root.display()
    );
    println!(
        "run,segment_size,segments,publish_ms,point_lookup_ms,ordered_lookup_ms,hard_links,publish_opens,publish_reads,publish_writes,point_opens,ordered_opens"
    );

    let mut measurements = Vec::with_capacity(runs * SEGMENT_SIZES.len());
    for run in 0..runs {
        for offset in 0..SEGMENT_SIZES.len() {
            let segment_size = SEGMENT_SIZES[(run + offset) % SEGMENT_SIZES.len()];
            let case_root = root.join(format!("run-{run}-segment-{segment_size}"));
            let measurement = measure_case(&case_root, records, segment_size)?;
            println!(
                "{},{},{},{:.3},{:.3},{:.3},{},{},{},{},{},{}",
                run + 1,
                measurement.segment_size,
                measurement.segments,
                milliseconds(measurement.publication),
                milliseconds(measurement.point_lookup),
                milliseconds(measurement.ordered_lookup),
                measurement.publication_ops.hard_links,
                measurement.publication_ops.opens,
                measurement.publication_ops.reads,
                measurement.publication_ops.writes,
                measurement.point_lookup_ops.opens,
                measurement.ordered_lookup_ops.opens,
            );
            measurements.push(measurement);
            fs::remove_dir_all(&case_root)?;
        }
    }

    let baseline_publish = median_for(&measurements, 1, |value| value.publication);
    println!(
        "segment_size,median_publish_ms,publish_speedup,median_point_lookup_ms,median_ordered_lookup_ms,namespace_reduction"
    );
    for segment_size in SEGMENT_SIZES {
        let publication = median_for(&measurements, segment_size, |value| value.publication);
        let point = median_for(&measurements, segment_size, |value| value.point_lookup);
        let ordered = median_for(&measurements, segment_size, |value| value.ordered_lookup);
        let segment_count = records.div_ceil(segment_size);
        println!(
            "{segment_size},{:.3},{:.2}x,{:.3},{:.3},{:.2}x",
            milliseconds(publication),
            baseline_publish.as_secs_f64() / publication.as_secs_f64(),
            milliseconds(point),
            milliseconds(ordered),
            records as f64 / segment_count as f64,
        );
    }

    println!(
        "ultra_run,publish_ms,point_lookup_ms,ordered_lookup_ms,hard_links,publish_opens,publish_reads,publish_writes,point_opens,point_seeks,ordered_opens"
    );
    let mut ultra = Vec::with_capacity(runs);
    for run in 0..runs {
        let case_root = root.join(format!("ultra-run-{run}"));
        let measurement = measure_ultra_case(&case_root, records)?;
        println!(
            "{},{:.3},{:.3},{:.3},{},{},{},{},{},{},{}",
            run + 1,
            milliseconds(measurement.publication),
            milliseconds(measurement.point_lookup),
            milliseconds(measurement.ordered_lookup),
            measurement.publication_ops.hard_links,
            measurement.publication_ops.opens,
            measurement.publication_ops.reads,
            measurement.publication_ops.writes,
            measurement.point_lookup_ops.opens,
            measurement.point_lookup_ops.seeks,
            measurement.ordered_lookup_ops.opens,
        );
        ultra.push(measurement);
        fs::remove_dir_all(&case_root)?;
    }
    let ultra_publication = median_ultra(&ultra, |value| value.publication);
    let ultra_point = median_ultra(&ultra, |value| value.point_lookup);
    let ultra_ordered = median_ultra(&ultra, |value| value.ordered_lookup);
    println!(
        "ultra_median,{:.3},{:.2}x,{:.3},{:.2}x,{:.3},{:.2}x",
        milliseconds(ultra_publication),
        baseline_publish.as_secs_f64() / ultra_publication.as_secs_f64(),
        milliseconds(ultra_point),
        median_for(&measurements, 1, |value| value.point_lookup).as_secs_f64()
            / ultra_point.as_secs_f64(),
        milliseconds(ultra_ordered),
        median_for(&measurements, 1, |value| value.ordered_lookup).as_secs_f64()
            / ultra_ordered.as_secs_f64(),
    );

    Ok(())
}

fn parse_bounded(
    args: &[String],
    index: usize,
    default: usize,
    minimum: usize,
    maximum: usize,
    name: &str,
) -> io::Result<usize> {
    let value = match args.get(index) {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| io::Error::other(format!("invalid {name}: {value}")))?,
        None => default,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(io::Error::other(format!(
            "{name} must be in {minimum}..={maximum}"
        )));
    }
    Ok(value)
}

fn measure_case(root: &Path, records: usize, segment_size: usize) -> io::Result<Measurement> {
    let private = root.join("private");
    let visible = root.join("visible");
    fs::create_dir_all(&private)?;
    fs::create_dir_all(&visible)?;
    let segments = records.div_ceil(segment_size);

    let mut publication_ops = Operations::default();
    let publication_started = Instant::now();
    for segment in 0..segments {
        let first = segment * segment_size;
        let end = records.min(first + segment_size);
        let private_path = private.join(segment_name(segment));
        let visible_path = visible.join(segment_name(segment));

        publication_ops.vacancy_checks += 1;
        match fs::symlink_metadata(&visible_path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => return Err(io::Error::other("visible segment unexpectedly exists")),
            Err(error) => return Err(error),
        }

        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&private_path)?;
        publication_ops.creates += 1;
        for ordinal in first..end {
            output.write_all(&record(ordinal))?;
            publication_ops.writes += 1;
        }
        output.flush()?;
        publication_ops.flushes += 1;
        drop(output);

        validate_segment(&private_path, first, end, &mut publication_ops)?;
        let metadata = fs::metadata(&private_path)?;
        publication_ops.metadata += 1;
        let expected_len = (end - first)
            .checked_mul(RECORD_BYTES)
            .ok_or_else(|| io::Error::other("segment length overflow"))?
            as u64;
        if metadata.len() != expected_len {
            return Err(io::Error::other("private segment length mismatch"));
        }
        let mut permissions = metadata.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&private_path, permissions)?;
        publication_ops.permission_changes += 1;

        fs::hard_link(&private_path, &visible_path)?;
        publication_ops.hard_links += 1;
        validate_segment(&visible_path, first, end, &mut publication_ops)?;
        fs::remove_file(&private_path)?;
        publication_ops.removes += 1;
    }
    let publication = publication_started.elapsed();

    let mut point_lookup_ops = Operations::default();
    let point_started = Instant::now();
    for ordinal in 0..records {
        let segment = ordinal / segment_size;
        let mut input = File::open(visible.join(segment_name(segment)))?;
        point_lookup_ops.opens += 1;
        input.seek(SeekFrom::Start(
            ((ordinal % segment_size) * RECORD_BYTES) as u64,
        ))?;
        point_lookup_ops.seeks += 1;
        validate_record(&mut input, ordinal, &mut point_lookup_ops)?;
    }
    let point_lookup = point_started.elapsed();

    let mut ordered_lookup_ops = Operations::default();
    let ordered_started = Instant::now();
    for segment in 0..segments {
        let first = segment * segment_size;
        let end = records.min(first + segment_size);
        validate_segment(
            &visible.join(segment_name(segment)),
            first,
            end,
            &mut ordered_lookup_ops,
        )?;
    }
    let ordered_lookup = ordered_started.elapsed();

    Ok(Measurement {
        segment_size,
        segments,
        publication,
        point_lookup,
        ordered_lookup,
        publication_ops,
        point_lookup_ops,
        ordered_lookup_ops,
    })
}

fn measure_ultra_case(root: &Path, records: usize) -> io::Result<UltraMeasurement> {
    let private = root.join("private");
    let visible = root.join("visible");
    fs::create_dir_all(&private)?;
    fs::create_dir_all(&visible)?;
    let private_path = private.join("catalog.bin");
    let visible_path = visible.join("catalog.bin");

    let mut publication_ops = Operations::default();
    let publication_started = Instant::now();
    publication_ops.vacancy_checks += 1;
    match fs::symlink_metadata(&visible_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => return Err(io::Error::other("visible catalog unexpectedly exists")),
        Err(error) => return Err(error),
    }

    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&private_path)?;
    publication_ops.creates += 1;
    // Canonical-order production makes sorting, duplicate probes, and a
    // second Version insertion pass unnecessary in this experimental lane.
    let mut encoded = Vec::with_capacity(CATALOG_WINDOW_RECORDS * RECORD_BYTES);
    for first in (0..records).step_by(CATALOG_WINDOW_RECORDS) {
        encoded.clear();
        for ordinal in first..records.min(first + CATALOG_WINDOW_RECORDS) {
            encoded.extend_from_slice(&record(ordinal));
        }
        output.write_all(&encoded)?;
        publication_ops.writes += 1;
    }
    output.flush()?;
    publication_ops.flushes += 1;
    drop(output);

    validate_segment_batched(&private_path, 0, records, &mut publication_ops)?;
    let metadata = fs::metadata(&private_path)?;
    publication_ops.metadata += 1;
    let expected_len = records
        .checked_mul(RECORD_BYTES)
        .ok_or_else(|| io::Error::other("catalog length overflow"))? as u64;
    if metadata.len() != expected_len {
        return Err(io::Error::other("private catalog length mismatch"));
    }
    let mut permissions = metadata.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&private_path, permissions)?;
    publication_ops.permission_changes += 1;
    let receipt = file_receipt(&private_path, &mut publication_ops)?;

    fs::hard_link(&private_path, &visible_path)?;
    publication_ops.hard_links += 1;
    if file_receipt(&visible_path, &mut publication_ops)? != receipt {
        return Err(io::Error::other("visible catalog receipt mismatch"));
    }
    fs::remove_file(&private_path)?;
    publication_ops.removes += 1;
    let publication = publication_started.elapsed();

    // One retained catalog handle plus direct fixed offsets replaces one
    // open and a binary-search/probe chain per logical locator.
    let mut point_lookup_ops = Operations::default();
    let point_started = Instant::now();
    let mut input = File::open(&visible_path)?;
    point_lookup_ops.opens += 1;
    for ordinal in (0..records).rev() {
        input.seek(SeekFrom::Start((ordinal * RECORD_BYTES) as u64))?;
        point_lookup_ops.seeks += 1;
        validate_record(&mut input, ordinal, &mut point_lookup_ops)?;
    }
    let point_lookup = point_started.elapsed();

    let mut ordered_lookup_ops = Operations::default();
    let ordered_started = Instant::now();
    validate_segment_batched(&visible_path, 0, records, &mut ordered_lookup_ops)?;
    let ordered_lookup = ordered_started.elapsed();

    Ok(UltraMeasurement {
        publication,
        point_lookup,
        ordered_lookup,
        publication_ops,
        point_lookup_ops,
        ordered_lookup_ops,
    })
}

#[cfg(unix)]
fn file_receipt(path: &Path, operations: &mut Operations) -> io::Result<(u64, u64, u64)> {
    let metadata = fs::metadata(path)?;
    operations.metadata += 1;
    Ok((metadata.dev(), metadata.ino(), metadata.len()))
}

#[cfg(not(unix))]
fn file_receipt(_path: &Path, _operations: &mut Operations) -> io::Result<(u64, u64, u64)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "ultra receipt experiment requires stable Unix file identity",
    ))
}

fn validate_segment(
    path: &Path,
    first: usize,
    end: usize,
    operations: &mut Operations,
) -> io::Result<()> {
    let mut input = File::open(path)?;
    operations.opens += 1;
    for ordinal in first..end {
        validate_record(&mut input, ordinal, operations)?;
    }
    Ok(())
}

fn validate_segment_batched(
    path: &Path,
    first: usize,
    end: usize,
    operations: &mut Operations,
) -> io::Result<()> {
    let mut input = File::open(path)?;
    operations.opens += 1;
    let mut bytes = vec![0_u8; CATALOG_WINDOW_RECORDS * RECORD_BYTES];
    for window_first in (first..end).step_by(CATALOG_WINDOW_RECORDS) {
        let window_end = end.min(window_first + CATALOG_WINDOW_RECORDS);
        let used = (window_end - window_first) * RECORD_BYTES;
        input.read_exact(&mut bytes[..used])?;
        operations.reads += 1;
        for (offset, actual) in bytes[..used].chunks_exact(RECORD_BYTES).enumerate() {
            let ordinal = window_first + offset;
            if actual != record(ordinal) {
                return Err(io::Error::other(format!(
                    "record {ordinal} failed batched validation"
                )));
            }
        }
    }
    Ok(())
}

fn validate_record(
    input: &mut File,
    ordinal: usize,
    operations: &mut Operations,
) -> io::Result<()> {
    let mut actual = [0_u8; RECORD_BYTES];
    input.read_exact(&mut actual)?;
    operations.reads += 1;
    if actual != record(ordinal) {
        return Err(io::Error::other(format!(
            "record {ordinal} failed validation"
        )));
    }
    Ok(())
}

fn record(ordinal: usize) -> [u8; RECORD_BYTES] {
    let mut output = [0_u8; RECORD_BYTES];
    let mut state = ordinal as u64 ^ 0x9e37_79b9_7f4a_7c15;
    for chunk in output.chunks_exact_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_le_bytes());
    }
    output[..8].copy_from_slice(&(ordinal as u64).to_le_bytes());
    output
}

fn segment_name(segment: usize) -> String {
    format!("segment-{segment:08}.bin")
}

fn median_for(
    measurements: &[Measurement],
    segment_size: usize,
    select: impl Fn(&Measurement) -> Duration,
) -> Duration {
    let mut values: Vec<Duration> = measurements
        .iter()
        .filter(|value| value.segment_size == segment_size)
        .map(select)
        .collect();
    values.sort_unstable();
    values[values.len() / 2]
}

fn median_ultra(
    measurements: &[UltraMeasurement],
    select: impl Fn(&UltraMeasurement) -> Duration,
) -> Duration {
    let mut values: Vec<Duration> = measurements.iter().map(select).collect();
    values.sort_unstable();
    values[values.len() / 2]
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
