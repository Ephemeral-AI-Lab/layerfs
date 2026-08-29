#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Histogram {
    buckets: [u64; 32],
}

impl Histogram {
    pub(crate) fn observe(&mut self, value: u64) {
        let bucket = if value == 0 {
            0
        } else {
            (63 - value.leading_zeros() as usize).min(self.buckets.len() - 1)
        };
        self.buckets[bucket] += 1;
    }
}
