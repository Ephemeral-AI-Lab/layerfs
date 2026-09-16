//! Pooled COPY/INSERT delta: the metadata instruction grammar and its producer.
//!
//! This is the existing metadata delta algorithm, not payload prefix compression:
//! a program of COPY and INSERT instructions rebuilds one physical body from a
//! direct base. The producer uses a fixed 4096-bucket seed table over sixteen-byte
//! seeds and a bounded comparison budget; a budget-exhausted trial returns no
//! candidate and never a partial program.

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};
use crate::policy::{GROUP_LIMIT, RECORD_COUNT_LIMIT};

/// Bytes charged to the match budget of one trial.
pub const MATCH_BUDGET_BYTES: usize = 128 * 1024;
const TABLE_BUCKETS: usize = 4_096;
const TABLE_SLOTS: usize = 4;
const SEED_BYTES: usize = 16;
/// Largest instruction program accepted.
pub const PROGRAM_LIMIT: usize = 64 * 1024;

/// True when `instructions` is a well-formed program producing `output_length`.
pub fn validate(instructions: &[u8], count: usize, output_length: usize) -> StorageResult<()> {
    if !(1..=RECORD_COUNT_LIMIT).contains(&count)
        || output_length == 0
        || output_length > GROUP_LIMIT
        || instructions.len() > PROGRAM_LIMIT
    {
        return Err(StorageError::Integrity("pooled instruction header"));
    }
    let mut cursor = 0_usize;
    let mut output = 0_usize;
    for _ in 0..count {
        let opcode = *instructions
            .get(cursor)
            .ok_or(StorageError::Integrity("pooled instruction"))?;
        cursor += 1;
        let (offset, length) = match opcode {
            0 => {
                let offset = read_u32(instructions, cursor)?;
                let length = read_u32(instructions, cursor + 4)?;
                cursor += 8;
                (Some(offset), length)
            }
            1 => {
                let length = read_u32(instructions, cursor)?;
                cursor += 4;
                (None, length)
            }
            _ => return Err(StorageError::Integrity("pooled instruction opcode")),
        };
        output = output
            .checked_add(length)
            .ok_or(StorageError::Integrity("pooled instruction length"))?;
        if length == 0 || output > output_length {
            return Err(StorageError::Integrity("pooled instruction length"));
        }
        if offset.is_none() {
            cursor = cursor
                .checked_add(length)
                .ok_or(StorageError::Integrity("pooled instruction literal"))?;
        }
        if cursor > instructions.len() {
            return Err(StorageError::Integrity("pooled instruction extent"));
        }
    }
    if cursor != instructions.len() || output != output_length {
        return Err(StorageError::Integrity("pooled instruction program"));
    }
    Ok(())
}

/// Applies one validated program to `base`, producing the stored body.
pub fn apply(
    base: &[u8],
    instructions: &[u8],
    count: usize,
    output_length: usize,
) -> StorageResult<Vec<u8>> {
    validate(instructions, count, output_length)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_length)
        .map_err(|_| StorageError::Integrity("pooled output allocation"))?;
    let mut cursor = 0_usize;
    for _ in 0..count {
        let opcode = instructions[cursor];
        cursor += 1;
        match opcode {
            0 => {
                let offset = read_u32(instructions, cursor)?;
                let length = read_u32(instructions, cursor + 4)?;
                cursor += 8;
                let end = offset
                    .checked_add(length)
                    .ok_or(StorageError::Integrity("pooled copy range"))?;
                let slice = base
                    .get(offset..end)
                    .ok_or(StorageError::Integrity("pooled copy range"))?;
                output.extend_from_slice(slice);
            }
            _ => {
                let length = read_u32(instructions, cursor)?;
                cursor += 4;
                let end = cursor
                    .checked_add(length)
                    .ok_or(StorageError::Integrity("pooled insert range"))?;
                let slice = instructions
                    .get(cursor..end)
                    .ok_or(StorageError::Integrity("pooled insert range"))?;
                output.extend_from_slice(slice);
                cursor = end;
            }
        }
    }
    if output.len() != output_length {
        return Err(StorageError::Integrity("pooled output length"));
    }
    Ok(output)
}

/// Builds one instruction program turning `base` into `target`.
///
/// `remaining` is the byte budget of this single trial. A budget-exhausted or
/// instruction-exhausted trial returns `None`; the caller keeps whatever FULL
/// alternative it already holds.
pub fn build(
    base_id: ObjectId,
    base: &[u8],
    target: &[u8],
    remaining: &mut usize,
) -> StorageResult<Option<Vec<u8>>> {
    if base.is_empty()
        || base.len() > GROUP_LIMIT
        || target.is_empty()
        || target.len() > GROUP_LIMIT
    {
        return Err(StorageError::Integrity("pooled delta inputs"));
    }
    let limit = (target.len() + 1).min(GROUP_LIMIT);
    if limit < 41 {
        return Ok(None);
    }
    let mut table = vec![[u32::MAX; TABLE_SLOTS]; TABLE_BUCKETS];
    let mut offset = 0_usize;
    while offset + SEED_BYTES <= base.len() {
        if !charge(remaining, SEED_BYTES) {
            return Ok(None);
        }
        let bucket = bucket(&base[offset..offset + SEED_BYTES]);
        if let Some(slot) = table[bucket].iter_mut().find(|slot| **slot == u32::MAX) {
            *slot = offset as u32;
        }
        offset += SEED_BYTES;
    }
    let mut program = Vec::with_capacity(limit);
    program.push(1);
    program.extend_from_slice(base_id.as_bytes());
    put_u32(&mut program, target.len())?;
    put_u32(&mut program, 0)?;
    let mut count = 0_usize;
    let mut cursor = 0_usize;
    let mut literal = 0_usize;
    while cursor + SEED_BYTES <= target.len() {
        if !charge(remaining, SEED_BYTES) {
            return Ok(None);
        }
        let mut best = (0_usize, 0_usize);
        for candidate in table[bucket(&target[cursor..cursor + SEED_BYTES])] {
            if candidate == u32::MAX {
                break;
            }
            let start = candidate as usize;
            let mut length = 0_usize;
            while start + length < base.len() && cursor + length < target.len() {
                if !charge(remaining, 1) {
                    return Ok(None);
                }
                if base[start + length] != target[cursor + length] {
                    break;
                }
                length += 1;
            }
            if length >= SEED_BYTES && (length > best.1 || (length == best.1 && start < best.0)) {
                best = (start, length);
            }
        }
        if best.1 == 0 {
            cursor += 1;
            continue;
        }
        if cursor > literal
            && !instruction(
                &mut program,
                &mut count,
                None,
                &target[literal..cursor],
                cursor - literal,
                limit,
            )?
        {
            return Ok(None);
        }
        if !instruction(&mut program, &mut count, Some(best.0), &[], best.1, limit)? {
            return Ok(None);
        }
        cursor += best.1;
        literal = cursor;
    }
    if literal < target.len()
        && !instruction(
            &mut program,
            &mut count,
            None,
            &target[literal..],
            target.len() - literal,
            limit,
        )?
    {
        return Ok(None);
    }
    if count == 0 {
        return Ok(None);
    }
    program[37..41].copy_from_slice(&(count as u32).to_le_bytes());
    Ok(Some(program))
}

fn charge(remaining: &mut usize, bytes: usize) -> bool {
    if bytes > *remaining {
        *remaining = 0;
        false
    } else {
        *remaining -= bytes;
        true
    }
}

fn bucket(seed: &[u8]) -> usize {
    let hash = blake3::hash(seed);
    usize::from(u16::from_le_bytes([hash.as_bytes()[0], hash.as_bytes()[1]])) & (TABLE_BUCKETS - 1)
}

fn instruction(
    program: &mut Vec<u8>,
    count: &mut usize,
    offset: Option<usize>,
    bytes: &[u8],
    length: usize,
    limit: usize,
) -> StorageResult<bool> {
    let size = if offset.is_some() { 9 } else { 5 + length };
    if *count == RECORD_COUNT_LIMIT || program.len() + size > limit {
        return Ok(false);
    }
    program.push(u8::from(offset.is_none()));
    if let Some(offset) = offset {
        put_u32(program, offset)?;
    }
    put_u32(program, length)?;
    if offset.is_none() {
        program.extend_from_slice(bytes);
    }
    *count += 1;
    Ok(true)
}

fn put_u32(bytes: &mut Vec<u8>, value: usize) -> StorageResult<()> {
    bytes.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| StorageError::Integrity("pooled instruction field"))?
            .to_le_bytes(),
    );
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> StorageResult<usize> {
    let end = offset
        .checked_add(4)
        .ok_or(StorageError::Integrity("pooled instruction field"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or(StorageError::Integrity("pooled instruction field"))?;
    Ok(u32::from_le_bytes(
        slice
            .try_into()
            .map_err(|_| StorageError::Integrity("pooled instruction field"))?,
    ) as usize)
}
