//! The independent oracle: census, coverage and read-back.
//!
//! **Independence is the whole point.** This module never reads the mutated
//! artifact to decide what the answer should be. Every expectation is derived from
//! the fixture recipe and from declared constants, and every read-back is
//! authenticated by the provider before it is hashed. It shares no code with the
//! operation it checks: it calls the product's *read* path to re-derive the logical
//! bytes, which is a different operation from the one that produced them.
//!
//! Read-back is streamed through a hashing sink, so a 500 MiB file is verified
//! without the oracle itself allocating 500 MiB. An oracle that allocates the whole
//! answer changes the memory figure it is there to check.

use std::io::Write;

use layerfs_content::{
    read_all, read_range, AuthenticatedObjects, ContentResult, ObjectId,
};
use layerfs_telemetry::timer::TimingScope;

use super::digest::{hex, Sha256};

/// The expected logical content of one constructed file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Expectation {
    /// Logical byte length.
    pub logical_len: u64,
    /// SHA-256 of the logical bytes, taken from the fixture recipe.
    pub sha256: [u8; 32],
}

impl Expectation {
    /// Builds the expectation from the fixture bytes themselves.
    ///
    /// The bytes exist in the harness before the product sees them, so this is a
    /// recipe-derived expectation and not a re-reading of the product's output.
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            logical_len: bytes.len() as u64,
            sha256: super::digest::sha256(bytes),
        }
    }

    /// Expectation of `base` with `start..end` replaced by `replacement`.
    ///
    /// Streamed: the spliced bytes are never materialised, so verifying a 500 MiB
    /// edit costs the oracle nothing beyond its 64-byte hasher state.
    pub fn spliced(base: &[u8], start: u64, end: u64, replacement: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(&base[..start as usize]);
        hasher.update(replacement);
        hasher.update(&base[end as usize..]);
        Self {
            logical_len: base.len() as u64 - (end - start) + replacement.len() as u64,
            sha256: hasher.finish(),
        }
    }

    /// Hex form of the expected digest.
    pub fn digest_hex(&self) -> String {
        hex(&self.sha256)
    }
}

/// A `Write` sink that hashes and counts without retaining anything.
pub struct HashingSink {
    hasher: Sha256,
    bytes: u64,
}

impl Default for HashingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl HashingSink {
    /// Empty sink.
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
            bytes: 0,
        }
    }

    /// Bytes absorbed.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Consumes the sink and returns the digest of everything written to it.
    pub fn finish(self) -> [u8; 32] {
        self.hasher.finish()
    }
}

impl Write for HashingSink {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(buffer);
        self.bytes += buffer.len() as u64;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What one read-back actually observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadBack {
    /// Bytes read from the product's read path.
    pub bytes: u64,
    /// Bytes the recipe says there are.
    pub expected_bytes: u64,
    /// Hex digest of what was read.
    pub digest: String,
    /// Hex digest the recipe says it should be.
    pub expected_digest: String,
}

impl ReadBack {
    /// Length agreement.
    pub fn length_matches(&self) -> bool {
        self.bytes == self.expected_bytes
    }

    /// Digest agreement.
    pub fn digest_matches(&self) -> bool {
        self.digest == self.expected_digest
    }

    /// Both, which is the O2 gate.
    pub fn matches(&self) -> bool {
        self.length_matches() && self.digest_matches()
    }
}

/// Reads a file root back through the product's read path and compares it with the
/// recipe's expectation.
pub fn read_back(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    expectation: &Expectation,
    scope: TimingScope<'_>,
) -> ContentResult<ReadBack> {
    let mut sink = HashingSink::new();
    read_all(reader, root, &mut sink, scope)?;
    Ok(ReadBack {
        bytes: sink.bytes(),
        expected_bytes: expectation.logical_len,
        digest: hex(&sink.finish()),
        expected_digest: expectation.digest_hex(),
    })
}

/// Reads one logical range and returns its digest, for sampled verification.
pub fn read_range_digest(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    start: u64,
    end: u64,
    scope: TimingScope<'_>,
) -> ContentResult<(u64, [u8; 32])> {
    let mut sink = HashingSink::new();
    read_range(reader, root, start..end, &mut sink, scope)?;
    let bytes = sink.bytes();
    Ok((bytes, sink.finish()))
}

/// One entry of the fixture census.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CensusEntry {
    /// Path or name of the entry, as the recipe declares it.
    pub key: String,
    /// Logical length of the entry.
    pub len: u64,
}

/// The fixture census: what the recipe says exists, and nothing else.
///
/// It is built by the fixture generator, so it cannot be influenced by what the
/// product produced. A census read back out of the artifact would agree with the
/// artifact by construction and would prove nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Census {
    /// Entries in declaration order.
    pub entries: Vec<CensusEntry>,
}

impl Census {
    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the census is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total logical bytes of every entry.
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|entry| entry.len).sum()
    }

    /// The entry with this key, if the recipe declares one.
    pub fn entry(&self, key: &str) -> Option<&CensusEntry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

/// Bounds of the frozen `TreeSample` oracle: at most eleven files, at most eleven
/// directories, three ranges per file, 64 KiB per range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeSampleBounds {
    /// Maximum files sampled.
    pub maximum_files: usize,
    /// Maximum directories sampled.
    pub maximum_directories: usize,
    /// Ranges read per sampled file.
    pub ranges_per_file: usize,
    /// Bytes read per range.
    pub range_bytes: u64,
}

impl Default for TreeSampleBounds {
    fn default() -> Self {
        Self {
            maximum_files: 11,
            maximum_directories: 11,
            ranges_per_file: 3,
            range_bytes: 65_536,
        }
    }
}

/// Three sampled ranges of one file: head, middle and tail.
///
/// The ranges are clamped to the file's length, so a one-byte file yields one
/// one-byte range rather than a request past its end.
pub fn sample_ranges(len: u64, bounds: TreeSampleBounds) -> Vec<(u64, u64)> {
    if len == 0 {
        return Vec::new();
    }
    let window = bounds.range_bytes.min(len);
    let candidates = [
        0,
        len.saturating_sub(window) / 2,
        len.saturating_sub(window),
    ];
    let mut ranges: Vec<(u64, u64)> = candidates
        .iter()
        .take(bounds.ranges_per_file)
        .map(|start| (*start, (*start + window).min(len)))
        .filter(|(start, end)| end > start)
        .collect();
    ranges.sort_unstable();
    ranges.dedup();
    ranges
}

/// One file's sampled verification outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SampleOutcome {
    /// Key of the sampled entry.
    pub key: String,
    /// Ranges read.
    pub ranges: usize,
    /// Bytes read.
    pub bytes: u64,
    /// Whether every range's digest matched the recipe's expectation.
    pub matched: bool,
}
