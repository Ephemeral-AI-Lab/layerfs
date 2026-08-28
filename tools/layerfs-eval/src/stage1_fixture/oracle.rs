use super::contract::{EvalResult, BUFFER_BYTES, FILE_BYTES, LABEL, RETAINED_SEED};
use super::error::io_error;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub fn fill_retained_buffer(buffer: &mut [u8], offset: u64) {
    let salt_hash = LABEL
        .bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    let mut state = RETAINED_SEED ^ salt_hash ^ offset;
    for (index, byte) in buffer.iter_mut().enumerate() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = offset.wrapping_add(index as u64);
        *byte = if (position / 8_192) % 23 == 0 {
            (salt_hash as u8).wrapping_add((position / 8_192) as u8)
        } else {
            (state >> 24) as u8
        };
    }
}

pub(super) fn generate_input(path: &Path, xor: u8) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut written = 0_u64;
    while written < FILE_BYTES {
        fill_retained_buffer(&mut buffer, written);
        if xor != 0 {
            buffer.iter_mut().for_each(|byte| *byte ^= xor);
        }
        let take = usize::try_from((FILE_BYTES - written).min(BUFFER_BYTES as u64))
            .map_err(|_| "fixture length overflow".to_owned())?;
        file.write_all(&buffer[..take]).map_err(io_error)?;
        hasher.update(&buffer[..take]);
        written += take as u64;
    }
    file.sync_all().map_err(io_error)?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn expected_bytes(start: u64, length: usize) -> EvalResult<Vec<u8>> {
    let end = start
        .checked_add(length as u64)
        .ok_or_else(|| "oracle range overflow".to_owned())?;
    if end > FILE_BYTES || length > BUFFER_BYTES {
        return Err("oracle request exceeds the bounded S1-100 range".to_owned());
    }
    let mut output = Vec::with_capacity(length);
    let mut position = start;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        fill_retained_buffer(&mut buffer, block);
        let within = usize::try_from(position - block).map_err(|_| "range overflow".to_owned())?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(|_| "range overflow".to_owned())?;
        output.extend_from_slice(&buffer[within..within + take]);
        position += take as u64;
    }
    Ok(output)
}

pub fn stream_expected<W: Write>(start: u64, length: u64, output: &mut W) -> EvalResult<()> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| "oracle range overflow".to_owned())?;
    if end > FILE_BYTES {
        return Err("oracle stream exceeds S1-100".to_owned());
    }
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut position = start;
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        fill_retained_buffer(&mut buffer, block);
        let within = usize::try_from(position - block).map_err(|_| "range overflow".to_owned())?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(|_| "range overflow".to_owned())?;
        output
            .write_all(&buffer[within..within + take])
            .map_err(io_error)?;
        position += take as u64;
    }
    Ok(())
}

pub fn edit_bytes(tag: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| tag.wrapping_add((index as u8).wrapping_mul(31)))
        .collect()
}

pub fn hash_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::super::contract::{BUFFER_BYTES, EXPECTED_RAW_DIGEST, FILE_BYTES};
    use super::{expected_bytes, fill_retained_buffer};

    #[test]
    fn retained_generator_is_chunk_stable() {
        let mut complete = vec![0_u8; BUFFER_BYTES];
        fill_retained_buffer(&mut complete, 0);
        assert_eq!(
            expected_bytes(8_000, 20_000).unwrap(),
            complete[8_000..28_000]
        );
    }

    #[test]
    fn retained_generator_matches_frozen_digest() {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0_u8; BUFFER_BYTES];
        let mut offset = 0_u64;
        while offset < FILE_BYTES {
            fill_retained_buffer(&mut buffer, offset);
            hasher.update(&buffer);
            offset += BUFFER_BYTES as u64;
        }
        assert_eq!(hasher.finalize().to_hex().as_str(), EXPECTED_RAW_DIGEST);
    }
}
