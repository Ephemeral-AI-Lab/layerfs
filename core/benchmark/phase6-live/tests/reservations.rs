//! Actual C5 SQLite authority and daemon SQLite; no fake allocator or clock.
use layerfs_content::{
    filesystem::{profile_id, scope_for_seed},
    ObjectId,
};
use layerfs_history::{
    catalog::{HistoryCatalog, HistoryCatalogConfig},
    records::ReserveRequest,
    sqlite,
};
use phase6_live_probe::{
    engine::Engine,
    reservations::{Book, Grant, Owner, Pages, PAGE},
    wire::{Bytes, Snapshot},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
fn fresh() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "phase6-reservations-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn authority(path: &std::path::Path) -> sqlite::SqliteCatalog {
    let db = sqlite::create(
        &path.join("history.sqlite"),
        &HistoryCatalogConfig {
            binding_key: b"real reservation authority".to_vec(),
            incarnation: 1,
            cursor_key: [9; 32],
        },
    )
    .unwrap();
    db.reserve_inodes(&ReserveRequest {
        scope: scope_for_seed([8; 32]).object(),
        count: 1,
    })
    .unwrap();
    db
}
fn context() -> (Owner, Snapshot) {
    let owner = Owner {
        workspace: b"ordinary-workspace".to_vec(),
        incarnation: [5; 32],
        project: [1; 17],
        branch: [2; 17],
    };
    let snapshot = Snapshot {
        stack: owner.project,
        branch: owner.branch,
        base: [3; 33],
        head: None,
        root: *ObjectId::for_bytes(b"selected root").as_bytes(),
        scope: *scope_for_seed([8; 32]).object().as_bytes(),
        profile: *profile_id().as_bytes(),
    };
    (owner, snapshot)
}
#[test]
fn real_c5_pages_nonoverlap_and_invalid_owner_refuses_without_consumption() {
    let path = fresh();
    let db = authority(&path);
    let (owner, s) = context();
    let mut book = Book::default();
    let first = book.bootstrap(&db, owner.clone(), &s).unwrap();
    assert_eq!((first.start, first.end, first.sequence), (2, 66, 1));
    let mut wrong = owner.clone();
    wrong.incarnation = [6; 32];
    assert!(book.next(&db, &wrong, s.scope, s.profile, 2, 66).is_err());
    assert!(book.next(&db, &owner, s.scope, s.profile, 3, 66).is_err());
    assert!(book.next(&db, &owner, s.scope, s.profile, 2, 65).is_err());
    assert!(book.next(&db, &owner, [4; 32], s.profile, 2, 66).is_err());
    assert!(book.next(&db, &owner, s.scope, [4; 32], 2, 66).is_err());
    let interleaved = db
        .reserve_inodes(&ReserveRequest {
            scope: scope_for_seed([8; 32]).object(),
            count: 3,
        })
        .unwrap();
    assert_eq!(
        interleaved.start, 66,
        "invalid requests consumed no serials"
    );
    let second = book.next(&db, &owner, s.scope, s.profile, 2, 66).unwrap();
    assert_eq!((second.start, second.end), (69, 133));
    assert!(
        book.next(&db, &owner, s.scope, s.profile, 2, 66).is_err(),
        "consumed grant never replayed"
    );
    let third = book.next(&db, &owner, s.scope, s.profile, 3, 133).unwrap();
    assert_eq!(third.start, 133);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn context_refusal_before_bootstrap_and_failed_provider_stays_pending() {
    let path = fresh();
    let db = authority(&path);
    let (owner, s) = context();
    let mut book = Book::default();
    let mut wrong = owner.clone();
    wrong.branch = [7; 17];
    assert!(book.bootstrap(&db, wrong, &s).is_err());
    let grant = book.bootstrap(&db, owner.clone(), &s).unwrap();
    assert_eq!(grant.start, 2);
    assert!(book.bootstrap(&db, owner.clone(), &s).is_err());
    let readonly = sqlite::open_read_only(
        &path.join("history.sqlite"),
        b"real reservation authority",
        [9; 32],
    )
    .unwrap();
    assert!(book
        .next(&readonly, &owner, s.scope, s.profile, 2, grant.end)
        .is_err());
    assert!(book
        .next(&db, &owner, s.scope, s.profile, 2, grant.end)
        .unwrap_err()
        .contains("pending"));
    let remaining = db
        .reserve_inodes(&ReserveRequest {
            scope: scope_for_seed([8; 32]).object(),
            count: 1,
        })
        .unwrap();
    assert_eq!(remaining.start, 66);
    drop(readonly);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn grant_exact_wire_eof_context_and_terminal_serials() {
    let (owner, s) = context();
    let mut grant = Grant {
        owner: owner.clone(),
        scope: s.scope,
        profile: s.profile,
        sequence: 1,
        start: 2,
        count: PAGE,
        end: 66,
    };
    let mut bytes = Vec::new();
    grant.encode(&mut bytes);
    let mut b = Bytes::new(&bytes);
    assert_eq!(Grant::read(&mut b).unwrap(), grant);
    b.done().unwrap();
    bytes.push(0);
    let mut b = Bytes::new(&bytes);
    Grant::read(&mut b).unwrap();
    assert!(b.done().is_err());
    assert!(Grant::read(&mut Bytes::new(&bytes[..bytes.len() - 2])).is_err());
    let mut wrong = owner;
    wrong.project = [9; 17];
    assert!(grant.matches(&wrong, &s).is_err());
    grant.end = 65;
    assert!(grant.check().is_err());
    grant.end = 66;
    grant.count = 63;
    assert!(grant.check().is_err());
    grant.count = 64;
    grant.start = i64::MAX as u64 - 64;
    grant.end = i64::MAX as u64;
    grant.check().unwrap();
}
struct ActualPages {
    history: sqlite::SqliteCatalog,
    book: Mutex<Book>,
    context: Grant,
}
impl Pages for ActualPages {
    fn next(&self, previous: u64) -> Result<Grant, String> {
        let mut book = self.book.lock().unwrap();
        // The engine endpoint determines the sequence of these contiguous actual C5 pages.
        let sequence = (previous - 2) / PAGE + 1;
        book.next(
            &self.history,
            &self.context.owner,
            self.context.scope,
            self.context.profile,
            sequence,
            previous,
        )
    }
}
#[test]
fn real_engine_creation_crosses_page_without_population_scan() {
    let path = fresh();
    let db = authority(&path);
    let (owner, s) = context();
    let mut book = Book::default();
    let grant = book.bootstrap(&db, owner, &s).unwrap();
    let provider = Arc::new(ActualPages {
        history: db,
        book: Mutex::new(book),
        context: grant.clone(),
    });
    let mut engine =
        Engine::create_reserved(&path.join("engine"), &grant, provider.clone()).unwrap();
    for i in 0..130 {
        let n = engine
            .create_node(1, format!("file{i}").as_bytes(), 1, 0o644)
            .unwrap();
        assert_eq!(n.id, i + 2);
    }
    assert_eq!((engine.next, engine.end), (132, 194));
    let node = engine.node(65).unwrap();
    engine.write(node.id, 0, b"accepted bytes").unwrap();
    let mut out = [0; 14];
    engine.read(node.id, 0, &mut out).unwrap();
    assert_eq!(&out, b"accepted bytes");
    let highwater = provider
        .history
        .reserve_inodes(&ReserveRequest {
            scope: scope_for_seed([8; 32]).object(),
            count: 1,
        })
        .unwrap();
    assert_eq!(highwater.start, 194);
    drop(engine);
    drop(provider);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn exhausted_local_range_refuses_before_inode_or_name_effect() {
    let path = fresh();
    let mut engine = Engine::create(&path.join("engine"), 2).unwrap();
    engine.next = engine.end;
    assert_eq!(
        engine.create_node(1, b"refused", 1, 0o644).err().unwrap(),
        "ENOSPC"
    );
    assert_eq!(engine.lookup(1, b"refused").unwrap(), None);
    assert_eq!(engine.nodes, 1);
    assert_eq!(engine.revision, 1);
    let (owner, s) = context();
    let grant = Grant {
        owner,
        scope: s.scope,
        profile: s.profile,
        sequence: 1,
        start: i64::MAX as u64 - 64,
        count: 64,
        end: i64::MAX as u64,
    };
    let db = authority(&path);
    let provider = Arc::new(ActualPages {
        history: db,
        book: Mutex::new(Book::default()),
        context: grant.clone(),
    });
    let terminal = Engine::create_reserved(&path.join("terminal"), &grant, provider).unwrap();
    assert_eq!(terminal.end, i64::MAX);
    drop(terminal);
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn real_refused_c5_refill_quarantines_creation_and_preserves_prior_bytes() {
    let path = fresh();
    let db = authority(&path);
    let (owner, s) = context();
    let mut book = Book::default();
    let grant = book.bootstrap(&db, owner, &s).unwrap();
    let readonly = sqlite::open_read_only(
        &path.join("history.sqlite"),
        b"real reservation authority",
        [9; 32],
    )
    .unwrap();
    let provider = Arc::new(ActualPages {
        history: readonly,
        book: Mutex::new(book),
        context: grant.clone(),
    });
    let mut engine =
        Engine::create_reserved(&path.join("engine"), &grant, provider.clone()).unwrap();
    let file = engine.create_node(1, b"accepted", 1, 0o644).unwrap();
    engine.write(file.id, 0, b"prior").unwrap();
    for i in 1..64 {
        engine
            .create_node(1, format!("file{i}").as_bytes(), 1, 0o644)
            .unwrap();
    }
    assert!(engine.create_node(1, b"refused", 1, 0o644).is_err());
    assert!(engine
        .create_node(1, b"again", 1, 0o644)
        .err()
        .unwrap()
        .contains("no resend"));
    assert_eq!(engine.lookup(1, b"refused").unwrap(), None);
    assert_eq!((engine.next, engine.end, engine.nodes), (66, 66, 65));
    let mut out = [0; 5];
    engine.read(file.id, 0, &mut out).unwrap();
    assert_eq!(&out, b"prior");
    let remaining = db
        .reserve_inodes(&ReserveRequest {
            scope: scope_for_seed([8; 32]).object(),
            count: 1,
        })
        .unwrap();
    assert_eq!(remaining.start, 66);
    drop(engine);
    drop(provider);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn native_range_reply_is_exact_and_bad_reply_never_resends_consumed_grant() {
    use layerfs_bridge::adapters::native::{
        connection::{self, Peer, VerifiedPeer},
        protocol::{Frame, Kind},
    };
    use phase6_live_probe::{
        metadata::Remote, metadata_session::Session, reservations::NativePages, wire,
    };
    use std::{
        net::TcpListener,
        time::{SystemTime, UNIX_EPOCH},
    };
    let path = fresh();
    let db = authority(&path);
    let (owner, s) = context();
    let mut book = Book::default();
    let initial = book.bootstrap(&db, owner, &s).unwrap();
    let expected = initial.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = std::thread::spawn(move || {
        let peer = Peer {
            selector: 1,
            public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
            expires_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
        };
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer]).unwrap();
        for sequence in 2..=3 {
            let frame = c.receive.read().unwrap();
            assert_eq!(frame.kind, Kind::Begin);
            assert_eq!(frame.bytes[7], 7);
            assert!(frame.bytes.starts_with(wire::PREFIX));
            let mut b = Bytes::new(&frame.bytes[8..]);
            let owner = Owner::read(&mut b).unwrap();
            let scope = b.take().unwrap();
            let profile = b.take().unwrap();
            let seq = b.u64().unwrap();
            let previous = b.u64().unwrap();
            b.done().unwrap();
            assert_eq!(seq, sequence);
            let mut grant = book
                .next(&db, &owner, scope, profile, seq, previous)
                .unwrap();
            if sequence == 3 {
                grant.profile = [4; 32];
            }
            let mut bytes = wire::PREFIX.to_vec();
            grant.encode(&mut bytes);
            c.send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .unwrap();
        }
        let remaining = db
            .reserve_inodes(&ReserveRequest {
                scope: ObjectId::from_bytes(&expected.scope).unwrap(),
                count: 1,
            })
            .unwrap();
        assert_eq!(
            remaining.start, 194,
            "malformed reply's C5 grant stays consumed"
        );
    });
    let remote = Remote {
        endpoint: address.to_string(),
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    let pages = NativePages::new(remote.clone(), initial);
    let known = pages.next(66).unwrap();
    assert_eq!((known.sequence, known.start, known.end), (2, 66, 130));
    assert!(pages.next(130).unwrap_err().contains("identity"));
    assert!(pages.next(130).unwrap_err().contains("no resend"));
    assert_eq!(remote.statistics().unwrap().calls[7], 2);
    thread.join().unwrap();
    drop(pages);
    drop(remote);
    std::fs::remove_dir_all(path).unwrap();
}
