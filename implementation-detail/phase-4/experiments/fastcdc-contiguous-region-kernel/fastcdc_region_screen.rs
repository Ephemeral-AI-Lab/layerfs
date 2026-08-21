use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Cursor, Write};
use std::path::Path;
use std::time::Instant;

use layerfs_core::cdc::{CdcCounters, FastCdc, MAXIMUM_CHUNK_BYTES};
use layerfs_core::{CoreError, CoreResult};

const EXPECTED_BYTES: usize = 104_857_600;
const EXPECTED_CHUNKS: usize = 5_284;

#[inline(never)]
fn timed_scan(source: &[u8], lengths: &mut Vec<u32>) -> CoreResult<(CdcCounters, u128)> {
    let started = Instant::now();
    let counters = FastCdc::new().scan(Cursor::new(source), |chunk| {
        if chunk.is_empty() || lengths.len() == EXPECTED_CHUNKS {
            return Err(CoreError::ObjectLimitExceeded);
        }
        lengths.push(u32::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?);
        Ok(())
    })?;
    Ok((counters, started.elapsed().as_nanos()))
}

fn write_boundaries(path: &Path, lengths: &[u32]) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = BufWriter::new(File::create(path)?);
    writeln!(output, "ordinal\tstart\tend\tlength")?;
    let mut start = 0_u64;
    for (ordinal, &length) in lengths.iter().enumerate() {
        let end = start + u64::from(length);
        writeln!(output, "{ordinal}\t{start}\t{end}\t{length}")?;
        start = end;
    }
    output.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let fixture = args.next().ok_or("missing fixture")?;
    let authority = args.next().ok_or("missing boundary authority or '-'")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }

    let source = fs::read(fixture)?;
    if source.len() != EXPECTED_BYTES {
        return Err("fixture length differs from frozen contract".into());
    }
    let mut lengths = Vec::with_capacity(EXPECTED_CHUNKS);
    let (counters, boundary_wall_ns) = timed_scan(&source, &mut lengths)?;

    let mut transcript = blake3::Hasher::new();
    let mut reconstruction = blake3::Hasher::new();
    let mut start = 0_u64;
    let mut minimum = usize::MAX;
    let mut maximum = 0_usize;
    for &length in &lengths {
        let length = u64::from(length);
        let end = start
            .checked_add(length)
            .ok_or("boundary length overflow")?;
        transcript.update(&start.to_be_bytes());
        transcript.update(&end.to_be_bytes());
        transcript.update(&length.to_be_bytes());
        reconstruction.update(&source[usize::try_from(start)?..usize::try_from(end)?]);
        minimum = minimum.min(usize::try_from(length)?);
        maximum = maximum.max(usize::try_from(length)?);
        start = end;
    }

    if counters.bytes_scanned != EXPECTED_BYTES as u64
        || counters.chunks_emitted != EXPECTED_CHUNKS as u64
        || lengths.len() != EXPECTED_CHUNKS
        || start != EXPECTED_BYTES as u64
        || lengths.capacity() != EXPECTED_CHUNKS
        || maximum > MAXIMUM_CHUNK_BYTES
    {
        return Err("screen counters differ from frozen contract".into());
    }
    if authority != "-" {
        write_boundaries(Path::new(&authority), &lengths)?;
    }

    println!(
        "{{\"status\":\"PASS\",\"input_bytes_consumed\":{},\"bytes_scanned\":{},\"output_occurrences\":{},\"callback_count\":{},\"sum_occurrence_lengths\":{},\"minimum_occurrence_length\":{},\"maximum_occurrence_length\":{},\"ordered_boundary_transcript_blake3\":\"{}\",\"reconstructed_source_blake3\":\"{}\",\"boundary_wall_ns\":{},\"scanner_chunk_buffer_capacity\":{},\"boundary_record_capacity\":{},\"terminal_end\":{}}}",
        source.len(),
        counters.bytes_scanned,
        counters.chunks_emitted,
        lengths.len(),
        start,
        minimum,
        maximum,
        transcript.finalize().to_hex(),
        reconstruction.finalize().to_hex(),
        boundary_wall_ns,
        MAXIMUM_CHUNK_BYTES,
        lengths.capacity(),
        start,
    );
    Ok(())
}
