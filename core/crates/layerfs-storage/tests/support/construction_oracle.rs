//! Independent finite LFCS v1 framing/digest oracle. No LayerFS types or calls.
//!
//! Inputs are the issued native association and literal deterministic roots.
//! The oracle supplies expected bytes; candidate seals/records never supply them.

pub fn root(serial: u64) -> [u8; 32] {
    let mut bytes = [0x6d; 32];
    bytes[..8].copy_from_slice(&serial.to_be_bytes());
    bytes[8..16].copy_from_slice(&serial.wrapping_mul(0x9e37_79b9).to_be_bytes());
    bytes
}

pub fn scope(selector: [u8; 32], token: u64, binding: [u8; 32], phase: u64) -> [u8; 81] {
    let mut bytes = [0; 81];
    bytes[..32].copy_from_slice(&selector);
    bytes[32..40].copy_from_slice(&token.to_be_bytes());
    bytes[40..72].copy_from_slice(&binding);
    bytes[72..80].copy_from_slice(&phase.to_be_bytes());
    bytes[80] = 1;
    bytes
}

pub fn key(scope: &[u8; 81], serial: u64) -> [u8; 25] {
    let mut bytes = [0; 25];
    bytes[..8].copy_from_slice(&scope[32..40]);
    bytes[8..16].copy_from_slice(&scope[72..80]);
    bytes[16] = 1;
    bytes[17..].copy_from_slice(&serial.to_be_bytes());
    bytes
}

pub fn record(scope: &[u8; 81], serial: u64) -> [u8; 63] {
    let mut bytes = [0; 63];
    bytes[..2].copy_from_slice(&25u16.to_be_bytes());
    bytes[2..27].copy_from_slice(&key(scope, serial));
    bytes[27..31].copy_from_slice(&32u32.to_be_bytes());
    bytes[31..].copy_from_slice(&root(serial));
    bytes
}

pub struct Transcript {
    scope: [u8; 81],
    digest: blake3::Hasher,
    count: u64,
}

impl Transcript {
    pub fn new(scope: [u8; 81]) -> Self {
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/indexed-state/v1\0");
        digest.update(&scope);
        Self {
            scope,
            digest,
            count: 0,
        }
    }

    pub fn append(&mut self, serial: u64) {
        self.digest.update(&record(&self.scope, serial));
        self.count += 1;
    }

    pub fn seal(&self) -> [u8; 130] {
        let mut digest = self.digest.clone();
        digest.update(&self.count.to_be_bytes());
        digest.update(&(self.count * 63).to_be_bytes());
        let mut bytes = [0; 130];
        bytes[0] = 1;
        bytes[1..82].copy_from_slice(&self.scope);
        bytes[82..90].copy_from_slice(&self.count.to_be_bytes());
        bytes[90..98].copy_from_slice(&(self.count * 63).to_be_bytes());
        bytes[98..].copy_from_slice(digest.finalize().as_bytes());
        bytes
    }
}

pub fn page_header(seal: &[u8; 130], last: Option<[u8; 25]>, count: u16, eof: bool) -> [u8; 163] {
    let mut bytes = [0; 163];
    bytes[..130].copy_from_slice(seal);
    if let Some(last) = last {
        bytes[130] = 1;
        bytes[131..156].copy_from_slice(&last);
    }
    bytes[156..158].copy_from_slice(&count.to_be_bytes());
    bytes[158..162].copy_from_slice(&(u32::from(count) * 63).to_be_bytes());
    bytes[162] = u8::from(eof);
    bytes
}
