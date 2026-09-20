//! Bounded, valid JSON projection without a full encoded intermediate.
use super::{TimingNode, TimingReport};
use std::io::{self, Write};

struct Count {
    bytes: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| io::Error::other("encoded report bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl TimingReport {
    /// Encodes within `maximum` bytes; returns None if even the root envelope
    /// cannot fit. A clipped projection preserves root name/outcome/duration.
    /// The preflight uses constant memory and writes no partial JSON to a sink.
    pub fn encode_bounded(&self, maximum: usize) -> io::Result<Option<Vec<u8>>> {
        if maximum > 1024 * 1024 {
            return Err(io::Error::other("encoded report hard limit"));
        }
        let mut count = Count {
            bytes: 0,
            limit: maximum,
        };
        if self.write_json(&mut count).is_ok() {
            let mut bytes = Vec::with_capacity(count.bytes);
            self.write_json(&mut bytes)?;
            return Ok(Some(bytes));
        }
        let Some(root) = self.root() else {
            return Ok(None);
        };
        let projection = Self::from_root(
            TimingNode::new(root.name().to_owned(), root.elapsed())
                .with_outcome(root.outcome())
                .with_incomplete(true),
        );
        let mut count = Count {
            bytes: 0,
            limit: maximum,
        };
        if projection.write_json(&mut count).is_err() {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(count.bytes);
        projection.write_json(&mut bytes)?;
        Ok(Some(bytes))
    }
}
