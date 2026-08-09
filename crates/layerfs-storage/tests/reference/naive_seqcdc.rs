//! Direct scalar transliteration of the pinned UWASL increasing-mode loop.
//!
//! Source authority: `dedup-bench` commit
//! `8e2697cbf6332ac5da6dc615bfab82a720e820e4`, Apache-2.0.

pub const MINIMUM: usize = 8_192;
pub const MAXIMUM: usize = 32_768;
pub const SEQUENCE_THRESHOLD: u16 = 5;
pub const JUMP_TRIGGER: u16 = 50;
pub const JUMP_SIZE: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OracleResult {
    pub cut: usize,
    pub comparisons: u64,
    pub equal_absorptions: u64,
    pub opposing_slopes: u64,
    pub jumps: u64,
    pub jump_bytes: u64,
}

pub fn cut(source: &[u8]) -> OracleResult {
    if source.len() < MINIMUM {
        return OracleResult {
            cut: source.len(),
            ..OracleResult::default()
        };
    }

    let size = source.len().min(MAXIMUM);
    let mut current_position = MINIMUM;
    let mut opposing_slope_count = 0_u16;
    let mut current_sequence_length = 0_u16;
    let mut result = OracleResult::default();

    while current_position < size {
        let current = source[current_position];
        let previous = source[current_position - 1];
        current_position += 1;
        result.comparisons += 1;

        if current == previous {
            result.equal_absorptions += 1;
            continue;
        }

        let opposing = current < previous;
        if opposing {
            opposing_slope_count += 1;
            current_sequence_length = 0;
            result.opposing_slopes += 1;
        } else {
            current_sequence_length += 1;
        }

        if current_sequence_length == SEQUENCE_THRESHOLD {
            result.cut = current_position - 1;
            return result;
        }
        if opposing_slope_count == JUMP_TRIGGER {
            current_position += JUMP_SIZE;
            opposing_slope_count = 0;
            result.jumps += 1;
            result.jump_bytes += JUMP_SIZE as u64;
        }
    }

    result.cut = size;
    result
}
