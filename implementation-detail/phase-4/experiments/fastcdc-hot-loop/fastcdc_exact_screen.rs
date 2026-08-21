use std::env;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::time::Instant;

use layerfs_core::cdc::{FastCdc, MAXIMUM_CHUNK_BYTES};
use layerfs_core::{CoreError, CoreResult};

const EXPECTED_BYTES: u64 = 104_857_600;
const EXPECTED_CHUNKS: usize = 5_284;

struct CountedReader {
    file: File,
    calls: u64,
    nonempty_calls: u64,
    bytes: u64,
}

impl Read for CountedReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.calls += 1;
        let read = self.file.read(output)?;
        if read != 0 {
            self.nonempty_calls += 1;
            self.bytes += read as u64;
        }
        Ok(read)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let fixture = args.next().ok_or("missing fixture")?;
    let boundaries_path = args.next().ok_or("missing boundary output")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }

    let mut reader = CountedReader {
        file: File::open(fixture)?,
        calls: 0,
        nonempty_calls: 0,
        bytes: 0,
    };
    let mut boundaries = Vec::with_capacity(EXPECTED_CHUNKS);
    let mut transcript = blake3::Hasher::new();
    let mut reconstruction = blake3::Hasher::new();
    let mut start = 0_u64;
    let mut minimum = usize::MAX;
    let mut maximum = 0_usize;

    let started = Instant::now();
    let counters = FastCdc::new().scan(&mut reader, |chunk| -> CoreResult<()> {
        if chunk.is_empty() || boundaries.len() == EXPECTED_CHUNKS {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let length = u64::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?;
        let end = start.checked_add(length).ok_or(CoreError::LengthOverflow)?;
        transcript.update(&start.to_be_bytes());
        transcript.update(&end.to_be_bytes());
        transcript.update(&length.to_be_bytes());
        reconstruction.update(chunk);
        boundaries.push((start, end, length));
        minimum = minimum.min(chunk.len());
        maximum = maximum.max(chunk.len());
        start = end;
        Ok(())
    })?;
    let scan_wall_ns = started.elapsed().as_nanos();

    if reader.bytes != EXPECTED_BYTES
        || counters.bytes_scanned != EXPECTED_BYTES
        || counters.chunks_emitted != EXPECTED_CHUNKS as u64
        || boundaries.len() != EXPECTED_CHUNKS
        || start != EXPECTED_BYTES
    {
        return Err("fixture counters differ from the frozen contract".into());
    }

    let mut output = BufWriter::new(File::create(boundaries_path)?);
    writeln!(output, "ordinal\tstart\tend\tlength")?;
    for (ordinal, (start, end, length)) in boundaries.iter().enumerate() {
        writeln!(output, "{ordinal}\t{start}\t{end}\t{length}")?;
    }
    output.flush()?;

    println!(
        "{{\"status\":\"PASS\",\"input_bytes_consumed\":{},\"bytes_scanned\":{},\"output_occurrences\":{},\"callback_count\":{},\"sum_occurrence_lengths\":{},\"minimum_occurrence_length\":{},\"maximum_occurrence_length\":{},\"ordered_boundary_transcript_blake3\":\"{}\",\"reconstructed_source_blake3\":\"{}\",\"source_read_calls\":{},\"source_nonempty_read_calls\":{},\"source_read_bytes\":{},\"scan_wall_ns\":{},\"scanner_chunk_buffer_capacity\":{},\"boundary_record_capacity\":{},\"terminal_occurrence_sum\":{}}}",
        reader.bytes,
        counters.bytes_scanned,
        counters.chunks_emitted,
        boundaries.len(),
        start,
        minimum,
        maximum,
        transcript.finalize().to_hex(),
        reconstruction.finalize().to_hex(),
        reader.calls,
        reader.nonempty_calls,
        reader.bytes,
        scan_wall_ns,
        MAXIMUM_CHUNK_BYTES,
        boundaries.capacity(),
        start,
    );
    Ok(())
}
