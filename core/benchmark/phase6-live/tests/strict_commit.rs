//! Engine capture/native storage/C5 edit-operation-v1 component.
//! Expected roots come from separately sealed original-parent apply_edits, while
//! full logical bytes remain the original complete-stream fixture raw vectors.
//! No SDK/Exec/FUSE/S2 or complete-stream/edit-root equivalence qualification.
mod strict_witness;
use layerfs_bridge::{
    adapters::native::{
        connection::{self, Peer, VerifiedPeer},
        protocol::{Frame, Kind},
    },
    contract::{CommitOutcomeWire, WorkspaceCommitOutcome},
};
use layerfs_content::{
    filesystem::{FilesystemRead, FilesystemRoot, FilesystemRootId, LogicalPath},
    AuthenticatedObjects, ObjectId,
};
use layerfs_history::{
    catalog::HistoryCatalog,
    catalog::HistoryCatalogConfig,
    identity::{BranchId, CommitId, HistoryName, LayerStackId},
    records::{ForkRequest, ForkSource, ReserveRequest, StackInitialization},
    sqlite,
};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use phase6_live_probe::{
    engine::Engine,
    reservations::Owner,
    strict_catalog::{LogicalUse, PlacementDomain, SaveContext, StrictCatalog},
    strict_commit,
    strict_publication::{self, Authority, Request},
    strict_read::WaveReader,
    strict_remote::{NativeCatalog, Session},
    strict_wire,
};
use std::{
    collections::BTreeMap,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use strict_witness::{disposable, empty_fixture, evidence, id, provider};
fn capacities() -> StorageCapacities {
    StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap()
}
fn native(catalog: Arc<StrictCatalog>) -> (Arc<NativeCatalog>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let peer = Peer {
            selector: 1,
            public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
            expires_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
        };
        let mut channel = connection::accept(stream, &[8; 32], &[peer]).unwrap();
        while let Ok(frame) = channel.receive.read() {
            assert_eq!(frame.kind, Kind::Begin);
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            channel
                .send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    (
        Arc::new(NativeCatalog {
            endpoint,
            selector: 1,
            private: [7; 32],
            server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
            session: Arc::new(Mutex::new(Session::default())),
        }),
        server,
    )
}
fn write_bytes(engine: &mut Engine, node: i64, bytes: &[u8]) {
    // Preserve the complete declared file; each ordinary Engine request honors
    // its existing128KiB per-call limit, exactly as a bounded FUSE writer would.
    for (index, chunk) in bytes.chunks(128 * 1024).enumerate() {
        engine
            .write(node, (index * 128 * 1024) as i64, chunk)
            .unwrap();
    }
}
fn fixed_metadata(engine: &Engine) {
    // External deterministic component preconditioning, not an ordinary FUSE
    // timestamp oracle and never a fake clock/product hook or measured setup.
    engine.db.execute_batch("UPDATE inodes SET mode=CASE kind WHEN 2 THEN 488 ELSE 416 END,seconds=1700000000,nanos=0,dirty=1;").unwrap();
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required; Engine fixture not public SDK/FUSE"]
fn real_provider_native_capture_and_three_retained_c5_states() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("strict-commit-catalog");
    let directory = path.parent().unwrap();
    let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
    empty_fixture(&catalog);
    let empty_id = id(
        std::fs::read_to_string(evidence().join("empty-oracle-v1/root.txt"))
            .unwrap()
            .trim(),
    );
    let empty = FilesystemRoot::decode(
        &std::fs::read(evidence().join(format!(
                "empty-oracle-v1/{}.canonical",
                std::fs::read_to_string(evidence().join("empty-oracle-v1/root.txt"))
                    .unwrap()
                    .trim()
            )))
        .unwrap(),
    )
    .unwrap();
    let history = sqlite::create(
        &directory.join("history.sqlite"),
        &HistoryCatalogConfig {
            binding_key: b"strict component history".to_vec(),
            incarnation: 1,
            cursor_key: [7; 32],
        },
    )
    .unwrap();
    let stack = LayerStackId::from_authority([1; 16]);
    let branch = BranchId::from_authority([2; 16]);
    let genesis = history
        .initialize_layerstack(&StackInitialization {
            stack,
            name: HistoryName::new("strict-component-stack").unwrap(),
            scope: empty.scope().object(),
            profile: empty.profile(),
            genesis_root: empty_id,
        })
        .unwrap();
    history
        .fork(&ForkRequest {
            stack,
            branch,
            name: HistoryName::new("strict-component-main").unwrap(),
            source: ForkSource::Layer(genesis.head_layer),
        })
        .unwrap();
    history
        .reserve_inodes(&ReserveRequest {
            scope: empty.scope().object(),
            count: 1,
        })
        .unwrap();
    let authority = Authority {
        catalog: catalog.clone(),
        history,
        branch,
        reservations: Mutex::new(Default::default()),
        publication: Mutex::new(Default::default()),
    };
    let base = authority.snapshot().unwrap();
    let owner = Owner {
        workspace: b"strict-component-w".to_vec(),
        incarnation: [5; 32],
        project: base.stack,
        branch: base.branch,
    };
    authority
        .reservations
        .lock()
        .unwrap()
        .bootstrap(&authority.history, owner.clone(), &base)
        .unwrap();
    let (native, server) = native(catalog.clone());
    let engine_path = directory.join("live-engine");
    let mut engine = Engine::create(&engine_path, 2).unwrap();
    engine.reader = Some(Arc::new(WaveReader {
        catalog: native.clone(),
        provider: provider.clone(),
        scope: native.capture(None).unwrap(),
        logical_use: LogicalUse::RegularFileGraph,
        capacities: capacities(),
    }));
    let raw_source = evidence().join("strict-oracle-v2/vectors");
    let source = evidence().join("edit-oracle-v1");
    let mut nodes = BTreeMap::new();
    for name in ["whole", "chunked", "boundary"] {
        let node = engine.create_node(1, name.as_bytes(), 1, 0o640).unwrap();
        write_bytes(
            &mut engine,
            node.id,
            &std::fs::read(raw_source.join(format!("state0-{name}.raw"))).unwrap(),
        );
        nodes.insert(name, node.id);
    }
    let expected_fs = std::fs::read_to_string(source.join("filesystem-roots.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| id(line.split('\t').nth(1).unwrap()))
        .collect::<Vec<_>>();
    let mut retained = Vec::new();
    let mut commits = Vec::new();
    assert_eq!(expected_fs.len(), 3);
    for (state, expected_root) in expected_fs.iter().enumerate() {
        if state == 1 {
            for name in ["whole", "chunked"] {
                let bytes = std::fs::read(raw_source.join(format!("state1-{name}.raw"))).unwrap();
                engine.write(nodes[name], 64, &bytes[64..96]).unwrap();
            }
            engine.write(nodes["boundary"], 131071, &[0x7a]).unwrap();
        }
        if state == 2 {
            engine.truncate(nodes["boundary"], 131071).unwrap();
            let duplicate = engine.create_node(1, b"duplicate", 1, 0o640).unwrap();
            engine
                .write(
                    duplicate.id,
                    0,
                    &std::fs::read(raw_source.join("state0-whole.raw")).unwrap(),
                )
                .unwrap();
            nodes.insert("duplicate", duplicate.id);
        }
        fixed_metadata(&engine);
        let base = authority.snapshot().unwrap();
        let context = SaveContext {
            owner: owner.clone(),
            generation: state as u64 + 1,
            scope: base.scope,
            profile: base.profile,
            base_root: base.root,
        };
        let prepared = strict_commit::prepare_owned(
            &engine,
            FilesystemRootId(ObjectId::from_bytes(&base.root).unwrap()),
            native.clone(),
            provider.clone(),
            capacities(),
            &context,
        )
        .unwrap();
        assert_eq!(prepared.candidate.root.0, *expected_root);
        match strict_publication::submit(
            &authority,
            Request {
                owner: &owner,
                generation: context.generation,
                revision: engine.revision as u64,
                base: &base,
                save: prepared.save,
                root: prepared.candidate.root.0,
            },
        ) {
            WorkspaceCommitOutcome::Completed(report) => match report.outcome {
                CommitOutcomeWire::Committed(commit) => {
                    assert_eq!(commit.root, *prepared.candidate.root.0.as_bytes());
                    commits.push(CommitId::from_bytes(commit.commit).unwrap());
                }
                other => panic!("expected changed C5 commit: {other:?}"),
            },
            WorkspaceCommitOutcome::Failed(error) => panic!("C5 component failed: {error:?}"),
        };
        engine.install_prepared().unwrap();
        engine.reader = Some(Arc::new(WaveReader {
            catalog: native.clone(),
            provider: provider.clone(),
            scope: native.capture(None).unwrap(),
            logical_use: LogicalUse::RegularFileGraph,
            capacities: capacities(),
        }));
        retained.push(prepared.candidate.root);
        println!("strict capture/native/C5 component state{state}: FSroot{:?},storage{:?},delta{:?},pool{:?}",prepared.candidate.root,prepared.storage,prepared.delta,prepared.pool);
    }
    // Native reader's current publication reopens all retained immutable roots.
    let reader = WaveReader {
        catalog: native.clone(),
        provider: provider.clone(),
        scope: native.capture(None).unwrap(),
        logical_use: LogicalUse::RegularFileGraph,
        capacities: capacities(),
    };
    // Filesystem traversal is metadata; regular file payload traversal has a distinct use.
    let meta = WaveReader {
        logical_use: LogicalUse::MetadataGraph,
        ..reader.clone()
    };
    for (state, root) in retained.iter().enumerate() {
        let mut fs = FilesystemRead::new(&meta, *root).unwrap();
        for row in std::fs::read_to_string(source.join("content-roots.tsv"))
            .unwrap()
            .lines()
            .skip(1)
        {
            let c = row.split('\t').collect::<Vec<_>>();
            if c[0] != state.to_string() {
                continue;
            }
            let value = fs.resolve(&LogicalPath::new(c[1]).unwrap()).unwrap().value;
            assert_eq!(value.content_root, id(c[3]));
            let mut bytes = Vec::new();
            layerfs_telemetry::timer::Timing::disabled("retained.component", |scope| {
                layerfs_content::read_all(
                    &reader,
                    value.content_root,
                    &mut bytes,
                    scope.child("read"),
                )
            })
            .0
            .unwrap();
            assert_eq!(
                bytes,
                std::fs::read(raw_source.join(format!("state{state}-{}.raw", c[1]))).unwrap()
            );
        }
    }
    // Fresh SQL owners prove persisted immutable/history access after reopen,
    // separately from the authenticated native path above.
    let reopened = Arc::new(StrictCatalog::open_read_only(&path).unwrap());
    let history = sqlite::open_read_only(
        &directory.join("history.sqlite"),
        b"strict component history",
        [7; 32],
    )
    .unwrap();
    let file_view = WaveReader {
        catalog: reopened.clone(),
        provider: provider.clone(),
        scope: reopened.capture(None).unwrap(),
        logical_use: LogicalUse::RegularFileGraph,
        capacities: capacities(),
    };
    let meta_view = WaveReader {
        logical_use: LogicalUse::MetadataGraph,
        ..file_view.clone()
    };
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    for (state, (root, commit)) in retained.iter().zip(&commits).enumerate() {
        let record = history.commit(*commit).unwrap().unwrap();
        assert_eq!(record.root, root.0);
        assert_eq!(
            record.parent,
            if state == 0 {
                None
            } else {
                Some(commits[state - 1])
            }
        );
        let mut fs = FilesystemRead::new(&meta_view, *root).unwrap();
        for row in std::fs::read_to_string(source.join("content-roots.tsv"))
            .unwrap()
            .lines()
            .skip(1)
        {
            let c = row.split('\t').collect::<Vec<_>>();
            if c[0] != state.to_string() {
                continue;
            }
            let value = fs.resolve(&LogicalPath::new(c[1]).unwrap()).unwrap().value;
            assert_eq!(value.content_root, id(c[3]));
            let mut bytes = Vec::new();
            layerfs_telemetry::timer::Timing::disabled("reopen.bytes", |scope| {
                layerfs_content::read_all(
                    &file_view,
                    value.content_root,
                    &mut bytes,
                    scope.child("read"),
                )
            })
            .0
            .unwrap();
            assert_eq!(
                bytes,
                std::fs::read(raw_source.join(format!("state{state}-{}.raw", c[1]))).unwrap()
            );
        }
        let inode = fs.root().inode_table();
        let canonical = meta_view.read_canonical(inode).unwrap();
        let leaf = layerfs_content::inode_leaf::InodeLeaf::decode(&canonical).unwrap();
        let selected = reopened
            .location(meta_view.scope, PlacementDomain::Metadata, inode)
            .unwrap()
            .unwrap();
        for row in leaf.rows {
            let value = layerfs_content::inode_leaf::decode_inode_value(&row.value).unwrap();
            for (child, usage) in [
                (value.metadata_root, LogicalUse::MetadataGraph),
                (
                    value.content_root,
                    if value.kind == layerfs_content::inode_leaf::InodeKind::RegularFile {
                        LogicalUse::RegularFileGraph
                    } else {
                        LogicalUse::MetadataGraph
                    },
                ),
            ] {
                let (role, _) = reopened.descriptor(child).unwrap().unwrap();
                let domain = phase6_live_probe::strict_read::placement(usage, role);
                let facts:i64=db.query_row("SELECT count(*) FROM object_references r JOIN locators l USING(locator_id) WHERE l.id=?1 AND l.domain=1 AND l.body_order=?2 AND r.logical_use=1 AND r.child_id=?3 AND r.child_domain=?4 AND r.child_use=?5",rusqlite::params![inode.as_bytes().as_slice(),selected.location.pack_id,child.as_bytes().as_slice(),domain as i64,usage as i64],|row|row.get(0)).unwrap();
                assert!(facts>0,"each actual inode value must retain its independently typed metadata/content reference");
            }
        }
    }
    assert_eq!(
        history
            .branch_snapshot(branch)
            .unwrap()
            .unwrap()
            .effective_root,
        retained[2].0
    );
    drop(db);
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let inode_edges:i64=db.query_row("SELECT count(*) FROM object_references r JOIN locators l USING(locator_id) JOIN identities i ON i.id=l.id WHERE i.role=6",[],|row|row.get(0)).unwrap();
    assert!(inode_edges>=8,"actual filesystem producer must bind inode-value roots, unlike raw synthetic pooled fixtures");
    let regular:i64=db.query_row("SELECT count(*) FROM object_references r JOIN locators l USING(locator_id) JOIN identities i ON i.id=l.id WHERE i.role=6 AND r.child_use=0",[],|row|row.get(0)).unwrap();
    assert!(regular >= 3);
    let shadows: i64 = db
        .query_row(
            "SELECT count(*) FROM bodies WHERE domain=0 AND metadata IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(shadows, 0);
    println!("SP1 edit-operation-v1 actualfilesystem capture/native+realpayloadMinio+globalSQL+C5 component PASS3 retained states/oldrawbytes/editFSroots; inodeedges{inode_edges}; PUBLICSDK/Exec/FUSE/S2 and freshstream/editroot equivalence NOT_RUN");
    drop(engine);
    drop(reader);
    drop(meta);
    drop(native);
    server.join().unwrap();
}
