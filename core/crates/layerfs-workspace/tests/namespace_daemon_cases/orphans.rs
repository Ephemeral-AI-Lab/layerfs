use super::*;
use layerfs_content::filesystem::{
    attributes::{
        build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
    },
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate,
};
use layerfs_overlay::{FileRead, OpenFile};
use layerfs_workspace::Position;
fn read_window(service: &Service, open: OpenFile) -> Vec<u8> {
    service.window(|view| {
        let read = service.job(
            Command::AcquireFileRead {
                source: view.source(),
                file: open,
                request: service.owners.fetch_add(1, Ordering::Relaxed) + 10000,
            },
            |r| match r {
                Response::FileReader(Some(read)) => *read,
                other => panic!("{other:?}"),
            },
        );
        let mut out = Vec::new();
        view.read_file(&service.client, read, 0, 131072, &mut out)
            .unwrap();
        service.job(Command::ReleaseFileRead(read), |_| ());
        out
    })
}
#[test]
fn inherited_open_unlinked_log_keeps_bytes_through_owner_installs_failures_and_idle_cleanup() {
    let service = Service::new("orphan-log");
    service.job(Command::LifetimePlans, |r| match r {
        Response::LifetimePlans(plans) => {
            assert!(plans
                .iter()
                .all(|p| p.contains("SEARCH") && !p.contains("TEMP")));
            println!("S6_CUSTODY_PLANS {plans:?}");
        }
        other => panic!("{other:?}"),
    });
    let open = service.window(|view| {
        let base = service.job(
            Command::SourceInode {
                source: view.source(),
                serial: 2,
            },
            |r| match r {
                Response::Inode(inode) => inode.clone(),
                other => panic!("{other:?}"),
            },
        );
        assert!(base.is_none());
        // The owning canonical metadata fact is fetched outside SQL.
        let stat = view.stat(&service.client, 2).unwrap();
        let inode = layerfs_overlay::Inode {
            serial: 2,
            kind: layerfs_overlay::InodeKind::File,
            mode: u16::try_from(stat.metadata.mode).unwrap(),
            mtime_seconds: stat.metadata.mtime_seconds,
            mtime_nanoseconds: stat.metadata.mtime_nanoseconds,
            nlink: stat.namespace_refs,
            size: stat.logical_len,
            inherited_cutoff: stat.logical_len,
            born: 0,
            entries: 0,
        };
        service.job(
            Command::OpenFile {
                source: view.source(),
                request: 1,
                base: inode,
                writable: true,
            },
            |r| match r {
                Response::File(Some(file)) => *file,
                other => panic!("{other:?}"),
            },
        )
    });
    service.applied(unlink(1, "alias"));
    service.applied(unlink(1, "file"));
    assert_eq!(service.lookup(1, "file"), None);
    assert_eq!(
        refusal(service.run(
            Operation::Write {
                serial: 2,
                position: Position::End,
                data: b"unauthorized".as_slice().into()
            },
            T1
        )),
        Some(Refusal::Missing)
    );
    let base = service.workspace.base().unwrap();
    let store = &service.gate.store;
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let portable = PortableMetadata {
        mode: 0o1777,
        mtime_seconds: T1.seconds,
        mtime_nanoseconds: T1.nanoseconds,
    };
    let kind = layerfs_content::object::inode_leaf::InodeKind::Directory;
    let mode = emit_value(&mut objects, &portable.mode_bytes(kind).unwrap()).unwrap();
    let time = emit_value(&mut objects, &portable.mtime_bytes().unwrap()).unwrap();
    let metadata = build_attribute_tree(
        &mut objects,
        [
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_root: time,
            }),
        ]
        .into_iter(),
    )
    .unwrap()
    .0;
    let mut value = base.inode(1).unwrap().value;
    value.metadata_root = metadata;
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("alias"), None), (name("file"), None)],
    }];
    let inodes = [InodeUpdate { serial: 1, value }];
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(base.identity()),
            scope: base.root().scope(),
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let mut expect = b"original\0\xff".to_vec();
    for round in 0..24 {
        let record = format!("record-{round:02}\n").into_bytes();
        service.applied(Operation::WriteOpen {
            file: open,
            position: Position::End,
            data: record.clone().into(),
        });
        expect.extend(record);
        if round == 5 {
            service.applied(Operation::SetOpenAttributes {
                file: open,
                mode: None,
                mtime: None,
                size: Some(7),
            });
            expect.truncate(7);
            service.applied(Operation::SetOpenAttributes {
                file: open,
                mode: None,
                mtime: None,
                size: Some(9000),
            });
            expect.resize(9000, 0);
        }
        assert_eq!(read_window(&service, open), expect);
        let capture = service.job(Command::Capture, |r| match r {
            Response::Captured(c) => *c,
            other => panic!("{other:?}"),
        });
        if round % 2 == 0 {
            let prepared = service
                .workspace
                .prepare_base_install(capture, next)
                .unwrap();
            service.job(
                Command::InstallPrepared {
                    workspace: service.workspace.clone(),
                    input: prepared,
                },
                |_| (),
            );
        } else {
            service.job(Command::ResolveFailed(capture), |_| ());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let ready = service.job(Command::State, |r| match r {
                Response::State(s) => s.consolidating.is_none(),
                other => panic!("{other:?}"),
            });
            if ready {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
    }
    let read: FileRead = service.window(|view| {
        service.job(
            Command::AcquireFileRead {
                source: view.source(),
                file: open,
                request: 99999,
            },
            |r| match r {
                Response::FileReader(Some(read)) => *read,
                other => panic!("{other:?}"),
            },
        )
    });
    service.job(Command::CloseFile(open), |_| ());
    let source = read.source();
    let view = service.workspace.view_for_source(source).unwrap();
    let mut got = Vec::new();
    view.read_file(&service.client, read, 0, 131072, &mut got)
        .unwrap();
    assert_eq!(got, expect);
    service.job(Command::ReleaseFileRead(read), |_| ());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if service.job(Command::MaintenanceIdle, |r| {
            matches!(r, Response::MaintenanceIdle(true))
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    println!("S6_ORPHAN_OWNER captures=24 success=12 failed=12 inherited_bytes=10 orphan_bytes={} truncate_regrow=true last_read_outlives_close=true idle_cleanup=true",expect.len());
}
