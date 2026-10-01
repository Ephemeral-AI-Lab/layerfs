//! Independent literal private-v4 framing/BLAKE3 oracle; no candidate codecs.

pub fn subject(source: [u8; 8], update: bool, root: u64, scratch: u64) -> [u8; 106] {
    let mut b = [0; 106];
    b[..8].copy_from_slice(&source);
    b[8..40].fill(0x42);
    b[40] = if update { 2 } else { 1 };
    if update {
        b[41] = 1;
        b[42..74].fill(0x43);
    }
    b[74..82].copy_from_slice(&root.to_be_bytes());
    b[82..90].copy_from_slice(&scratch.to_be_bytes());
    let records = scratch / 256;
    b[90..98].copy_from_slice(&records.to_be_bytes());
    b[98..].copy_from_slice(&(records * 63).to_be_bytes());
    b
}
pub fn scope(selector: [u8; 32], token: u64, binding: [u8; 32], subject: [u8; 106]) -> [u8; 188] {
    let mut b = [0; 188];
    b[..32].copy_from_slice(&selector);
    b[32..40].copy_from_slice(&token.to_be_bytes());
    b[40..72].copy_from_slice(&binding);
    b[72..80].copy_from_slice(&2u64.to_be_bytes());
    b[80] = 4;
    b[81] = 5;
    b[82..].copy_from_slice(&subject);
    b
}
pub fn node_key(scope: &[u8; 188], serial: u64) -> [u8; 25] {
    let mut b = [0; 25];
    b[..8].copy_from_slice(&scope[32..40]);
    b[8..16].copy_from_slice(&2u64.to_be_bytes());
    b[16] = 4;
    b[17..].copy_from_slice(&serial.to_be_bytes());
    b
}
pub fn edge_key(scope: &[u8; 188], parent: u64, child: u64) -> [u8; 33] {
    let mut b = [0; 33];
    b[..17].copy_from_slice(&node_key(scope, parent)[..17]);
    b[16] = 5;
    b[17..25].copy_from_slice(&parent.to_be_bytes());
    b[25..].copy_from_slice(&child.to_be_bytes());
    b
}
pub fn node(scope: &[u8; 188], serial: u64, fields: (u8, u32, u32, u64, u64, u32)) -> [u8; 60] {
    let (flags, discovery, low, parent, after, incoming) = fields;
    let mut b = [0; 60];
    b[..2].copy_from_slice(&25u16.to_be_bytes());
    b[2..27].copy_from_slice(&node_key(scope, serial));
    b[27..31].copy_from_slice(&29u32.to_be_bytes());
    b[31] = flags;
    b[32..36].copy_from_slice(&discovery.to_be_bytes());
    b[36..40].copy_from_slice(&low.to_be_bytes());
    b[40..48].copy_from_slice(&parent.to_be_bytes());
    b[48..56].copy_from_slice(&after.to_be_bytes());
    b[56..].copy_from_slice(&incoming.to_be_bytes());
    b
}
pub fn edge(scope: &[u8; 188], parent: u64, child: u64, multiplicity: u32) -> [u8; 43] {
    let mut b = [0; 43];
    b[..2].copy_from_slice(&33u16.to_be_bytes());
    b[2..35].copy_from_slice(&edge_key(scope, parent, child));
    b[35..39].copy_from_slice(&4u32.to_be_bytes());
    b[39..].copy_from_slice(&multiplicity.to_be_bytes());
    b
}
pub fn transcript<const N: usize>(
    domain: &[u8],
    scope: &[u8; 188],
    records: &[[u8; N]],
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(domain);
    hash.update(scope);
    for record in records {
        hash.update(record);
    }
    hash.update(&(records.len() as u64).to_be_bytes());
    hash.update(&((records.len() * N) as u64).to_be_bytes());
    *hash.finalize().as_bytes()
}
pub fn adjacency(scope: &[u8; 188], nodes: &[[u8; 60]], edges: &[[u8; 43]]) -> [u8; 285] {
    let mut b = [0; 285];
    b[0] = 1;
    b[1..189].copy_from_slice(scope);
    b[189..197].copy_from_slice(&(nodes.len() as u64).to_be_bytes());
    b[197..205].copy_from_slice(&(edges.len() as u64).to_be_bytes());
    b[205..213].copy_from_slice(&((nodes.len() * 60) as u64).to_be_bytes());
    b[213..221].copy_from_slice(&((edges.len() * 43) as u64).to_be_bytes());
    b[221..253].copy_from_slice(&transcript(
        b"layerfs/effective-graph/adjacency-nodes/v1\0",
        scope,
        nodes,
    ));
    b[253..].copy_from_slice(&transcript(
        b"layerfs/effective-graph/adjacency-edges/v1\0",
        scope,
        edges,
    ));
    b
}
pub fn proof(adjacency: &[u8; 285], nodes: &[[u8; 60]]) -> [u8; 317] {
    let mut b = [0; 317];
    b[..285].copy_from_slice(adjacency);
    let scope: [u8; 188] = adjacency[1..189].try_into().unwrap();
    b[221..253].copy_from_slice(&transcript(
        b"layerfs/effective-graph/proof-nodes/v1\0",
        &scope,
        nodes,
    ));
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/effective-graph/adjacency-seal/v1\0");
    hash.update(adjacency);
    b[285..].copy_from_slice(hash.finalize().as_bytes());
    b
}
