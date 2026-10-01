//! Literal finite LFCS3 framing/digests. No candidate types/codecs/ledgers.

pub fn scope(selector: [u8; 32], token: u64, binding: [u8; 32], source: [u8; 8]) -> [u8; 89] {
    let mut bytes = [0; 89];
    bytes[..32].copy_from_slice(&selector);
    bytes[32..40].copy_from_slice(&token.to_be_bytes());
    bytes[40..72].copy_from_slice(&binding);
    bytes[72..80].copy_from_slice(&1u64.to_be_bytes());
    bytes[80] = 3;
    bytes[81..].copy_from_slice(&source);
    bytes
}

pub fn key(scope: &[u8; 89], serial: u64) -> [u8; 25] {
    let mut bytes = [0; 25];
    bytes[..8].copy_from_slice(&scope[32..40]);
    bytes[8..16].copy_from_slice(&1u64.to_be_bytes());
    bytes[16] = 3;
    bytes[17..].copy_from_slice(&serial.to_be_bytes());
    bytes
}

pub fn point(source: [u8; 8], parent: u64, descriptor: u64, ordinal: u32) -> [u8; 28] {
    let mut bytes = [0; 28];
    bytes[..8].copy_from_slice(&source);
    bytes[8..16].copy_from_slice(&parent.to_be_bytes());
    bytes[16..24].copy_from_slice(&descriptor.to_be_bytes());
    bytes[24..].copy_from_slice(&ordinal.to_be_bytes());
    bytes
}

pub fn record(scope: &[u8; 89], serial: u64, flags: u8, point: [u8; 28]) -> [u8; 60] {
    let mut bytes = [0; 60];
    bytes[..2].copy_from_slice(&25u16.to_be_bytes());
    bytes[2..27].copy_from_slice(&key(scope, serial));
    bytes[27..31].copy_from_slice(&29u32.to_be_bytes());
    bytes[31] = flags;
    bytes[32..].copy_from_slice(&point);
    bytes
}

pub struct Transcript {
    scope: [u8; 89],
    count: u64,
    digest: blake3::Hasher,
}

impl Transcript {
    pub fn new(scope: [u8; 89], final_flags: bool) -> Self {
        let mut digest = blake3::Hasher::new();
        digest.update(if final_flags {
            b"layerfs/binding-sites/final/v1\0"
        } else {
            b"layerfs/binding-sites/birth/v1\0"
        });
        digest.update(&scope);
        Self {
            scope,
            count: 0,
            digest,
        }
    }

    pub fn append(&mut self, serial: u64, flags: u8, point: [u8; 28]) {
        self.digest
            .update(&record(&self.scope, serial, flags, point));
        self.count += 1;
    }

    pub fn seal(&self) -> [u8; 138] {
        let mut digest = self.digest.clone();
        digest.update(&self.count.to_be_bytes());
        digest.update(&(self.count * 60).to_be_bytes());
        let mut bytes = [0; 138];
        bytes[0] = 1;
        bytes[1..90].copy_from_slice(&self.scope);
        bytes[90..98].copy_from_slice(&self.count.to_be_bytes());
        bytes[98..106].copy_from_slice(&(self.count * 60).to_be_bytes());
        bytes[106..].copy_from_slice(digest.finalize().as_bytes());
        bytes
    }
}

pub fn membership(birth: &[u8; 138], maximum: Option<[u8; 25]>) -> [u8; 164] {
    let mut bytes = [0; 164];
    bytes[..138].copy_from_slice(birth);
    if let Some(maximum) = maximum {
        bytes[138] = 1;
        bytes[139..].copy_from_slice(&maximum);
    }
    bytes
}

pub fn page(seal: &[u8; 138], last: Option<[u8; 25]>, count: u16, eof: bool) -> [u8; 171] {
    let mut bytes = [0; 171];
    bytes[..138].copy_from_slice(seal);
    if let Some(last) = last {
        bytes[138] = 1;
        bytes[139..164].copy_from_slice(&last);
    }
    bytes[164..166].copy_from_slice(&count.to_be_bytes());
    bytes[166..170].copy_from_slice(&(u32::from(count) * 60).to_be_bytes());
    bytes[170] = u8::from(eof);
    bytes
}

pub fn parent_page(
    members: &[u8; 164],
    parent: u64,
    last: Option<u32>,
    maximum: Option<u32>,
    count: u16,
    eof: bool,
) -> [u8; 189] {
    let mut bytes = [0; 189];
    bytes[..164].copy_from_slice(members);
    bytes[164..172].copy_from_slice(&parent.to_be_bytes());
    if let Some(last) = last {
        bytes[172] = 1;
        bytes[173..177].copy_from_slice(&last.to_be_bytes());
    }
    if let Some(maximum) = maximum {
        bytes[177] = 1;
        bytes[178..182].copy_from_slice(&maximum.to_be_bytes());
    }
    bytes[182..184].copy_from_slice(&count.to_be_bytes());
    bytes[184..188].copy_from_slice(&(u32::from(count) * 60).to_be_bytes());
    bytes[188] = u8::from(eof);
    bytes
}
