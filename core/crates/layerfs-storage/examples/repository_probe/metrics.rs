//! Fixed-size external attribution counters; no product hooks or input registry.
use std::time::Instant;

pub const NAMES: [&str; 12] = [
    "sql_membership",
    "sql_object_insert",
    "sql_locator_prepare",
    "sql_locator_update",
    "sql_pack_insert",
    "sql_file_root_update",
    "sql_transaction",
    "sql_validation",
    "c2_full_encoding",
    "group_build",
    "pack_assembly",
    "pack_hash_and_write",
];
pub const MEMBERSHIP: usize = 0;
pub const OBJECT_INSERT: usize = 1;
pub const LOCATOR_PREPARE: usize = 2;
pub const LOCATOR_UPDATE: usize = 3;
pub const PACK_INSERT: usize = 4;
pub const FILE_ROOT: usize = 5;
pub const TRANSACTION: usize = 6;
pub const VALIDATION: usize = 7;
pub const ENCODE: usize = 8;
pub const GROUP: usize = 9;
pub const ASSEMBLY: usize = 10;
pub const PACK_IO: usize = 11;

#[derive(Clone, Copy, Default)]
pub struct Span {
    pub ns: u128,
    pub calls: u64,
    pub bytes: u64,
}
impl Span {
    pub fn add(&mut self, started: Instant, bytes: u64) {
        self.add_elapsed(started.elapsed().as_nanos(), bytes);
    }
    pub fn add_elapsed(&mut self, ns: u128, bytes: u64) {
        self.ns += ns;
        self.calls += 1;
        self.bytes += bytes;
    }
    pub fn json(self) -> String {
        format!(
            "{{\"elapsed_ns\":{},\"calls\":{},\"bytes\":{}}}",
            self.ns, self.calls, self.bytes
        )
    }
}
#[derive(Clone, Copy, Default)]
pub struct Encoding {
    pub span: Span,
    pub canonical_bytes: u64,
    pub raw_bytes: u64,
    pub stored: u64,
}
#[derive(Clone, Copy, Default)]
pub struct Group {
    pub span: Span,
    pub raw_groups: u64,
    pub compressed_groups: u64,
    pub decoded_bytes: u64,
}
#[derive(Default)]
pub struct Metrics {
    pub spans: [Span; 12],
    pub encodings: [Encoding; 13],
    pub groups: [Group; 5],
    pub consumer: Span,
    pub membership: [Span; 2],
    pub pack_hash: Span,
    pub pack_write: Span,
    pub commits_object: u64,
    pub commits_file: u64,
    pub commits_final: u64,
}
impl Metrics {
    pub fn json(&self, probe_ns: u64) -> String {
        let spans = NAMES
            .iter()
            .zip(self.spans)
            .map(|(name, s)| format!("\"{name}\":{}", s.json()))
            .collect::<Vec<_>>()
            .join(",");
        let encodings=self.encodings.iter().enumerate().map(|(i,e)|format!("{{\"role\":{},\"span\":{},\"canonical_bytes\":{},\"raw_bytes\":{},\"verbatim_records\":{}}}",i+1,e.span.json(),e.canonical_bytes,e.raw_bytes,e.stored)).collect::<Vec<_>>().join(",");
        let groups=self.groups.iter().enumerate().map(|(i,g)|format!("{{\"lane_index\":{i},\"span\":{},\"raw_groups\":{},\"compressed_groups\":{},\"decoded_bytes\":{}}}",g.span.json(),g.raw_groups,g.compressed_groups,g.decoded_bytes)).collect::<Vec<_>>().join(",");
        format!("{{\"spans\":{{{spans}}},\"encoding_roles\":[{encodings}],\"group_lanes\":[{groups}],\"membership_miss\":{},\"membership_hit\":{},\"consumer_inclusive\":{},\"pack_hash\":{},\"pack_write\":{},\"nested_probe_ns\":{probe_ns},\"transaction_batches\":{{\"object\":{},\"file\":{},\"final\":{}}}}}",self.membership[0].json(),self.membership[1].json(),self.consumer.json(),self.pack_hash.json(),self.pack_write.json(),self.commits_object,self.commits_file,self.commits_final)
    }
}
