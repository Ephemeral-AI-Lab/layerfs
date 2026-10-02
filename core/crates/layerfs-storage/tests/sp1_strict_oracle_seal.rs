//! Opt-in acquisition from archived unchanged C1/C2, never a candidate oracle.
mod support;
use layerfs_content::filesystem::attributes::{
    build::build_attribute_tree, codec::AttributeEntry, keys::AttributeKey,
    portable::PortableMetadata, value::emit_value,
};
use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate, PathName,
};
use layerfs_content::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AdvisoryPredecessors, ConstructionPolicy, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{StoragePolicy, Store};
use std::{fmt::Write, path::Path};
use support::{disabled, noise, save_one, Collected};
fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut out, v| {
        write!(out, "{v:02x}").unwrap();
        out
    })
}
fn construct(raw: &[u8]) -> (Collected, ObjectId) {
    let mut collected = Collected::new();
    let policy = ConstructionPolicy::frozen_default();
    let file = disabled(|scope| {
        layerfs_content::construct_stream(
            policy,
            &policy.capacities(),
            std::io::Cursor::new(raw),
            &mut collected,
            scope.child("construct"),
        )
    })
    .unwrap();
    (collected, file.root)
}
fn export(out: &Path, label: &str, objects: &Collected) {
    let mut manifest = String::from("ordinal\tid\trole\tcanonical_length\trefs\tpredecessors\n");
    for (ordinal, (id, role, bytes, refs)) in objects.objects().iter().enumerate() {
        let file = out.join(format!("{}.canonical", hex(id.as_bytes())));
        if file.exists() {
            assert_eq!(std::fs::read(&file).unwrap(), *bytes);
        } else {
            std::fs::write(file, bytes).unwrap();
        }
        let predecessors = objects.advisories()[ordinal]
            .ids()
            .map(|id| hex(id.as_bytes()))
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            manifest,
            "{ordinal}\t{}\t{}\t{}\t{}\t{predecessors}",
            hex(id.as_bytes()),
            role.code(),
            bytes.len(),
            refs.iter()
                .map(|id| hex(id.as_bytes()))
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
    }
    std::fs::write(out.join(format!("{label}.tsv")), manifest).unwrap();
}
fn append(target: &mut Collected, source: &Collected) {
    for object in source.finalized() {
        target.accept(object).unwrap();
    }
}
fn attributes(all: &mut Collected, kind: InodeKind, opaque: Option<&[u8]>) -> ObjectId {
    let provider = all.clone();
    let mut sink = Collected::new();
    let reader = support::Provider(&provider);
    let mut objects = FilesystemObjects::new(&reader, &mut sink);
    let metadata = PortableMetadata {
        mode: if kind == InodeKind::Directory {
            0o750
        } else {
            0o640
        },
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 0,
    };
    let mut entries = Vec::new();
    for (domain, key, raw) in [
        (
            "portable",
            b"mode".as_slice(),
            metadata.mode_bytes(kind).unwrap().to_vec(),
        ),
        (
            "portable",
            b"mtime".as_slice(),
            metadata.mtime_bytes().unwrap().to_vec(),
        ),
    ] {
        entries.push(Ok(AttributeEntry {
            key: AttributeKey::new(domain.to_owned(), key.to_vec()).unwrap(),
            value_root: emit_value(&mut objects, &raw).unwrap(),
        }));
    }
    if let Some(raw) = opaque {
        entries.push(Ok(AttributeEntry {
            key: AttributeKey::new("sp1".to_owned(), b"opaque".to_vec()).unwrap(),
            value_root: emit_value(&mut objects, raw).unwrap(),
        }));
    }
    entries.sort_by(|a, b| a.as_ref().unwrap().key.cmp(&b.as_ref().unwrap().key));
    let root = build_attribute_tree(&mut objects, entries.into_iter())
        .unwrap()
        .0;
    append(all, &sink);
    root
}
fn predecessor(object: FinalizedObject, previous: Option<ObjectId>) -> FinalizedObject {
    let mut hints = AdvisoryPredecessors::new();
    if let Some(id) = previous {
        hints.push(id, PredecessorProvenance::OriginalBase).unwrap();
    }
    object.with_predecessors(hints)
}
#[test]
#[ignore = "only archived unchanged producer; SP1_STRICT_SEAL_OUT fresh directory"]
fn seal_strict_v2_missing_vectors() {
    let out = std::path::PathBuf::from(std::env::var("SP1_STRICT_SEAL_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let raw = noise(200000);
    let (file, root) = construct(&raw);
    export(&out, "regular-N200000", &file);
    let first = file
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .unwrap();
    let chunk = layerfs_content::file::mapping::decode_chunk_payload(
        layerfs_content::object::decode_bytes_object(&first.2).unwrap(),
    )
    .unwrap();
    assert_eq!(chunk, &raw[..chunk.len()]);
    assert!(chunk.len() <= 32768);
    std::fs::write(out.join("first-cdc.raw"), chunk).unwrap();
    let mut attribute = Collected::new();
    let attrs = attributes(&mut attribute, InodeKind::RegularFile, Some(chunk));
    export(&out, "attribute-dual-use", &attribute);
    let dual = attribute
        .objects()
        .iter()
        .find(|(id, role, _, _)| *id == first.0 && *role == ObjectRole::Chunk)
        .unwrap();
    assert_eq!(dual.2, first.2);
    std::fs::write(out.join("provenance.tsv"),format!("first_chunk_id\t{}\nraw_start\t0\nraw_end\t{}\ncanonical_length\t{}\nregular_root\t{}\nattribute_root\t{}\nregular_use\tFilePayload / RegularFileGraph\nattribute_use\tMetadata / MetadataGraph\n",hex(first.0.as_bytes()),chunk.len(),first.2.len(),hex(root.as_bytes()),hex(attrs.as_bytes()))).unwrap();
    // Same mapping body emitted by an ordinary chunked regular file and attributes.
    let mut attribute_chunks = Collected::new();
    let provider = Collected::new();
    let value_root = {
        let provider = support::Provider(&provider);
        let mut objects = FilesystemObjects::new(&provider, &mut attribute_chunks);
        emit_value(&mut objects, chunk).unwrap()
    };
    export(&out, "opaque-value", &attribute_chunks);
    std::fs::write(out.join("opaque-root.txt"), hex(value_root.as_bytes())).unwrap();
    let mut all = Collected::new();
    let mut roots = String::from("state\tpath\tlength\tcontent_root\n");
    let scope = layerfs_content::filesystem::scope_for_seed([0x53; 32]);
    let dir_attrs = attributes(&mut all, InodeKind::Directory, None);
    let file_attrs = attributes(&mut all, InodeKind::RegularFile, None);
    let mut fsroots = String::from("state\tfilesystem_root\n");
    for state in 0..3 {
        let mut whole = noise(96000);
        let mut chunked = noise(200000);
        let mut boundary = noise(131071);
        if state > 0 {
            for b in &mut whole[64..96] {
                *b ^= 255;
            }
            for b in &mut chunked[64..96] {
                *b ^= 255;
            }
        }
        if state == 1 {
            boundary.push(0x7a);
        }
        let mut files = vec![
            ("boundary", boundary),
            ("chunked", chunked),
            ("whole", whole),
        ];
        if state == 2 {
            files.push(("duplicate", noise(96000)));
        }
        files.sort_by_key(|(name, _)| *name);
        let mut directory_changes = Vec::new();
        let mut inodes = vec![InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b""),
                metadata_root: dir_attrs,
            },
        }];
        for (name, raw) in &files {
            let serial = match *name {
                "whole" => 2,
                "chunked" => 3,
                "boundary" => 4,
                _ => 5,
            };
            let (objects, content_root) = construct(raw);
            export(&out, &format!("state{state}-{name}"), &objects);
            append(&mut all, &objects);
            std::fs::write(out.join(format!("state{state}-{name}.raw")), raw).unwrap();
            writeln!(
                roots,
                "{state}\t{name}\t{}\t{}",
                raw.len(),
                hex(content_root.as_bytes())
            )
            .unwrap();
            directory_changes.push((PathName::new(name).unwrap(), Some(serial)));
            inodes.push(InodeUpdate {
                serial,
                value: InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 0,
                    content_root,
                    metadata_root: file_attrs,
                },
            });
        }
        inodes.sort_by_key(|inode| inode.serial);
        let new_inodes = inodes.iter().map(|inode| inode.serial).collect::<Vec<_>>();
        let directories = [DirectoryUpdate {
            parent: 1,
            changes: directory_changes,
        }];
        let input = FilesystemInput {
            base: None,
            scope,
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: FilesystemResources::default(),
        };
        let provider = all.clone();
        let mut sink = Collected::new();
        let root = {
            let provider = support::Provider(&provider);
            let mut objects = FilesystemObjects::new(&provider, &mut sink);
            build_filesystem(&mut objects, &input, None).unwrap().root.0
        };
        export(&out, &format!("state{state}-filesystem"), &sink);
        append(&mut all, &sink);
        writeln!(fsroots, "{state}\t{}", hex(root.as_bytes())).unwrap();
    }
    export(&out, "filesystem-all", &all);
    std::fs::write(out.join("content-roots.tsv"), roots).unwrap();
    std::fs::write(out.join("filesystem-roots.tsv"), fsroots).unwrap();
    let store = support::create_store(&out.join("filesystem-producer.sqlite"));
    let result = support::save_via_handoff(&store, &all).unwrap();
    std::fs::write(out.join("filesystem-save.txt"), format!("{result:?}\n")).unwrap();
    drop(store);
    // Frozen existing work-budget fixture, without seed or threshold search.
    let path = out.join("work-boundary.sqlite");
    let policy = StoragePolicy::new(1, 131072, 16, 4);
    let store = disabled(|scope| Store::create(&path, policy, scope.child("create"))).unwrap();
    let mut raw = noise(120000);
    let mut previous = None;
    let mut counters = String::new();
    for step in 0..12 {
        let object = predecessor(
            FinalizedObject::new(
                ObjectRole::WholeFile,
                layerfs_content::encode_whole_file_payload(&raw).unwrap(),
            )
            .unwrap(),
            previous,
        );
        previous = Some(object.id());
        let result = save_one(&store, object).unwrap();
        writeln!(
            counters,
            "{step}\t{}\t{result:?}",
            hex(previous.unwrap().as_bytes())
        )
        .unwrap();
        raw[step * 64..step * 64 + 32].fill(0xa5);
    }
    drop(store);
    std::fs::write(out.join("work-boundary.txt"), counters).unwrap();
    let path = out.join("chunk-depth2.sqlite");
    let policy = StoragePolicy::new(1, 131072, 8, 2);
    let store = disabled(|scope| Store::create(&path, policy, scope.child("create"))).unwrap();
    let mut raw = noise(20000);
    let mut previous = None;
    let mut counters = String::new();
    for step in 0..4 {
        let object = predecessor(
            FinalizedObject::new(
                ObjectRole::Chunk,
                layerfs_content::file::mapping::encode_chunk_object(&raw).unwrap(),
            )
            .unwrap(),
            previous,
        );
        previous = Some(object.id());
        let result = save_one(&store, object).unwrap();
        writeln!(
            counters,
            "{step}\t{}\t{result:?}",
            hex(previous.unwrap().as_bytes())
        )
        .unwrap();
        if step == 2 {
            for byte in &mut raw {
                *byte ^= 0xa5;
            }
        } else {
            for byte in &mut raw[64 + step * 64..96 + step * 64] {
                *byte ^= 255;
            }
        }
    }
    drop(store);
    std::fs::write(out.join("chunk-depth2.txt"), counters).unwrap();
}

#[test]
#[ignore = "only archived unchanged producer; fresh SP1_STRICT_EXTRA_OUT"]
fn seal_strict_v2_same_save_vectors() {
    use layerfs_content::inode_leaf::{
        encode_inode_value, InodeLeaf, InodeLeafRow, LEAF_ROW_BYTES,
    };
    let out = std::path::PathBuf::from(std::env::var("SP1_STRICT_EXTRA_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let store = support::create_store(&out.join("writer-same-save.sqlite"));
    let whole = noise(96000);
    let mut edited = whole.clone();
    for b in &mut edited[64..96] {
        *b ^= 255;
    }
    let chunk = noise(20000);
    let mut edited_chunk = chunk.clone();
    for b in &mut edited_chunk[64..96] {
        *b ^= 255;
    }
    let unrelated = chunk.iter().map(|b| b ^ 0xa5).collect::<Vec<_>>();
    let mut objects = Collected::new();
    for (role, raw) in [
        (ObjectRole::WholeFile, &whole),
        (ObjectRole::WholeFile, &edited),
        (ObjectRole::Chunk, &chunk),
        (ObjectRole::Chunk, &edited_chunk),
        (ObjectRole::WholeFile, &whole),
        (ObjectRole::Chunk, &unrelated),
    ] {
        let canonical = if role == ObjectRole::WholeFile {
            layerfs_content::encode_whole_file_payload(raw).unwrap()
        } else {
            layerfs_content::file::mapping::encode_chunk_object(raw).unwrap()
        };
        objects
            .accept(FinalizedObject::new(role, canonical).unwrap())
            .unwrap();
    }
    export(&out, "writer-emission", &objects);
    let outcome = support::save_via_handoff(&store, &objects).unwrap();
    std::fs::write(out.join("writer-outcome.txt"), format!("{outcome:?}\n")).unwrap();
    drop(store);
    let value = |seed: u64| {
        let mut b = [0; 32];
        b[..8].copy_from_slice(&seed.to_be_bytes());
        encode_inode_value(InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: ObjectId::for_bytes(&b),
            metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
        })
    };
    let leaf = |first: u64, values: &Vec<_>| {
        let rows = values
            .iter()
            .enumerate()
            .map(|(i, value)| InodeLeafRow {
                serial: first + i as u64,
                value: *value,
            })
            .collect::<Vec<_>>();
        FinalizedObject::new(
            ObjectRole::InodeLeaf,
            InodeLeaf {
                subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
                rows,
            }
            .encode()
            .unwrap(),
        )
        .unwrap()
    };
    let mut values = (0..8).map(value).collect::<Vec<_>>();
    let first = leaf(1, &values);
    let first_id = first.id();
    values.extend((20..22).map(value));
    let second = predecessor(leaf(1000, &values), Some(first_id));
    let mut objects = Collected::new();
    objects.accept(first).unwrap();
    objects.accept(second).unwrap();
    export(&out, "pool-same-save", &objects);
    let store = support::create_store(&out.join("pool-same-save.sqlite"));
    let outcome = support::save_via_handoff(&store, &objects).unwrap();
    assert_eq!(
        (
            outcome.pool.leaves,
            outcome.pool.reused_values,
            outcome.pool.new_values
        ),
        (2, 8, 10)
    );
    std::fs::write(out.join("pool-outcome.txt"), format!("{outcome:?}\n")).unwrap();
}

#[test]
#[ignore = "only archived unchanged C2; canonical inputs from independent sealed acquisition"]
fn seal_strict_v2_physical_domains() {
    use std::collections::BTreeSet;
    let source = std::path::PathBuf::from(std::env::var("SP1_STRICT_CANONICAL_SOURCE").unwrap());
    let out = std::path::PathBuf::from(std::env::var("SP1_STRICT_DOMAINS_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let mut payload_ids = BTreeSet::new();
    for state in 0..3 {
        for name in ["whole", "chunked", "boundary", "duplicate"] {
            let path = source.join(format!("state{state}-{name}.tsv"));
            if !path.exists() {
                continue;
            }
            for row in std::fs::read_to_string(path).unwrap().lines().skip(1) {
                let cells = row.split('\t').collect::<Vec<_>>();
                let role = ObjectRole::from_code(cells[2].parse().unwrap()).unwrap();
                if matches!(role, ObjectRole::Chunk | ObjectRole::WholeFile) {
                    payload_ids.insert(cells[1].to_owned());
                }
            }
        }
    }
    let all = std::fs::read_to_string(source.join("filesystem-all.tsv")).unwrap();
    for domain in ["metadata", "file-payload"] {
        let store = support::create_store(&out.join(format!("{domain}.sqlite")));
        let mut collected = Collected::new();
        let mut seen = BTreeSet::new();
        for row in all.lines().skip(1) {
            let cells = row.split('\t').collect::<Vec<_>>();
            let is_payload = payload_ids.contains(cells[1]);
            if (domain == "file-payload") != is_payload || !seen.insert(cells[1].to_owned()) {
                continue;
            }
            let bytes = std::fs::read(source.join(format!("{}.canonical", cells[1]))).unwrap();
            let role = ObjectRole::from_code(cells[2].parse().unwrap()).unwrap();
            collected
                .accept(FinalizedObject::new(role, bytes).unwrap())
                .unwrap();
        }
        export(&out, domain, &collected);
        let outcome = support::save_via_handoff(&store, &collected).unwrap();
        std::fs::write(
            out.join(format!("{domain}-outcome.txt")),
            format!("{outcome:?}\n"),
        )
        .unwrap();
    }
}
