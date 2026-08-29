use std::io::Cursor;

use blake3::Hasher;
use layerfs_content::file::cdc::FastCdc;
use layerfs_content::ObjectId;

const SEED: u64 = 0x4c41594552534653;
const K: usize = 64;
const F: usize = 64;
const CONTEXT: &str = "layerfs/canonical-v2/ordered-occurrence/v1";

#[derive(Clone, Copy)]
struct Reference {
    offset: usize,
    length: u32,
    id: ObjectId,
}

#[derive(Clone)]
struct Node {
    id: ObjectId,
    canonical: Vec<u8>,
    total: u64,
    refs: std::ops::Range<usize>,
    children: Vec<Node>,
    level: u8,
}

struct Oracle {
    label: &'static str,
    source_fingerprint: String,
    raw_sequence: String,
    commitment: String,
    corpus: String,
    reconstruction: String,
    range: String,
    references: usize,
    leaves: usize,
    branches: usize,
    level: u8,
    mapping_bytes: usize,
    file_root: ObjectId,
    workspace_root: ObjectId,
    transition: ObjectId,
    closure: String,
}

fn retained_source(size: usize, label: &str) -> Vec<u8> {
    let salt = label
        .bytes()
        .fold(0_u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    let mut output = vec![0_u8; size];
    for (block, buffer) in output.chunks_mut(1024 * 1024).enumerate() {
        let offset = (block * 1024 * 1024) as u64;
        let mut state = SEED ^ salt ^ offset;
        for (index, byte) in buffer.iter_mut().enumerate() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let position = offset + index as u64;
            *byte = if (position / 8192) % 23 == 0 {
                (salt as u8).wrapping_add((position / 8192) as u8)
            } else {
                (state >> 24) as u8
            };
        }
    }
    output
}

fn bytes_object(value: &[u8]) -> Vec<u8> {
    let payload = 4 + value.len();
    let mut output = Vec::with_capacity(9 + payload);
    output.extend_from_slice(b"LFSO");
    output.push(1);
    output.extend_from_slice(&(payload as u32).to_be_bytes());
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
    output
}

fn mapping(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(11 + body.len());
    inner.extend_from_slice(b"LFS4MAP\0");
    inner.extend_from_slice(&2_u16.to_be_bytes());
    inner.push(tag);
    inner.extend_from_slice(body);
    bytes_object(&inner)
}

fn leaf(references: &[Reference], first: usize) -> Node {
    let mut body = Vec::with_capacity(4 + references.len() * 36);
    body.extend_from_slice(&(references.len() as u32).to_be_bytes());
    for reference in references {
        body.extend_from_slice(&reference.length.to_be_bytes());
        body.extend_from_slice(reference.id.as_bytes());
    }
    let canonical = mapping(2, &body);
    Node {
        id: ObjectId::for_bytes(&canonical),
        canonical,
        total: references
            .iter()
            .map(|reference| u64::from(reference.length))
            .sum(),
        refs: first..first + references.len(),
        children: Vec::new(),
        level: 0,
    }
}

fn branch(children: Vec<Node>, level: u8) -> Node {
    let mut body = Vec::with_capacity(5 + children.len() * 40);
    body.push(level);
    body.extend_from_slice(&(children.len() as u32).to_be_bytes());
    let mut end = 0_u64;
    for child in &children {
        end += child.total;
        body.extend_from_slice(&end.to_be_bytes());
        body.extend_from_slice(child.id.as_bytes());
    }
    let canonical = mapping(7, &body);
    let refs = children.first().map_or(0, |child| child.refs.start)
        ..children.last().map_or(0, |child| child.refs.end);
    Node {
        id: ObjectId::for_bytes(&canonical),
        canonical,
        total: end,
        refs,
        children,
        level,
    }
}

fn root(total: u64, count: usize, level: u8, children: &[Node]) -> Vec<u8> {
    let mut body = Vec::with_capacity(25 + children.len() * 40);
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(&(count as u64).to_be_bytes());
    body.push(level);
    body.extend_from_slice(&(children.len() as u32).to_be_bytes());
    let mut end = 0_u64;
    for child in children {
        end += child.total;
        body.extend_from_slice(&end.to_be_bytes());
        body.extend_from_slice(child.id.as_bytes());
    }
    mapping(1, &body)
}

fn namespace(file_root: ObjectId) -> Vec<u8> {
    let mut payload = Vec::with_capacity(45);
    payload.extend_from_slice(&1_u32.to_be_bytes());
    payload.extend_from_slice(&4_u32.to_be_bytes());
    payload.extend_from_slice(b"file");
    payload.push(1);
    payload.extend_from_slice(file_root.as_bytes());
    let mut output = Vec::with_capacity(54);
    output.extend_from_slice(b"LFSO");
    output.push(2);
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    output.extend_from_slice(&payload);
    output
}

fn genesis(child: ObjectId) -> Vec<u8> {
    let mut body = Vec::with_capacity(41);
    body.push(0);
    body.extend_from_slice(child.as_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    mapping(5, &body)
}

fn observe(hasher: &mut Hasher, role: &[u8], id: ObjectId, canonical: &[u8]) {
    hasher.update(&(role.len() as u64).to_be_bytes());
    hasher.update(role);
    hasher.update(id.as_bytes());
    hasher.update(&(canonical.len() as u64).to_be_bytes());
    hasher.update(canonical);
}

fn corpus_item(
    hasher: &mut Hasher,
    ordinal: &mut u64,
    role: &[u8],
    id: ObjectId,
    canonical: &[u8],
) {
    hasher.update(&(role.len() as u64).to_be_bytes());
    hasher.update(role);
    hasher.update(&ordinal.to_be_bytes());
    hasher.update(&(canonical.len() as u64).to_be_bytes());
    hasher.update(id.as_bytes());
    hasher.update(canonical);
    *ordinal += 1;
}

fn walk(
    node: &Node,
    source: &[u8],
    references: &[Reference],
    closure: &mut Hasher,
    corpus: &mut Hasher,
    ordinal: &mut u64,
) {
    observe(closure, b"file-mapping", node.id, &node.canonical);
    corpus_item(corpus, ordinal, b"file-mapping", node.id, &node.canonical);
    if node.level == 0 {
        for reference in &references[node.refs.clone()] {
            let start = reference.offset;
            let end = start + reference.length as usize;
            let canonical = bytes_object(&source[start..end]);
            observe(closure, b"file-chunk", reference.id, &canonical);
            corpus_item(corpus, ordinal, b"file-chunk", reference.id, &canonical);
        }
    } else {
        for child in &node.children {
            walk(child, source, references, closure, corpus, ordinal);
        }
    }
}

fn oracle(size: usize, label: &'static str) -> Oracle {
    let source = retained_source(size, label);
    let source_fingerprint = blake3::hash(&source).to_hex().to_string();
    let reconstruction = source_fingerprint.clone();
    let range_start = (size - 1024 * 1024) / 2;
    let range = blake3::hash(&source[range_start..range_start + 1024 * 1024])
        .to_hex()
        .to_string();
    let mut references = Vec::new();
    let mut offset = 0usize;
    let mut raw_sequence = Hasher::new();
    let mut commitment = Hasher::new_derive_key(CONTEXT);
    FastCdc::new()
        .scan(Cursor::new(&source), |chunk| {
            let length = u32::try_from(chunk.len()).unwrap();
            let canonical = bytes_object(chunk);
            let id = ObjectId::for_bytes(&canonical);
            raw_sequence.update(&length.to_be_bytes());
            raw_sequence.update(ObjectId::for_bytes(chunk).as_bytes());
            commitment.update(&length.to_be_bytes());
            commitment.update(id.as_bytes());
            references.push(Reference { offset, length, id });
            offset += chunk.len();
            Ok(())
        })
        .unwrap();

    let leaves = references
        .chunks(K)
        .enumerate()
        .map(|(index, chunk)| leaf(chunk, index * K))
        .collect::<Vec<_>>();
    let leaf_count = leaves.len();
    let mut level = 0_u8;
    let mut nodes = leaves;
    let mut branch_count = 0usize;
    while nodes.len() > F {
        level += 1;
        let mut next = Vec::new();
        for group in nodes.chunks(F) {
            next.push(branch(group.to_vec(), level));
            branch_count += 1;
        }
        nodes = next;
    }
    let total = source.len() as u64;
    let file_root_bytes = root(total, references.len(), level, &nodes);
    let file_root = ObjectId::for_bytes(&file_root_bytes);
    let workspace = namespace(file_root);
    let workspace_root = ObjectId::for_bytes(&workspace);
    let transition_bytes = genesis(workspace_root);
    let transition = ObjectId::for_bytes(&transition_bytes);
    let mut content_closure = Hasher::new();
    let mut corpus = Hasher::new();
    let mut ordinal = 0_u64;
    observe(
        &mut content_closure,
        b"namespace-root",
        workspace_root,
        &workspace,
    );
    corpus_item(
        &mut corpus,
        &mut ordinal,
        b"namespace-root",
        workspace_root,
        &workspace,
    );
    observe(
        &mut content_closure,
        b"file-root",
        file_root,
        &file_root_bytes,
    );
    corpus_item(
        &mut corpus,
        &mut ordinal,
        b"file-root",
        file_root,
        &file_root_bytes,
    );
    for node in &nodes {
        walk(
            node,
            &source,
            &references,
            &mut content_closure,
            &mut corpus,
            &mut ordinal,
        );
    }
    let content_closure = content_closure.finalize();
    let mut transition_closure = Hasher::new();
    observe(
        &mut transition_closure,
        b"transition",
        transition,
        &transition_bytes,
    );
    corpus_item(
        &mut corpus,
        &mut ordinal,
        b"transition",
        transition,
        &transition_bytes,
    );
    observe(
        &mut transition_closure,
        b"transition-child",
        workspace_root,
        &workspace,
    );
    corpus_item(
        &mut corpus,
        &mut ordinal,
        b"transition-child",
        workspace_root,
        &workspace,
    );
    let mut closure = Hasher::new();
    closure.update(b"layerfs/wp4m/ordered-closure/v1\0");
    closure.update(transition_closure.finalize().as_bytes());
    closure.update(content_closure.as_bytes());

    let mapping_bytes = file_root_bytes.len()
        + nodes
            .iter()
            .map(|node| {
                fn total(node: &Node) -> usize {
                    node.canonical.len() + node.children.iter().map(total).sum::<usize>()
                }
                total(node)
            })
            .sum::<usize>();
    Oracle {
        label,
        source_fingerprint,
        raw_sequence: raw_sequence.finalize().to_hex().to_string(),
        commitment: commitment.finalize().to_hex().to_string(),
        corpus: corpus.finalize().to_hex().to_string(),
        reconstruction,
        range,
        references: references.len(),
        leaves: leaf_count,
        branches: branch_count,
        level,
        mapping_bytes,
        file_root,
        workspace_root,
        transition,
        closure: closure.finalize().to_hex().to_string(),
    }
}

#[test]
fn independent_actual_fixture_oracle_freezes_complete_v2_corpus() {
    let expected = [
        (
            1024 * 1024,
            "S1-1",
            "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
            "6a1d02f70694a50859c88c0080f0e2cc046c8b0d9e21f474c58dab66a895f1c1",
            53,
            1,
            0,
            0,
            2_025,
            "c2b4a92188569d206717210b596dde9b8aeade1c9c81b87f02b8d0d6ebda1112",
            "b0266bbda936c1532c04fc0155f1efef2fb63d69afb5647952e8f4a10060ab20",
            "2274f609bfbd578a600da5e07b1deed6ff2c9a77927eaba854b0ebf7ab542142",
            "18f33e3ca6030e966cf8ed41c0b43f4769de8b02247f453fae447627bee4b77c",
            "60d191810b303b26d12453add0b9e1718b1f1b654473615d9323f0ee477a9b7d",
            "7e806f7023c3e33914c59d2b0d0d84bca8859fdbd7663b55f5f5c99313252d42",
            "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
        ),
        (
            10 * 1024 * 1024,
            "S1-10",
            "e40db05d7407b92253e56099df402f03b399990014b2d1397e422ca305472449",
            "982e992203cd527c1b7147e4e9509bcd2e5828706fc2313f18bcfe1b4de2f3ed",
            531,
            9,
            0,
            0,
            19_777,
            "8eb047a5d7ac6cc86c26d30d014c46f722936147a0989683303057c96fbec67c",
            "f119169a3aee39fdac17b72197dd5429155a34524ec7b02af421037e8deace08",
            "8ad4351bb76bac1b0a80e279d8a5225a5ff752bce73c569f70daa7a15b79a0bf",
            "003fac659363e97667cc75fa8fb81fef7065b856c547440e22722b76c1e72342",
            "001cdef1e85c266038e98bc86e8470dc1b9d21e021bac1abd0d03e994e42c440",
            "35282fcfecc493c025a3bc4a7567efc12562fc8a4d863c88e07617fb5e97d1c9",
            "08a1490b77d2afcc6fc9149e24bfe66696735791d7a7c370d2e1a31076ecaa1d",
        ),
        (
            100 * 1024 * 1024,
            "S1-100",
            "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7",
            "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994",
            5_284,
            83,
            2,
            1,
            196_055,
            "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2",
            "c7107d5f0ecd8bd8a9efe11bde900aa50dbbff49dfc3122000835dc1323e1ecd",
            "6f923dfa4f32981884af0437476f9c4e8b7f4bb1af84ecc6420a48daa455713c",
            "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1",
            "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89",
            "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
            "0a4b6b60703a8b25d01b990ec346f5ab26661367c56de210b21769947692cd0f",
        ),
    ];
    for (
        size,
        label,
        fingerprint,
        raw_sequence,
        refs,
        leaves,
        branches,
        level,
        mapping_bytes,
        commitment,
        corpus,
        file_root,
        workspace_root,
        transition,
        closure,
        range,
    ) in expected
    {
        let actual = oracle(size, label);
        println!(
            "{label}\tcommitment={}\tcorpus={}\tfile_root={}\tworkspace_root={}\ttransition={}\tclosure={}\trange={}",
            actual.commitment,
            actual.corpus,
            actual.file_root,
            actual.workspace_root,
            actual.transition,
            actual.closure,
            actual.range,
        );
        assert_eq!(actual.label, label);
        assert_eq!(actual.source_fingerprint, fingerprint);
        assert_eq!(actual.raw_sequence, raw_sequence);
        assert_eq!(actual.references, refs);
        assert_eq!(actual.leaves, leaves);
        assert_eq!(actual.branches, branches);
        assert_eq!(actual.level, level);
        assert_eq!(actual.mapping_bytes, mapping_bytes);
        assert_eq!(
            actual.mapping_bytes + 119,
            match label {
                "S1-1" => 2_144,
                "S1-10" => 19_896,
                "S1-100" => 196_174,
                _ => unreachable!(),
            }
        );
        assert_eq!(actual.reconstruction, fingerprint);
        assert_eq!(actual.commitment, commitment);
        assert_eq!(actual.corpus, corpus);
        assert_eq!(actual.file_root.to_string(), file_root);
        assert_eq!(actual.workspace_root.to_string(), workspace_root);
        assert_eq!(actual.transition.to_string(), transition);
        assert_eq!(actual.closure, closure);
        assert_eq!(actual.range, range);
    }
}
