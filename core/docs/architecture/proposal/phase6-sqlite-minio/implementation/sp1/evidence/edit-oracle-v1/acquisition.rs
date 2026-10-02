//! Independent original-parent edit-operation acquisition; complete-stream oracle remains separate.
#[path = "../../layerfs-content/tests/support/edits.rs"]
#[allow(dead_code)]
mod edits;
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
use layerfs_content::{ConstructionPolicy, Edit, EditRequest, FinalizedConsumer, ObjectId};
use std::{collections::BTreeMap, fmt::Write, path::Path};
use support::{disabled, Collected};
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

fn edit(
    all: &Collected,
    root: ObjectId,
    base: u64,
    change: Edit,
    replacement: Vec<u8>,
) -> (Collected, ObjectId) {
    let sequence = edits::Edits::new(base, vec![change]).unwrap();
    let mut parts = edits::Parts::new();
    parts.push(replacement);
    let reader = support::Provider(all);
    let mut sink = Collected::new();
    let policy = ConstructionPolicy::frozen_default();
    let built = disabled(|scope| {
        layerfs_content::apply_edits(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root,
                edits: &sequence,
                source: &parts,
            },
            &mut sink,
            scope.child("edit"),
        )
    })
    .unwrap();
    (sink, built.root)
}
#[test]
#[ignore = "archived unchanged original producer only; SP1_EDIT_SEAL_IN sealed full-stream inputs; fresh SP1_EDIT_SEAL_OUT"]
fn seal_original_edit_filesystem_states() {
    let input = std::path::PathBuf::from(std::env::var("SP1_EDIT_SEAL_IN").unwrap());
    let out = std::path::PathBuf::from(std::env::var("SP1_EDIT_SEAL_OUT").unwrap());
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let mut all = Collected::new();
    let dir_attrs = attributes(&mut all, InodeKind::Directory, None);
    let file_attrs = attributes(&mut all, InodeKind::RegularFile, None);
    let mut current = BTreeMap::new();
    let mut roots = String::from("state\tpath\tlength\tcontent_root\n");
    let mut fsroots = String::from("state\tfilesystem_root\n");
    for state in 0..3 {
        for name in ["boundary", "chunked", "whole", "duplicate"] {
            let raw_path = input.join(format!("state{state}-{name}.raw"));
            if !raw_path.exists() {
                continue;
            }
            let raw = std::fs::read(raw_path).unwrap();
            let previous = all.clone();
            let (emitted, root) = if state == 0 || name == "duplicate" {
                construct(&raw)
            } else if state == 1 && name == "boundary" {
                edit(
                    &previous,
                    current[name],
                    131071,
                    Edit::insert(131071, 1),
                    vec![0x7a],
                )
            } else if state == 1 {
                edit(
                    &previous,
                    current[name],
                    raw.len() as u64,
                    Edit::overwrite(64, 96),
                    raw[64..96].to_vec(),
                )
            } else if name == "boundary" {
                edit(
                    &previous,
                    current[name],
                    131072,
                    Edit::delete(131071, 131072),
                    vec![],
                )
            } else {
                (Collected::new(), current[name])
            };
            export(&out, &format!("state{state}-{name}"), &emitted);
            append(&mut all, &emitted);
            current.insert(name, root);
            let reader = support::Provider(&all);
            let mut actual = Vec::new();
            disabled(|scope| {
                layerfs_content::read_all(&reader, root, &mut actual, scope.child("bytes"))
            })
            .unwrap();
            assert_eq!(actual, raw);
            std::fs::write(out.join(format!("state{state}-{name}.raw")), &raw).unwrap();
            writeln!(
                roots,
                "{state}\t{name}\t{}\t{}",
                raw.len(),
                hex(root.as_bytes())
            )
            .unwrap();
        }
        let mut inodes = vec![InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b""),
                metadata_root: dir_attrs,
            },
        }];
        let mut changes = Vec::new();
        for (name, content_root) in &current {
            let serial = match *name {
                "whole" => 2,
                "chunked" => 3,
                "boundary" => 4,
                _ => 5,
            };
            changes.push((PathName::new(name).unwrap(), Some(serial)));
            inodes.push(InodeUpdate {
                serial,
                value: InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 0,
                    content_root: *content_root,
                    metadata_root: file_attrs,
                },
            });
        }
        inodes.sort_by_key(|row| row.serial);
        let new_inodes = inodes.iter().map(|row| row.serial).collect::<Vec<_>>();
        let directories = [DirectoryUpdate { parent: 1, changes }];
        let spec = FilesystemInput {
            base: None,
            scope: layerfs_content::filesystem::scope_for_seed([0x53; 32]),
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: FilesystemResources::default(),
        };
        let previous = all.clone();
        let reader = support::Provider(&previous);
        let mut sink = Collected::new();
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        let root = build_filesystem(&mut objects, &spec, None).unwrap().root.0;
        export(&out, &format!("state{state}-filesystem"), &sink);
        append(&mut all, &sink);
        writeln!(fsroots, "{state}\t{}", hex(root.as_bytes())).unwrap();
    }
    export(&out, "all-emissions", &all);
    std::fs::write(out.join("content-roots.tsv"), roots).unwrap();
    std::fs::write(out.join("filesystem-roots.tsv"), fsroots).unwrap();
}
