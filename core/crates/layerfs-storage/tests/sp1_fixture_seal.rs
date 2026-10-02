//! Explicit old-producer fixture acquisition; never a candidate oracle generator.
mod support;
use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
};
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use std::{fmt::Write, path::PathBuf};
use support::{disabled, noise, save_one};
fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut out, v| {
        write!(out, "{v:02x}").unwrap();
        out
    })
}
fn payload(role: ObjectRole, bytes: &[u8], base: Option<ObjectId>) -> FinalizedObject {
    let canonical = match role {
        ObjectRole::WholeFile => layerfs_content::encode_whole_file_payload(bytes).unwrap(),
        _ => layerfs_content::file::mapping::encode_chunk_object(bytes).unwrap(),
    };
    predecessor(FinalizedObject::new(role, canonical).unwrap(), base)
}
fn predecessor(o: FinalizedObject, base: Option<ObjectId>) -> FinalizedObject {
    let mut p = AdvisoryPredecessors::new();
    if let Some(id) = base {
        p.push(id, PredecessorProvenance::OriginalBase).unwrap();
    }
    o.with_predecessors(p)
}
#[test]
#[ignore = "explicit acquisition on pinned unchanged C2 only"]
fn seal_old_producer_fixture() {
    let out = PathBuf::from(std::env::var("SP1_SEAL_OUT").expect("fresh evidence output"));
    assert!(!out.join("producer.sqlite").exists());
    std::fs::create_dir_all(&out).unwrap();
    let store = support::create_store(&out.join("producer.sqlite"));
    let mut manifest = String::from("name\tid\trole\tlength\tfull\tprefix\ttrials\tpool_full\tpool_delta\tpool_new\tpool_reuse\n");
    let mut save = |name: &str, o: FinalizedObject| {
        let id = o.id();
        let role = o.role();
        let bytes = o.canonical().to_vec();
        std::fs::write(out.join(format!("{name}.canonical")), &bytes).unwrap();
        let r = save_one(&store, o).unwrap();
        writeln!(
            manifest,
            "{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            hex(id.as_bytes()),
            role.code(),
            bytes.len(),
            r.full_records,
            r.prefix_records,
            r.delta.trials,
            r.pool.full_leaves,
            r.pool.delta_leaves,
            r.pool.new_values,
            r.pool.reused_values
        )
        .unwrap();
        id
    };
    let whole = noise(96000);
    let w0 = save("whole0", payload(ObjectRole::WholeFile, &whole, None));
    let mut w = whole.clone();
    for b in &mut w[64..96] {
        *b ^= 255;
    }
    save("whole1", payload(ObjectRole::WholeFile, &w, Some(w0)));
    let chunk = noise(20000);
    let c0 = save("chunk0", payload(ObjectRole::Chunk, &chunk, None));
    let mut c = chunk.clone();
    for b in &mut c[64..96] {
        *b ^= 255;
    }
    let c1 = save("chunk1", payload(ObjectRole::Chunk, &c, Some(c0)));
    for b in &mut c[128..160] {
        *b ^= 255;
    }
    save("chunk2", payload(ObjectRole::Chunk, &c, Some(c1)));
    save(
        "unrelated",
        payload(
            ObjectRole::Chunk,
            &chunk.iter().map(|b| b ^ 165).collect::<Vec<_>>(),
            Some(c0),
        ),
    );
    let mut base = None;
    for step in 0..4u8 {
        let rows = (0..40u64)
            .map(|seed| {
                let mut b = [0u8; 32];
                b[..8].copy_from_slice(&seed.to_be_bytes());
                let mut value = encode_inode_value(InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: ObjectId::for_bytes(&b),
                    metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
                });
                if seed < 4 {
                    value[1] = value[1].wrapping_add(step);
                }
                InodeLeafRow {
                    serial: seed + 1,
                    value,
                }
            })
            .collect::<Vec<_>>();
        let bytes = InodeLeaf {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        }
        .encode()
        .unwrap();
        base = Some(save(
            &format!("pool{step}"),
            predecessor(
                FinalizedObject::new(ObjectRole::InodeLeaf, bytes).unwrap(),
                base,
            ),
        ));
    }
    std::fs::write(out.join("objects.tsv"), manifest).unwrap();
    for n in [131071, 131072, 200000] {
        let raw = noise(n);
        let (objects, root, _) = support::construct_file(&raw);
        std::fs::write(
            out.join(format!("content-{n}.tsv")),
            objects
                .objects()
                .iter()
                .fold(String::new(), |mut rows, (id, role, b, _)| {
                    writeln!(rows, "{}\t{}\t{}", hex(id.as_bytes()), role.code(), b.len()).unwrap();
                    rows
                }),
        )
        .unwrap();
        std::fs::write(out.join(format!("root-{n}.txt")), hex(root.as_bytes())).unwrap();
    }
    let pair = [
        payload(ObjectRole::WholeFile, &whole, None),
        payload(ObjectRole::WholeFile, &w, None),
    ];
    let same = support::create_store(&out.join("same-save.sqlite"));
    let result = disabled(|scope| {
        let mut op = same.begin_save(scope.child("save"))?;
        for o in pair {
            op.accept(o)?;
        }
        op.finish(scope.child("finish"))
    })
    .unwrap();
    assert_eq!(
        (
            result.full_records,
            result.prefix_records,
            result.delta.trials
        ),
        (1, 1, 1)
    );
    std::fs::write(out.join("same-save.txt"), format!("{result:?}")).unwrap();
}
