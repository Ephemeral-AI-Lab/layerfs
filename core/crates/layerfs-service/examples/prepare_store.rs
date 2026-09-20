//! Host-only fixture preparation through public C1/C2 APIs.
use layerfs_content::{
    filesystem::{
        attributes::{
            build::build_attribute_tree, codec::AttributeEntry, keys::AttributeKey,
            portable::PortableMetadata, value::emit_value,
        },
        symlink::{emit_symlink, SymlinkTarget},
    },
    object::inode_leaf::{InodeKind, InodeValue},
    *,
};
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::Timing;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Store path required")?;
    let store = Timing::disabled("create", |s| {
        Store::create(path, Store::default_policy(), s.child("create"))
    })
    .0?;
    let (root, scope, file, metadata) =
        Timing::disabled("prepare", |s| -> Result<_, Box<dyn std::error::Error>> {
            let policy = store.policy().construction();
            let mut save = store.begin_save(s.child("save"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let file = construct_bytes(
                policy,
                &policy.capacities(),
                b"original",
                &mut handoff,
                s.child("file"),
            )?
            .root;
            drop(handoff);
            save.finish(s.child("finish"))?;
            let provider = StoreProvider::new(&store);
            let mut save = store.begin_save(s.child("meta"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let mut objects = FilesystemObjects::new(&provider, &mut handoff);
            let meta = PortableMetadata {
                mode: 0o777,
                mtime_seconds: 1700000000,
                mtime_nanoseconds: 7,
            };
            let mode = emit_value(&mut objects, &meta.mode_bytes(InodeKind::RegularFile)?)?;
            let mtime = emit_value(&mut objects, &meta.mtime_bytes()?)?;
            let metadata = build_attribute_tree(
                &mut objects,
                vec![
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".into(), b"mode".to_vec())?,
                        value_root: mode,
                    }),
                    Ok(AttributeEntry {
                        key: AttributeKey::new("portable".into(), b"mtime".to_vec())?,
                        value_root: mtime,
                    }),
                ]
                .into_iter(),
            )?
            .0;
            let link = emit_symlink(&mut objects, SymlinkTarget::new(b"f".to_vec())?)?;
            drop(handoff);
            save.finish(s.child("finish"))?;
            let scope = filesystem::scope_for_seed([42; 32]);
            let directories = [DirectoryUpdate {
                parent: 1,
                changes: vec![
                    (PathName::new("f")?, Some(2)),
                    (PathName::new("g")?, Some(4)),
                    (PathName::new("link")?, Some(3)),
                ],
            }];
            let inodes = [
                InodeUpdate {
                    serial: 1,
                    value: InodeValue {
                        kind: InodeKind::Directory,
                        namespace_ref_count: 0,
                        content_root: file,
                        metadata_root: metadata,
                    },
                },
                InodeUpdate {
                    serial: 2,
                    value: InodeValue {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 0,
                        content_root: file,
                        metadata_root: metadata,
                    },
                },
                InodeUpdate {
                    serial: 3,
                    value: InodeValue {
                        kind: InodeKind::Symlink,
                        namespace_ref_count: 0,
                        content_root: link,
                        metadata_root: metadata,
                    },
                },
                InodeUpdate {
                    serial: 4,
                    value: InodeValue {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 0,
                        content_root: file,
                        metadata_root: metadata,
                    },
                },
            ];
            let input = FilesystemInput {
                base: None,
                scope,
                root_serial: 1,
                directories: &directories,
                inodes: &inodes,
                new_inodes: &[1, 2, 3, 4],
                resources: FilesystemResources::default(),
            };
            let mut save = store.begin_save(s.child("tree"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let mut objects = FilesystemObjects::new(&provider, &mut handoff);
            let root = build_filesystem(&mut objects, &input, None)?.root.0;
            drop(handoff);
            save.finish(s.child("finish"))?;
            Ok((root, scope.object(), file, metadata))
        })
        .0?;
    println!("{{\"root\":\"{root}\",\"scope\":\"{scope}\",\"file\":\"{file}\",\"metadata\":\"{metadata}\"}}");
    Ok(())
}
