//! FastCDC's pre-shifted view of the frozen profile GEAR table.

use crate::profile::GEAR;

pub(super) const GEAR_LS: [u64; 256] = shifted_gear();

const fn shifted_gear() -> [u64; 256] {
    let mut shifted = [0_u64; 256];
    let mut index = 0;
    while index < GEAR.len() {
        shifted[index] = GEAR[index].wrapping_shl(1);
        index += 1;
    }
    shifted
}
