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

fn chunk_object(value: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::with_capacity(8 + value.len());
    chunk.extend_from_slice(b"LFS4CHK\0");
    chunk.extend_from_slice(value);
    bytes_object(&chunk)
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
            let canonical = chunk_object(&source[start..end]);
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
            let canonical = chunk_object(chunk);
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
            "77ff53eb762c4e3ca98208c241ab54dea8db267bc321b22a6ab25a9af6d414a0",
            53,
            1,
            0,
            0,
            2_025,
            "e074a65048cbbdb9e7e30589a16f5b1459c630f0b27f8c080d816c21531dd985",
            "55556ec86bca1f459795eb31474200eff52d603df0baa1cb930ac6728932a292",
            "1e273a1957af2647663e1f983209ff231c00e486a76135d2ca0c6c3083fa73bb",
            "82e78c92e4c71fd61d681c056a5b64d0d37e4f1f231ea14445dbb0f720644c9e",
            "6d1fa7dd6ead3d51f088e1c4122983ee35712a96bb7bc37f08239cbe53dc2832",
            "eb2061eacfd658373a5ac24f5a0481c4a09e7fca1a8868944ce0de294ec619b9",
            "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
        ),
        (
            10 * 1024 * 1024,
            "S1-10",
            "e40db05d7407b92253e56099df402f03b399990014b2d1397e422ca305472449",
            "797a2b7bb2c7bdac86302b1931f5a241c947b283203ff993a5ae8c1eda4946fa",
            531,
            9,
            0,
            0,
            19_777,
            "1de74c92d958a116f38ddbaf527d9d74a0aca1d0bd41f115a4188d2ae959a709",
            "8c11b3ff6ea68013f95ca14deb67f6fb7098c163c80a3c06a41085dcaf2bbe78",
            "887518e17ae5c9dcfb22c4a96fea70c0bcb54f7e17ad1a8f26f62595a2dd954f",
            "a523df3dadd3a398116a133c21c40068ccd9757620f4cf69d7fcbb6d430eda47",
            "2c82004f87c37c0b58723e30b066a2b40fd123803f3701b4b0c1973cef0b6542",
            "813417c95ad0ba809e0e27e5854722296e431887cff894d69ab734cb922c271d",
            "08a1490b77d2afcc6fc9149e24bfe66696735791d7a7c370d2e1a31076ecaa1d",
        ),
        (
            100 * 1024 * 1024,
            "S1-100",
            "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7",
            "2e0c066ba84c1bf9be2661309bd602ce98d00ec66aa169fe9890367c9f8bbaa1",
            5_284,
            83,
            2,
            1,
            196_055,
            "4a224c5b09e8816a8fb7c45ad8a2792d65ff456f8ed643489a3ebd7ab8fbe232",
            "057d320a87ead610a4bfd9fb89fd11260d73ac21c606a8dfe7134e2665cce8b8",
            "345cfc1741056cb2e3156e4e85cb5779966acdd1deeadb1aa8a0ba486a623fe2",
            "3e31be67d3d838f37f4f51e63963be780b3d22dee604a95804c0bbf7ef44c85b",
            "90f709a28b35ac9d338e562da38ba7837dff9f5484ea9364b161fc5d2b01dd88",
            "11421674d69d01c37d28184dcc6e6f5eb8a34116a52747fc1f795de46ada35b5",
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
