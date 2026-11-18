//! The inode value ordinals one mark read.
//!
//! A reached inode leaf stores ordinals, and its values live in
//! `inode_value_block` under them. Reading a leaf asks for the ordinals of
//! every body of its delta chain, and every member of that chain is itself
//! reached, so the ordinals the walk asked its source for are exactly the
//! ones any later read of a kept leaf can ask for. A rebuild keeps the blocks
//! that hold one of them and leaves the rest behind.
//!
//! The set is a bitmap in pages of 65,536 ordinals, allocated only where an
//! ordinal was read, so its size follows the values a Store keeps, not the
//! highest ordinal it ever reserved.
use std::collections::BTreeMap;

/// Ordinals per page, and 64-bit words per page.
const PAGE_BITS: u32 = 16;
const PAGE_WORDS: usize = 1 << (PAGE_BITS - 6);

/// The ordinals a walk asked for.
#[derive(Default)]
pub(super) struct Ordinals {
    pages: BTreeMap<u32, Box<[u64; PAGE_WORDS]>>,
}

impl Ordinals {
    /// Notes every ordinal of one read.
    pub(super) fn extend(&mut self, ordinals: &[u32]) {
        for ordinal in ordinals {
            let page = self
                .pages
                .entry(ordinal >> PAGE_BITS)
                .or_insert_with(|| Box::new([0; PAGE_WORDS]));
            let bit = ordinal & ((1 << PAGE_BITS) - 1);
            page[(bit >> 6) as usize] |= 1 << (bit & 63);
        }
    }

    /// The noted ordinals as inclusive runs, ascending and never adjacent.
    pub(super) fn runs(&self) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for (page, words) in &self.pages {
            for (index, word) in words.iter().enumerate() {
                let mut rest = *word;
                while rest != 0 {
                    let low = rest.trailing_zeros();
                    // The set bits from `low` up to the next clear one.
                    let length = (rest >> low).trailing_ones();
                    let first = (page << PAGE_BITS) | ((index as u32) << 6) | low;
                    let last = first + (length - 1);
                    match runs.last_mut() {
                        Some(open) if open.1.checked_add(1) == Some(first) => open.1 = last,
                        _ => runs.push((first, last)),
                    }
                    rest = if low + length >= 64 {
                        0
                    } else {
                        rest & !(((1u64 << length) - 1) << low)
                    };
                }
            }
        }
        runs
    }
}
