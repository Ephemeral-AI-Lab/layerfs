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
