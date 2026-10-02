//! Real SQLite metadata pages over the existing authenticated native transport.
use layerfs_bridge::adapters::native::{
    connection::{self, Peer, VerifiedPeer},
    protocol::{Frame, Kind},
};
use layerfs_content::{file::mapping::encode_chunk_object, FinalizedObject, ObjectRole};
use layerfs_storage::{access::ObjectLocation, encoding::CompressionWorkspace};
use phase6_live_probe::{
    packing,
    strict_catalog::{LogicalUse, PlacementDomain, Registration, StrictCatalog},
    strict_remote::{NativeCatalog, RemoteError, Session},
    strict_wire::{self, Bytes, Request, BODY_PAGE},
};
use sha2::{Digest, Sha256};
use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Database {
    path: PathBuf,
    catalog: Arc<StrictCatalog>,
}
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "strict-wire-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
        Self { path, catalog }
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).unwrap();
    }
}
fn call(catalog: &StrictCatalog, request: &Request<'_>) -> Vec<u8> {
    let reply = strict_wire::handle(catalog, &request.encode().unwrap()).unwrap();
    strict_wire::response(request.action(), &reply)
        .unwrap()
        .unwrap()
        .to_vec()
}
fn signed_reply(reply: &[u8]) -> i64 {
    let mut b = Bytes::new(reply);
    let result = b.signed().unwrap();
    b.done().unwrap();
    result
}
fn pack() -> packing::Pack {
    let mut seed = 0x912f4_u64;
    let raw: Vec<u8> = (0..32768)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect();
    let object =
        FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(&raw).unwrap()).unwrap();
    packing::build(vec![object], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap()
}
fn row(
    pack: &packing::Pack,
    order: i64,
    domain: PlacementDomain,
    usage: LogicalUse,
) -> Registration {
    let r = &pack.rows[0];
    Registration {
        location: ObjectLocation {
            object_id: r.id,
            role: r.role,
            canonical_length: r.length,
            pack_id: order,
            group_number: r.group as usize,
            record_number: r.record as usize,
        },
        domain,
        logical_use: usage,
        references: vec![],
        base: None,
    }
}
#[test]
fn page_transfer_roundtrips_and_missing_domain_remains_absent() {
    let db = Database::new();
    let save = signed_reply(&call(&db.catalog, &Request::BeginSave));
    let pack = pack();
    assert!(pack.bytes.len() > BODY_PAGE);
    let digest: [u8; 32] = Sha256::digest(&pack.bytes).into();
    let order = signed_reply(&call(
        &db.catalog,
        &Request::BeginMetadataBody(save, digest, pack.bytes.len()),
    ));
    for (page, bytes) in pack.bytes.chunks(BODY_PAGE).enumerate() {
        assert!(call(
            &db.catalog,
            &Request::AppendMetadataBody(save, order, page * BODY_PAGE, bytes)
        )
        .is_empty());
    }
    assert!(call(&db.catalog, &Request::FinishMetadataBody(save, order)).is_empty());
    let scope = db.catalog.capture(Some(save)).unwrap();
    let rows = [row(
        &pack,
        order,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    )];
    call(&db.catalog, &Request::RegisterRows(scope, save, &rows));
    let mut assembled = Vec::new();
    while assembled.len() < pack.bytes.len() {
        let reply = call(
            &db.catalog,
            &Request::BodyPage(
                scope,
                PlacementDomain::Metadata,
                order,
                assembled.len(),
                BODY_PAGE,
            ),
        );
        let mut b = Bytes::new(&reply);
        assert_eq!(b.take::<32>().unwrap(), digest);
        assert!(b.bool().unwrap());
        assert_eq!(b.size().unwrap(), pack.bytes.len());
        assembled.extend_from_slice(b.blob().unwrap());
        b.done().unwrap();
    }
    assert_eq!(assembled, pack.bytes);
    let reply = call(
        &db.catalog,
        &Request::Locations(scope, PlacementDomain::FilePayload, &[pack.rows[0].id]),
    );
    let mut b = Bytes::new(&reply);
    assert_eq!(b.count(128).unwrap(), 1);
    assert!(b.optional_located().unwrap().is_none());
    b.done().unwrap();
    let payload = signed_reply(&call(
        &db.catalog,
        &Request::RegisterPayloadBody(save, digest),
    ));
    let scope = db.catalog.capture(Some(save)).unwrap();
    call(
        &db.catalog,
        &Request::RegisterRows(
            scope,
            save,
            &[row(
                &pack,
                payload,
                PlacementDomain::FilePayload,
                LogicalUse::RegularFileGraph,
            )],
        ),
    );
    let reply = call(
        &db.catalog,
        &Request::BodyPage(scope, PlacementDomain::FilePayload, payload, 0, BODY_PAGE),
    );
    let mut b = Bytes::new(&reply);
    assert_eq!(b.take::<32>().unwrap(), digest);
    assert!(!b.bool().unwrap());
    assert!(b.blob().unwrap().is_empty());
    b.done().unwrap();
}
#[test]
fn malformed_requests_refuse_before_mutation_and_old_profile_is_incompatible() {
    let db = Database::new();
    assert!(strict_wire::handle(&db.catalog, b"P6META7\x01").is_err());
    assert!(strict_wire::handle(&db.catalog, b"P6SP1V1\x01").is_err());
    let mut bytes = Request::BeginSave.encode().unwrap();
    bytes.push(0);
    let reply = strict_wire::handle(&db.catalog, &bytes).unwrap();
    assert!(strict_wire::response(1, &reply).unwrap().is_err());
    assert_eq!(signed_reply(&call(&db.catalog, &Request::BeginSave)), 1);
    assert!(
        Request::AppendMetadataBody(1, 1, 0, &vec![0; BODY_PAGE + 1])
            .encode()
            .is_err()
    );
}
fn peer() -> Peer {
    Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
        expires_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 60,
    }
}
#[test]
fn real_native_session_transfers_sql_metadata_and_preserves_definite_refusal() {
    let db = Database::new();
    let catalog = db.catalog.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        while let Ok(frame) = c.receive.read() {
            assert_eq!(frame.kind, Kind::Begin);
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    let remote = NativeCatalog {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    let save = remote.begin_save().unwrap();
    let pack = pack();
    let digest: [u8; 32] = Sha256::digest(&pack.bytes).into();
    let order = remote
        .register_body(PlacementDomain::Metadata, save, digest, Some(&pack.bytes))
        .unwrap();
    let scope = remote.capture(Some(save)).unwrap();
    assert_eq!(
        remote
            .body(scope, PlacementDomain::Metadata, order)
            .unwrap()
            .1
            .unwrap(),
        pack.bytes
    );
    assert!(matches!(
        remote.body(scope, PlacementDomain::FilePayload, order),
        Err(RemoteError::Definite(_))
    ));
    assert!(remote.capture(Some(save)).is_ok());
    assert_eq!(remote.statistics().unwrap().connect_attempts, 1);
    drop(remote);
    server.join().unwrap();
}
#[test]
fn wrong_native_reply_identity_quarantines_without_replay() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        let frame = c.receive.read().unwrap();
        let bytes = [strict_wire::PREFIX.as_slice(), &[1, 0], &1u64.to_be_bytes()].concat();
        c.send
            .write(&Frame {
                kind: Kind::Success,
                id: frame.id + 1,
                bytes,
            })
            .unwrap();
    });
    let key = *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key();
    let mut session = Session::default();
    assert!(matches!(
        session.call(address, 1, &[7; 32], &key, &Request::BeginSave),
        Err(RemoteError::Unknown(_))
    ));
    assert!(matches!(
        session.call(address, 1, &[7; 32], &key, &Request::BeginSave),
        Err(RemoteError::Unknown(_))
    ));
    assert_eq!(session.statistics.connect_attempts, 1);
    assert_eq!(session.statistics.calls[1], 1);
    server.join().unwrap();
}

#[test]
fn reserved_metadata_transfer_keeps_body_order_and_frozen_digest() {
    let db = Database::new();
    let save = signed_reply(&call(&db.catalog, &Request::BeginSave));
    let pack = pack();
    let digest: [u8; 32] = Sha256::digest(&pack.bytes).into();
    let order = signed_reply(&call(
        &db.catalog,
        &Request::ReserveBody(PlacementDomain::Metadata, save, digest),
    ));
    call(
        &db.catalog,
        &Request::BeginReservedMetadataBody(save, order, digest, pack.bytes.len()),
    );
    for (page, bytes) in pack.bytes.chunks(BODY_PAGE).enumerate() {
        call(
            &db.catalog,
            &Request::AppendMetadataBody(save, order, page * BODY_PAGE, bytes),
        );
    }
    call(&db.catalog, &Request::FinishMetadataBody(save, order));
    let scope = db.catalog.capture(Some(save)).unwrap();
    assert_eq!(
        db.catalog
            .body(scope, PlacementDomain::Metadata, order)
            .unwrap(),
        (digest, Some(pack.bytes))
    );
    let spare = signed_reply(&call(
        &db.catalog,
        &Request::ReserveBody(PlacementDomain::Metadata, save, [4; 32]),
    ));
    call(&db.catalog, &Request::DiscardReservedBody(save, spare));
    assert!(db
        .catalog
        .body(scope, PlacementDomain::Metadata, spare)
        .is_err());
}

#[test]
fn authenticated_reply_engine_change_quarantines_captured_scope() {
    let first = Database::new();
    let second = Database::new();
    assert_ne!(
        first.catalog.cache_namespace(),
        second.catalog.cache_namespace()
    );
    let a = first.catalog.clone();
    let b = second.catalog.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        for catalog in [a, b] {
            let frame = c.receive.read().unwrap();
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    let remote = NativeCatalog {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    remote.begin_save().unwrap();
    assert_eq!(remote.cache_namespace(), first.catalog.cache_namespace());
    assert!(matches!(remote.capture(None), Err(RemoteError::Unknown(_))));
    assert!(matches!(remote.capture(None), Err(RemoteError::Unknown(_))));
    assert_eq!(remote.statistics().unwrap().calls[0], 1);
    server.join().unwrap();
}

#[test]
fn native_catalog_drives_existing_metadata_writer_and_c2_reader_without_provider_io() {
    use layerfs_content::{AuthenticatedObjects, FinalizedConsumer};
    use layerfs_storage::{StorageCapacities, StoragePolicy};
    use phase6_live_probe::{
        minio::Minio,
        strict_client::Catalog,
        strict_read::{Access, ReadOwner},
        strict_writer::{Consumer, Writer},
    };
    use std::{cell::RefCell, rc::Rc};
    let db = Database::new();
    let catalog = db.catalog.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        while let Ok(frame) = c.receive.read() {
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    let native = Arc::new(NativeCatalog {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    });
    let catalog: Arc<dyn Catalog> = native.clone();
    // No HTTP listener is supplied. Any provider call would fail this SQL-only path.
    let provider = Minio {
        authority: "127.0.0.1:9".into(),
        bucket: "metadata-only-component".into(),
        access: "unused".into(),
        secret: "unused".into(),
        stats: Arc::new(Mutex::new(Default::default())),
    };
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let owner = Rc::new(RefCell::new(
        Writer::new(catalog.clone(), provider.clone(), capacities).unwrap(),
    ));
    let mut consumer = Consumer {
        owner: owner.clone(),
        logical_use: LogicalUse::MetadataGraph,
    };
    let raw: Vec<u8> = (0..32768usize)
        .map(|n| ((n * 71 + n / 13) % 251) as u8)
        .collect();
    let chunk =
        FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(&raw).unwrap()).unwrap();
    let expected = chunk.canonical().to_vec();
    let id = chunk.id();
    consumer.accept(chunk).unwrap();
    let fixture=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/evidence/old-producer-v1/pool0.canonical");
    let leaf =
        FinalizedObject::new(ObjectRole::InodeLeaf, std::fs::read(fixture).unwrap()).unwrap();
    let leaf_expected = leaf.canonical().to_vec();
    let leaf_id = leaf.id();
    consumer.accept(leaf).unwrap();
    owner.borrow_mut().finish_storage().unwrap();
    assert!(owner.borrow().counts.private_group_seals > 0);
    assert!(owner.borrow().pool_counts.groups > 0);
    assert_eq!(owner.borrow().counts.payload_packs, 0);
    let scope = catalog.capture(Some(owner.borrow().save)).unwrap();
    let access = Access {
        catalog: catalog.as_ref(),
        provider: &provider,
        scope,
        logical_use: LogicalUse::MetadataGraph,
    };
    let mut reader = ReadOwner::new().unwrap();
    let before = provider.statistics().unwrap();
    assert_eq!(
        reader.read(&access, &capacities, &[id, leaf_id]).unwrap(),
        vec![expected, leaf_expected]
    );
    let after = provider.statistics().unwrap();
    assert_eq!(after.get_calls - before.get_calls, 0);
    assert_eq!(after.put_calls - before.put_calls, 0);
    let construction = phase6_live_probe::strict_writer::ConstructionReader {
        owner: owner.clone(),
        logical_use: LogicalUse::MetadataGraph,
    };
    assert_eq!(
        construction.read_canonical(id).unwrap(),
        encode_chunk_object(&raw).unwrap()
    );
    drop(construction);
    drop(consumer);
    drop(owner);
    drop(catalog);
    drop(native);
    server.join().unwrap();
}

#[test]
fn native_owned_save_context_is_immutable_and_anonymous_save_has_no_public_binding() {
    use phase6_live_probe::{reservations::Owner, strict_catalog::SaveContext};
    let db = Database::new();
    let catalog = db.catalog.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        while let Ok(frame) = c.receive.read() {
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    let remote = NativeCatalog {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    let context = SaveContext {
        owner: Owner {
            workspace: b"ordinary_workspace".to_vec(),
            incarnation: [5; 32],
            project: layerfs_history::LayerStackId::from_authority([1; 16]).to_bytes(),
            branch: layerfs_history::BranchId::from_authority([2; 16]).to_bytes(),
        },
        generation: 1,
        scope: *layerfs_content::filesystem::scope_for_seed([8; 32])
            .object()
            .as_bytes(),
        profile: *layerfs_content::filesystem::profile_id().as_bytes(),
        base_root: *layerfs_content::ObjectId::for_bytes(b"captured independent base").as_bytes(),
    };
    let save = remote.begin_save_owned(&context).unwrap();
    assert!(db.catalog.matches_save_owner(save, &context).unwrap());
    for field in 0..8 {
        let mut wrong = context.clone();
        match field {
            0 => wrong.owner.workspace = b"foreign_workspace".to_vec(),
            1 => wrong.owner.incarnation = [6; 32],
            2 => wrong.owner.project[1] ^= 1,
            3 => wrong.owner.branch[1] ^= 1,
            4 => wrong.generation += 1,
            5 => wrong.scope[0] ^= 1,
            6 => wrong.profile[0] ^= 1,
            _ => wrong.base_root[0] ^= 1,
        };
        assert!(!db.catalog.matches_save_owner(save, &wrong).unwrap());
    }
    assert!(matches!(
        remote.begin_save_owned(&context),
        Err(RemoteError::Definite(_))
    ));
    assert!(db.catalog.matches_save_owner(save, &context).unwrap());
    let client: &dyn phase6_live_probe::strict_client::Catalog = &remote;
    let refused = client.begin_save_owned(&context).unwrap_err();
    assert!(phase6_live_probe::strict_catalog::is_definite_catalog_error(&refused));
    let anonymous = remote.begin_save().unwrap();
    assert!(!db.catalog.matches_save_owner(anonymous, &context).unwrap());
    assert_eq!(remote.statistics().unwrap().calls[29], 3);
    drop(remote);
    server.join().unwrap();
}

#[test]
fn native_registration_preserves_order_across_encoded_byte_pages() {
    use layerfs_content::file::mapping::{encode_node, ExtentNode, ExtentSlice};
    use phase6_live_probe::strict_catalog::Reference;
    let db = Database::new();
    let catalog = db.catalog.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let server_observed = observed.clone();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer()]).unwrap();
        while let Ok(frame) = c.receive.read() {
            assert!(frame.bytes.len() <= strict_wire::FRAME_LIMIT);
            if frame.bytes[7] == 11 {
                server_observed.lock().unwrap().push(frame.bytes.len());
            }
            let bytes = strict_wire::handle(&catalog, &frame.bytes).unwrap();
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
    });
    let remote = NativeCatalog {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    let save = remote.begin_save().unwrap();
    let children: Vec<_> = (0..183u16)
        .map(|i| {
            FinalizedObject::new(
                ObjectRole::Chunk,
                encode_chunk_object(&i.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let ids: Vec<_> = children.iter().map(FinalizedObject::id).collect();
    let mut workspace = CompressionWorkspace::new().unwrap();
    for pack in packing::build(children, &mut workspace).unwrap() {
        let order = remote
            .register_body(
                PlacementDomain::Metadata,
                save,
                Sha256::digest(&pack.bytes).into(),
                Some(&pack.bytes),
            )
            .unwrap();
        let scope = remote.capture(Some(save)).unwrap();
        let rows: Vec<_> = pack
            .rows
            .iter()
            .map(|r| Registration {
                location: ObjectLocation {
                    object_id: r.id,
                    role: r.role,
                    canonical_length: r.length,
                    pack_id: order,
                    group_number: r.group as usize,
                    record_number: r.record as usize,
                },
                domain: PlacementDomain::Metadata,
                logical_use: LogicalUse::MetadataGraph,
                references: vec![],
                base: None,
            })
            .collect();
        for page in rows.chunks(128) {
            remote.register(scope, save, page).unwrap();
        }
    }
    // C1 caps a canonical leaf at 128 entries. Eight overlapping valid pages
    // keep all 183 children and increase the byte-paging challenge to 1024 edges.
    let parent_children: Vec<Vec<_>> = (0..8)
        .map(|page| {
            (0..128)
                .map(|entry| ids[(page * 11 + entry) % ids.len()])
                .collect()
        })
        .collect();
    let parents: Vec<_> = parent_children
        .iter()
        .map(|children| {
            FinalizedObject::new(
                ObjectRole::ExtentLeaf,
                encode_node(
                    &ExtentNode::Leaf {
                        subtree_logical_bytes: children.len() as u64,
                        extents: children
                            .iter()
                            .map(|id| ExtentSlice::new(*id, 0, 1).unwrap())
                            .collect(),
                    },
                    true,
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect();
    let parent_ids: Vec<_> = parents.iter().map(FinalizedObject::id).collect();
    let mut rows = Vec::new();
    for pack in packing::build(parents, &mut workspace).unwrap() {
        let order = remote
            .register_body(
                PlacementDomain::Metadata,
                save,
                Sha256::digest(&pack.bytes).into(),
                Some(&pack.bytes),
            )
            .unwrap();
        for r in &pack.rows {
            let index = parent_ids.iter().position(|id| *id == r.id).unwrap();
            rows.push(Registration {
                location: ObjectLocation {
                    object_id: r.id,
                    role: r.role,
                    canonical_length: r.length,
                    pack_id: order,
                    group_number: r.group as usize,
                    record_number: r.record as usize,
                },
                domain: PlacementDomain::Metadata,
                logical_use: LogicalUse::MetadataGraph,
                references: parent_children[index]
                    .iter()
                    .map(|id| Reference {
                        id: *id,
                        domain: PlacementDomain::Metadata,
                        logical_use: LogicalUse::MetadataGraph,
                    })
                    .collect(),
                base: None,
            });
        }
    }
    // Keep caller order explicit even if pack construction sorted physical rows.
    rows.sort_by_key(|r| {
        parent_ids
            .iter()
            .position(|id| *id == r.location.object_id)
            .unwrap()
    });
    let scope = remote.capture(Some(save)).unwrap();
    let before = remote.statistics().unwrap().calls[11];
    for row in &rows {
        assert_eq!(
            Request::RegisterRows(scope, save, std::slice::from_ref(row))
                .encode()
                .unwrap()
                .len(),
            43 + strict_wire::registration_bytes(row).unwrap()
        );
    }
    assert!(Request::RegisterRows(scope, save, &rows).encode().is_err());
    let normalized = remote.register(scope, save, &rows).unwrap();
    assert_eq!(remote.statistics().unwrap().calls[11] - before, 3);
    assert_eq!(
        normalized
            .iter()
            .map(|r| r.location.object_id)
            .collect::<Vec<_>>(),
        parent_ids
    );
    assert!(remote
        .locations(scope, PlacementDomain::Metadata, &parent_ids)
        .unwrap()
        .iter()
        .all(Option::is_some));
    let frames = observed.lock().unwrap();
    assert!(frames.iter().all(|n| *n <= 16384));
    assert_eq!(&frames[frames.len() - 3..], &[13261, 13261, 8855]);
    drop(frames);
    drop(remote);
    server.join().unwrap();
}
