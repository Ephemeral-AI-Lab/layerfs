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
