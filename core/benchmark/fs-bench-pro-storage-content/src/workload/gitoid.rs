//! SHA-1 and the Git blob identity — the second hand-rolled digest in the harness.
//!
//! The retained-history corpus names every changed blob by its **Git object id**:
//! `inputs/<sha>/blobs/<oid>` and the `oid` column of `manifest.tsv`. A blob that
//! does not hash back to the name it is filed under is corrupt, and the corpus must
//! refuse it rather than construct content from it (`preparation.md` §8 test 5,
//! `HistoryError::BlobIdentity`). Recomputing that identity needs SHA-1, which the
//! harness does not have.
//!
//! **Why not a crate.** `shared/test_lock_parity.py` fails a *harness-only registry
//! package*, and neither `sha1` nor `sha1_smol` is in `core/Cargo.lock`, so linking
//! one would make the harness link a crate the product seal has never seen. This is
//! the same trade `workload/digest.rs` records for SHA-256, and the reason is the
//! same: the harness's own identity function must not be the product's, and it must
//! not move the lockfile either.
//!
//! SHA-1 is used here **only** as an identity function over corpus bytes, which is
//! what Git uses it for. Nothing in this campaign is authenticated by it.

/// Round constants: `floor(2^30 * sqrt(n))` for `n` in 2..5.
const K: [u32; 4] = [0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xca62c1d6];

/// Initial hash value, fixed by FIPS 180-1.
const H0: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

/// Streaming SHA-1.
#[derive(Clone)]
pub struct Sha1 {
    state: [u32; 5],
    buffer: [u8; 64],
    buffered: usize,
    length_bytes: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1 {
    /// Empty hasher.
    pub const fn new() -> Self {
        Self {
            state: H0,
            buffer: [0; 64],
            buffered: 0,
            length_bytes: 0,
        }
    }

    /// Absorbs bytes.
    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length_bytes = self.length_bytes.wrapping_add(bytes.len() as u64);
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(bytes.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&bytes[..take]);
            self.buffered += take;
            bytes = &bytes[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while bytes.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&bytes[..64]);
            self.compress(&block);
            bytes = &bytes[64..];
        }
        if !bytes.is_empty() {
            self.buffer[..bytes.len()].copy_from_slice(bytes);
            self.buffered = bytes.len();
        }
    }

    /// Finishes and returns the 20-byte digest.
    pub fn finish(mut self) -> [u8; 20] {
        let bits = self.length_bytes.wrapping_mul(8);
        self.update(&[0x80]);
        while self.buffered != 56 {
            self.update(&[0x00]);
        }
        // `update` has already counted the padding into `length_bytes`; the length
        // field is the message length *before* padding, which is what `bits` holds.
        let mut tail = [0u8; 8];
        tail.copy_from_slice(&bits.to_be_bytes());
        let buffered = self.buffered;
        self.buffer[buffered..buffered + 8].copy_from_slice(&tail);
        let block = self.buffer;
        self.compress(&block);

        let mut out = [0u8; 20];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut schedule = [0u32; 80];
        for index in 0..16 {
            schedule[index] = u32::from_be_bytes([
                block[index * 4],
                block[index * 4 + 1],
                block[index * 4 + 2],
                block[index * 4 + 3],
            ]);
        }
        for index in 16..80 {
            schedule[index] = (schedule[index - 3]
                ^ schedule[index - 8]
                ^ schedule[index - 14]
                ^ schedule[index - 16])
                .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = self.state;
        for (index, word) in schedule.iter().enumerate() {
            let (function, constant) = match index / 20 {
                0 => ((b & c) | ((!b) & d), K[0]),
                1 => (b ^ c ^ d, K[1]),
                2 => ((b & c) | (b & d) | (c & d), K[2]),
                _ => (b ^ c ^ d, K[3]),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// One-shot SHA-1 of a byte slice.
pub fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hasher.finish()
}

/// The Git object id of a blob with these bytes: `sha1("blob <len>\0" || bytes)`.
///
/// The header is hashed rather than prefixed into a copy, so a 1.2 MB blob costs no
/// second buffer — which matters because the corpus's largest blob is 1,241,221 B
/// and the reader hashes every blob it serves.
pub fn blob_oid(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hasher.finish()
}

/// Lowercase hex of a 20-byte object id.
pub fn hex_oid(oid: &[u8; 20]) -> String {
    let mut out = String::with_capacity(40);
    for byte in oid {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// Parses 40 hex characters into an object id.
pub fn parse_oid(text: &str) -> Option<[u8; 20]> {
    let bytes = text.as_bytes();
    if bytes.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for index in 0..20 {
        let high = (bytes[index * 2] as char).to_digit(16)?;
        let low = (bytes[index * 2 + 1] as char).to_digit(16)?;
        out[index] = ((high << 4) | low) as u8;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three vectors FIPS 180-1 publishes, plus the empty string.
    #[test]
    fn sha1_matches_the_published_vectors() {
        assert_eq!(hex_oid(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex_oid(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex_oid(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex_oid(&sha1(&million)),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    /// A digest that changed with the block boundary would pass the short vectors
    /// and fail every real blob, so the boundaries are checked directly.
    #[test]
    fn the_buffer_boundaries_are_exact() {
        for length in [0usize, 1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129] {
            let bytes = vec![b'x'; length];
            let one_shot = sha1(&bytes);
            // The same bytes delivered one at a time must agree.
            let mut streaming = Sha1::new();
            for byte in &bytes {
                streaming.update(&[*byte]);
            }
            assert_eq!(streaming.finish(), one_shot, "length {length}");
            // And split at an awkward offset.
            let mut split = Sha1::new();
            let cut = length / 3;
            split.update(&bytes[..cut]);
            split.update(&bytes[cut..]);
            assert_eq!(split.finish(), one_shot, "split length {length}");
        }
    }

    /// The blob header is what makes a Git id a Git id; without it the digest is
    /// just SHA-1 of the content and every oid in the corpus would be refused.
    #[test]
    fn a_blob_oid_is_the_header_plus_the_bytes() {
        // `git hash-object` of the empty file, and of "hello\n".
        assert_eq!(
            hex_oid(&blob_oid(b"")),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            hex_oid(&blob_oid(b"hello\n")),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
        assert_ne!(blob_oid(b"abc"), sha1(b"abc"));
    }

    #[test]
    fn object_ids_parse_and_render_round_trip() {
        let oid = blob_oid(b"content");
        assert_eq!(parse_oid(&hex_oid(&oid)), Some(oid));
        assert_eq!(parse_oid(""), None);
        assert_eq!(parse_oid("zz"), None);
        assert_eq!(parse_oid(&"g".repeat(40)), None);
        assert_eq!(parse_oid(&"a".repeat(39)), None);
    }
}
