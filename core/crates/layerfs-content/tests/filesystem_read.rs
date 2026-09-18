//! Logical reads: resolve, stat, list, readlink and bounded pagination.

mod support;

use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::filesystem::{DirectoryUpdate, InodeUpdate, LogicalPath, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectId};
use support::filesystem::{synthetic, value, Session};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn file(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("read/meta"),
    )
}

fn directory() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("read/unused"),
        synthetic("read/dir-meta"),
    )
}

/// A directory with `count` files named `e0000..`, plus one symlink.
fn wide(count: usize) -> (Session, u64, Vec<u64>) {
    let mut session = Session::new(1).expect("empty");
    let d = session.allocate();
    let mut serials = Vec::new();
    for _ in 0..count {
        serials.push(session.allocate());
    }
    let link = session.allocate();
    let mut changes = vec![(name("d"), Some(d)), (name("z-link"), Some(link))];
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let mut inodes = vec![InodeUpdate {
        serial: d,
        value: directory(),
    }];
    let mut new = vec![d];
    for (index, serial) in serials.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: file(&format!("read/content-{index:04}")),
        });
        new.push(*serial);
    }
    inodes.push(InodeUpdate {
        serial: link,
        value: value(
            InodeKind::Symlink,
            synthetic("read/link-target"),
            synthetic("read/link-meta"),
        ),
    });
    new.push(link);
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let mut directories = vec![
        DirectoryUpdate { parent: 1, changes },
        DirectoryUpdate {
            parent: d,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("e{index:04}")), Some(*serial)))
                .collect(),
        },
    ];
    directories.sort_by_key(|update| update.parent);
    session
        .apply(&directories, &inodes, &new)
        .expect("wide tree");
    (session, d, serials)
}

#[test]
fn resolve_stat_and_list_agree_on_the_same_tree() {
    let (session, d, serials) = wide(7);
    let mut read = session.read().expect("reader");
    assert_eq!(
        read.stat(&LogicalPath::root()).expect("root").kind,
        InodeKind::Directory
    );
    assert_eq!(
        read.resolve(&LogicalPath::new("d").unwrap())
            .expect("resolve d")
            .serial,
        d
    );
    let page = read
        .list(&LogicalPath::new("d").unwrap(), None, 3, 8192)
        .expect("first page");
    assert_eq!(page.entries.len(), 3);
    assert_eq!(page.entries[0].0.as_str(), "e0000");
    assert!(page.continuation.is_some());
    let next = read
        .list(
            &LogicalPath::new("d").unwrap(),
            page.continuation.as_ref(),
            3,
            8192,
        )
        .expect("second page");
    assert_eq!(next.entries[0].0.as_str(), "e0003");
    assert_eq!(
        page.entries
            .iter()
            .chain(next.entries.iter())
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>(),
        serials[..6].to_vec()
    );
    assert!(matches!(
        read.list(&LogicalPath::new("d/e0000").unwrap(), None, 3, 8192),
        Err(ContentError::WrongLogicalRole)
    ));
    // An unbound name is a *logical* absence and never provider absence: the
    // tree was read successfully and the name is not in it. `MissingObject` here
    // would collapse "this path does not exist" into "the provider does not hold
    // an object this tree names", which the provider contract forbids.
    assert!(matches!(
        read.stat(&LogicalPath::new("d/absent").unwrap()),
        Err(ContentError::PathNotFound)
    ));
    assert!(matches!(
        read.stat(&LogicalPath::new("d/e0000/deeper").unwrap()),
        Err(ContentError::InvalidRecord(_))
    ));
}

/// The two absences are different answers, and this is the test that says so.
///
/// A name no directory binds is a **logical** absence: the tree was read
/// successfully and the name is not in it. A provider that does not hold an
/// object the tree names is a **provider** absence. `object::access`,
/// `cas::provider`, `error::MissingObject` and `architecture/01-boundary.md` all
/// state that these must stay distinguishable - "the object is not here" and
/// "this path does not exist" must not collapse into one error, because only the
/// first is a legitimate reason for a caller to choose a different
/// representation. Before this test, `FilesystemRead::resolve` answered both with
/// `MissingObject`.
#[test]
fn an_unbound_name_is_not_provider_absence() {
    let (session, _d, _serials) = wide(3);

    // The tree reads fine and the name is simply not bound.
    let mut read = session.read().expect("reader");
    let unbound = read
        .stat(&LogicalPath::new("d/absent").unwrap())
        .expect_err("an unbound name must not resolve");
    assert_eq!(unbound, ContentError::PathNotFound);

    // A provider that does not hold the tree's own root object is the other
    // class, and it is not the answer above.
    let empty = support::filesystem::TreeStore::new();
    // Matched rather than `expect_err`: `FilesystemRead` is deliberately not
    // `Debug`, and a reader that printed its own state would be a second way to
    // read the tree.
    let missing = match layerfs_content::filesystem::FilesystemRead::new(
        &empty,
        layerfs_content::filesystem::FilesystemRootId(session.root),
    ) {
        Ok(_) => panic!("an empty provider cannot serve the root"),
        Err(error) => error,
    };
    assert_eq!(missing, ContentError::MissingObject);
    assert_ne!(
        missing, unbound,
        "provider absence and an unbound name must not be the same answer"
    );
}

#[test]
fn listing_is_bounded_by_bytes_as_well_as_count() {
    let (session, _d, _serials) = wide(20);
    let mut read = session.read().expect("reader");
    let path = LogicalPath::new("d").unwrap();
    let by_count = read.list(&path, None, 5, 8192).expect("count bound");
    assert_eq!(by_count.entries.len(), 5);
    // Each name is `e0000` (5 bytes) plus a 2-byte prefix and an 8-byte serial.
    let by_bytes = read.list(&path, None, 64, 15).expect("byte bound");
    assert_eq!(by_bytes.entries.len(), 1, "one 15-byte row fits exactly");
    // A byte bound that cannot fit one row is a refusal, not an exhausted
    // directory: an empty page with no continuation is what the end of a
    // listing looks like, and returning it here would be indistinguishable
    // from it. The 15-byte case above fits exactly and must keep working.
    assert!(matches!(
        read.list(&path, None, 64, 1),
        Err(ContentError::ObjectLimitExceeded { limit: 1, .. })
    ));
    for bound in 1..15_usize {
        assert!(
            matches!(
                read.list(&path, None, 64, bound),
                Err(ContentError::ObjectLimitExceeded { limit, .. }) if limit == bound
            ),
            "a {bound}-byte bound cannot fit one row and must be refused"
        );
    }
    assert!(matches!(
        read.list(&path, None, 0, 8192),
        Err(ContentError::InvalidRecord("listing limit"))
    ));
    // A resumed listing never repeats or skips an entry.
    let mut after = None;
    let mut observed = Vec::new();
    loop {
        let page = read.list(&path, after.as_ref(), 4, 8192).expect("page");
        observed.extend(
            page.entries
                .iter()
                .map(|(name, _)| name.as_str().to_owned()),
        );
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(observed.len(), 20);
    assert!(observed.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn repeated_and_shared_demands_keep_order_and_cardinality() {
    let (session, _d, serials) = wide(60);
    let mut read = session.read().expect("reader");
    let names = vec![
        name("e0000"),
        name("e0030"),
        name("e0000"),
        name("absent"),
        name("e0059"),
    ];
    let found = read
        .lookup_names(&LogicalPath::root(), &names)
        .expect("batch lookup");
    assert_eq!(found.len(), names.len());
    assert_eq!(found[0], None, "the names live in /d, not in the root");
    let found = read
        .lookup_names(&LogicalPath::new("d").unwrap(), &names)
        .expect("batch lookup");
    assert_eq!(found[0], Some(serials[0]));
    assert_eq!(found[2], Some(serials[0]), "a duplicate demand repeats");
    assert_eq!(found[3], None);
    assert_eq!(found[4], Some(serials[59]));
    // A batch of inode demands shares its waves and keeps order.
    let order = vec![serials[5], serials[0], serials[59], 999_999];
    let values = read.lookup_inodes(&order).expect("inode batch");
    assert_eq!(
        values[0].expect("value").content_root,
        synthetic("read/content-0005")
    );
    assert_eq!(
        values[1].expect("value").content_root,
        synthetic("read/content-0000")
    );
    assert_eq!(values[3], None, "an absent serial is absent, not missing");
}

#[test]
fn a_symlink_reads_back_its_exact_stored_target() {
    let mut session = Session::new(1).expect("empty");
    let link = session.allocate();
    let mut target_bytes = vec![0x2f];
    target_bytes.extend_from_slice("target/path".as_bytes());
    let target = SymlinkTarget::new(target_bytes.clone()).expect("target");
    let canonical = target.encode().expect("encode");
    let id = session
        .store
        .insert(layerfs_content::ObjectRole::Symlink, canonical);
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("s"), Some(link))],
            }],
            &[InodeUpdate {
                serial: link,
                value: value(InodeKind::Symlink, id, synthetic("read/meta")),
            }],
            &[link],
        )
        .expect("symlink");
    let mut read = session.read().expect("reader");
    let observed = read
        .readlink(&LogicalPath::new("s").unwrap())
        .expect("readlink");
    assert_eq!(observed.as_bytes(), target_bytes.as_slice());
    // A non-symlink is refused, and a malformed stored target is rejected.
    assert!(matches!(
        read.readlink(&LogicalPath::root()),
        Err(ContentError::WrongLogicalRole)
    ));
    let mut damaged = vec![0_u8; 8];
    damaged.extend_from_slice(b"LFS4LNK\0");
    assert!(SymlinkTarget::decode(&damaged).is_err());
    assert!(SymlinkTarget::new(vec![0]).is_err());
    assert!(SymlinkTarget::new(vec![b'x'; 4097]).is_err());
    let _ = ObjectId::for_bytes(b"unused");
}
