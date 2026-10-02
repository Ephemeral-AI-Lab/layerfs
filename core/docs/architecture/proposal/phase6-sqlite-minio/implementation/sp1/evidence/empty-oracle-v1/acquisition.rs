//! Independent empty deterministic namespace from archived unchanged C1/C2.
mod support;
use layerfs_content::filesystem::attributes::{
    build::build_attribute_tree, codec::AttributeEntry, keys::AttributeKey,
    portable::PortableMetadata, value::emit_value,
};
use layerfs_content::filesystem::{
    build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate,
};
use layerfs_content::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{FinalizedConsumer, ObjectId};
use std::{fmt::Write, path::PathBuf};
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    })
}
#[test]
#[ignore = "only archived unchanged producer; fresh SP1_EMPTY_SEAL_OUT"]
fn seal_empty_namespace() {
    let out = PathBuf::from(std::env::var("SP1_EMPTY_SEAL_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let mut objects = support::Collected::new();
    let metadata = PortableMetadata {
        mode: 0o750,
        mtime_seconds: 1700000000,
        mtime_nanoseconds: 0,
    };
    let provider = support::Collected::new();
    let provider = support::Provider(&provider);
    let mut value_sink = support::Collected::new();
    let mut scoped = FilesystemObjects::new(&provider, &mut value_sink);
    let mode = emit_value(
        &mut scoped,
        &metadata.mode_bytes(InodeKind::Directory).unwrap(),
    )
    .unwrap();
    let mtime = emit_value(&mut scoped, &metadata.mtime_bytes().unwrap()).unwrap();
    let attribute = build_attribute_tree(
        &mut scoped,
        vec![
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
                value_root: mtime,
            }),
        ]
        .into_iter(),
    )
    .unwrap()
    .0;
    for object in value_sink.finalized() {
        objects.accept(object).unwrap();
    }
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![],
    }];
    let inodes = [InodeUpdate {
        serial: 1,
        value: InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: ObjectId::for_bytes(b""),
            metadata_root: attribute,
        },
    }];
    let new_inodes = [1];
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x53; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let provider = objects.clone();
    let provider = support::Provider(&provider);
    let mut sink = support::Collected::new();
    let root = {
        let mut scoped = FilesystemObjects::new(&provider, &mut sink);
        build_filesystem(&mut scoped, &input, None).unwrap().root.0
    };
    for object in sink.finalized() {
        objects.accept(object).unwrap();
    }
    let mut inventory = String::from("ordinal\tid\trole\tcanonical_length\trefs\n");
    for (ordinal, (id, role, bytes, refs)) in objects.objects().iter().enumerate() {
        std::fs::write(out.join(format!("{}.canonical", hex(id.as_bytes()))), bytes).unwrap();
        writeln!(
            inventory,
            "{ordinal}\t{}\t{}\t{}\t{}",
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
    std::fs::write(out.join("objects.tsv"), inventory).unwrap();
    std::fs::write(out.join("root.txt"), hex(root.as_bytes())).unwrap();
    let store = support::create_store(&out.join("metadata.sqlite"));
    let result = support::save_via_handoff(&store, &objects).unwrap();
    std::fs::write(out.join("outcome.txt"), format!("{result:?}\n")).unwrap();
}
