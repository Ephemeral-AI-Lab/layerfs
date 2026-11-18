//! A set of numbers one mark keeps: the packs it retains, the inode value
//! ordinals it read.
//!
//! Both are drawn from one counter of the Store, so the set is a bitmap. It
//! is kept in pages of 65,536 numbers, allocated only where a number was
//! added, so its size follows what the mark kept (one bit each where they
//! lie together), not the highest number the Store ever reserved.
//!
//! A reached inode leaf stores ordinals, and its values live in
//! `inode_value_block` under them. Reading a leaf asks for the ordinals of
//! every body of its delta chain, and every member of that chain is itself
//! reached, so the ordinals the walk asked its source for are exactly the
//! ones any later read of a kept leaf can ask for. A rebuild keeps the blocks
//! that hold one of them and leaves the rest behind.
use std::collections::BTreeMap;

/// Numbers per page, and 64-bit words per page.
const PAGE_BITS: u32 = 16;
const PAGE_WORDS: usize = 1 << (PAGE_BITS - 6);

/// A set of non-negative numbers, held as one bit each in the pages that
/// have a member.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IdSet {
    pages: BTreeMap<u64, Box<[u64; PAGE_WORDS]>>,
    count: u64,
}

impl IdSet {
    /// Adds `id` and says whether it was absent.
    pub fn insert(&mut self, id: u64) -> bool {
        let page = self
            .pages
            .entry(id >> PAGE_BITS)
            .or_insert_with(|| Box::new([0; PAGE_WORDS]));
        let bit = id & ((1 << PAGE_BITS) - 1);
        let word = &mut page[(bit >> 6) as usize];
        let absent = *word & (1 << (bit & 63)) == 0;
        *word |= 1 << (bit & 63);
        self.count += u64::from(absent);
        absent
    }

    /// Whether `id` is a member.
    pub fn contains(&self, id: u64) -> bool {
        self.pages.get(&(id >> PAGE_BITS)).is_some_and(|page| {
            let bit = id & ((1 << PAGE_BITS) - 1);
            page[(bit >> 6) as usize] & (1 << (bit & 63)) != 0
        })
    }

    /// How many members the set has.
    pub fn len(&self) -> u64 {
        self.count
    }

    /// Whether the set has no member.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The members as inclusive runs, ascending and never adjacent. Nothing
    /// is collected: a run is found as it is asked for.
    pub fn runs(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        let mut words = self.pages.iter().flat_map(|(page, words)| {
            words
                .iter()
                .enumerate()
                .filter(|(_, word)| **word != 0)
                .map(move |(index, word)| ((page << PAGE_BITS) | ((index as u64) << 6), *word))
        });
        // The run being grown, and the rest of the word it ends in.
        let mut open: Option<(u64, u64)> = None;
        let mut rest: (u64, u64) = (0, 0);
        std::iter::from_fn(move || loop {
            if rest.1 == 0 {
                match words.next() {
                    Some(next) => rest = next,
                    None => return open.take(),
                }
            }
            let low = rest.1.trailing_zeros();
            // The set bits from `low` up to the next clear one.
            let length = (rest.1 >> low).trailing_ones();
            let first = rest.0 | u64::from(low);
            let last = first + u64::from(length - 1);
            rest.1 = if low + length >= 64 {
                0
            } else {
                rest.1 & !(((1u64 << length) - 1) << low)
            };
            match open {
                Some((from, to)) if to + 1 == first => open = Some((from, last)),
                Some(done) => {
                    open = Some((first, last));
                    return Some(done);
                }
                None => open = Some((first, last)),
            }
        })
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.runs().flat_map(|(first, last)| first..=last)
    }
}
