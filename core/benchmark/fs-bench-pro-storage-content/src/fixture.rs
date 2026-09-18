//! The three generators and the expected-result model.
//!
//! Every fixture here is **deterministic from its recipe**: the same seed and the
//! same length always produce the same bytes, so a golden pin names a real fixture
//! rather than whatever a run happened to build. The expectation is returned
//! *beside* the bytes, from the recipe, never read back out of the artifact.
//!
//! Three generators, because the families need three different things:
//!
//! * [`noise`] — incompressible bytes, the honest worst case for a chunker;
//! * [`zeros`] — a long run, which is where a content-defined chunker takes its
//!   largest chunks;
//! * [`structured`] — a base with a declared zero region, which is what makes the
//!   C1-3 chunk-count directions reachable at all.

use crate::workload::oracle::{Census, CensusEntry, Expectation};

/// Deterministic splitmix64 generator.
///
/// Written out rather than taken from a crate: a new dependency would move the
/// harness's lockfile, and the lockfile is compared entry by entry with the
/// product seal.
pub struct Rng(u64);

impl Rng {
    /// Seeds the generator.
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Fills a buffer with pseudo-random bytes.
    pub fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let value = self.next_u64().to_le_bytes();
            let take = chunk.len();
            chunk.copy_from_slice(&value[..take]);
        }
    }
}

/// Incompressible bytes of `len` from `seed`.
pub fn noise(len: u64, seed: u64) -> Vec<u8> {
    let mut out = vec![0_u8; len as usize];
    let mut rng = Rng::new(seed);
    rng.fill(&mut out);
    out
}

/// A zero run of `len`.
pub fn zeros(len: u64) -> Vec<u8> {
    vec![0_u8; len as usize]
}

/// Text-like bytes: long repeated words, which is the `-text-v1` fixture shape.
pub fn text(len: u64, seed: u64) -> Vec<u8> {
    let words = [
        "layerfs ", "canonical ", "content ", "storage ", "namespace ", "delta ",
    ];
    let mut out = Vec::with_capacity(len as usize + 16);
    let mut rng = Rng::new(seed);
    while (out.len() as u64) < len {
        let word = words[(rng.next_u64() % words.len() as u64) as usize];
        out.extend_from_slice(word.as_bytes());
    }
    out.truncate(len as usize);
    out
}

/// The declared zero region of the structured base, as `[start, end)`.
///
/// The C1-3 chunk-count family edits `START = 147,456` for `LEN = 65,536`, so the
/// region is placed to contain exactly that range.
pub const STRUCTURED_ZERO_REGION: (u64, u64) = (96 * 1024, 224 * 1024);

/// A base with a declared zero region inside otherwise incompressible bytes.
///
/// `zero_region` selects whether the edited region holds zeros. C1-3's three
/// directions need two different bases at the same offsets — a zero run to grow
/// the chunk count out of, and noise to shrink it into — and the specification
/// fixes the offsets, not the base content. The choice is declared here rather
/// than hidden in a driver.
pub fn structured(len: u64, seed: u64, zero_region: bool) -> Vec<u8> {
    let mut out = noise(len, seed);
    if zero_region {
        let (start, end) = STRUCTURED_ZERO_REGION;
        let start = start.min(len) as usize;
        let end = end.min(len) as usize;
        for byte in &mut out[start..end] {
            *byte = 0;
        }
    }
    out
}

/// The fixture profiles a registry row can name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    /// Uniformly small files, all below the cutoff.
    CompactV2,
    /// Mixed sizes straddling the cutoff.
    MixedV4,
    /// A second mixed shape.
    MixedV3,
    /// Compact, third revision.
    CompactV3,
    /// Text-like content.
    TextV1,
    /// Low-cardinality content.
    LowV1,
    /// A 128-file base.
    Base128V3,
    /// No profile: the row carries its own shape.
    None,
}

impl Profile {
    /// Parses the profile token a registry row carries.
    pub fn parse(token: &str) -> Self {
        match token {
            "compact-v2" => Self::CompactV2,
            "mixed-v4" => Self::MixedV4,
            "mixed-v3" => Self::MixedV3,
            "compact-v3" => Self::CompactV3,
            "text-v1" => Self::TextV1,
            "low-v1" => Self::LowV1,
            "base128-v3" => Self::Base128V3,
            _ => Self::None,
        }
    }
}

/// One generated file: its bytes and the expectation derived from the recipe.
pub struct Fixture {
    /// Logical bytes.
    pub bytes: Vec<u8>,
    /// Recipe-derived expectation.
    pub expectation: Expectation,
}

impl Fixture {
    /// Builds a fixture from bytes that were just generated.
    pub fn of(bytes: Vec<u8>) -> Self {
        let expectation = Expectation::of(&bytes);
        Self { bytes, expectation }
    }

    /// Logical length.
    pub fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Whether the fixture is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Builds the census for an entry-ladder fixture.
///
/// `count` entries of `sizes[index % sizes.len()]` bytes, named `f00000` upward,
/// in one directory. The census is the recipe: it is returned here, before any
/// product call, so the oracle compares the product against the recipe and not
/// against itself.
pub fn census_of_entries(count: u32, sizes: &[u64], directory: &str) -> Census {
    let mut entries = Vec::with_capacity(count as usize);
    for index in 0..count {
        let len = sizes.get(index as usize % sizes.len()).copied().unwrap_or(0);
        entries.push(CensusEntry {
            key: format!("{directory}/f{index:05}"),
            len,
        });
    }
    Census { entries }
}

/// The compact profile's per-file sizes: every file is below the cutoff.
pub const COMPACT_SIZES: [u64; 4] = [64, 512, 2_048, 8_192];

/// The mixed profile's per-file sizes: the ladder straddles the cutoff.
pub const MIXED_SIZES: [u64; 4] = [64, 4_096, 131_071, 131_072];

/// The text profile's per-file sizes.
pub const TEXT_SIZES: [u64; 3] = [1_024, 8_192, 32_768];

/// Per-file size ladder for a profile.
pub fn sizes_for(profile: Profile) -> &'static [u64] {
    match profile {
        Profile::MixedV3 | Profile::MixedV4 => &MIXED_SIZES,
        Profile::TextV1 => &TEXT_SIZES,
        _ => &COMPACT_SIZES,
    }
}

/// Builds the bytes of one fixture file from a profile and an ordinal.
pub fn entry_bytes(profile: Profile, ordinal: u32, len: u64, seed: u64) -> Vec<u8> {
    match profile {
        Profile::TextV1 => text(len, seed ^ u64::from(ordinal)),
        Profile::LowV1 => {
            // Low cardinality: a short repeating alphabet, so many entries share
            // content and the reuse claim has something to reuse.
            let alphabet = [b'a', b'b', b'c', b'd'];
            (0..len)
                .map(|index| alphabet[(ordinal as usize + index as usize) % alphabet.len()])
                .collect()
        }
        _ => noise(len, seed ^ u64::from(ordinal)),
    }
}
