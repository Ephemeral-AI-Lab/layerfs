//! Independent C1 timing: real database-free work, coarse phases, on/off equality.
//!
//! The operation runs with a recording scope and with timing disabled over the
//! same inputs. Both produce the same root and the same emitted bytes; only the
//! report differs. Phases are coarse and bounded: no trace node per inode.

mod support;

use layerfs_content::filesystem::{
    build_filesystem_timed, update_filesystem_timed, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemResources, FilesystemRoot, FilesystemRootId,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ObjectId, ObjectRole};
use support::filesystem::{synthetic, value, Session, TreeStore};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn regular(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("timing/meta"),
    )
}

fn directory() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("timing/unused"),
        synthetic("timing/dir-meta"),
    )
}

/// One wide tree: `count` files in `/d`, built by a real update.
fn wide_session(count: usize) -> (Session, Vec<u64>) {
    let mut session = Session::new(1).expect("empty");
    let d = session.allocate();
    let serials = (0..count).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![InodeUpdate {
        serial: d,
        value: directory(),
    }];
    let mut new = vec![d];
    for (index, serial) in serials.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: regular(&format!("timing/content-{index:04}")),
        });
        new.push(*serial);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(d))],
        },
        DirectoryUpdate {
            parent: d,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("f{index:04}")), Some(*serial)))
                .collect(),
        },
    ];
    session
        .apply(&directories, &inodes, &new)
        .expect("wide tree");
    (session, serials)
}

#[test]
fn a_recorded_update_reports_coarse_phases_and_changes_nothing() {
    let (session, serials) = wide_session(400);
    let base = session.root;
    let scope = layerfs_content::filesystem::scope_for_seed([0x11; 32]);
    // Update a handful of names and one inode value under a recording scope.
    let directories = [DirectoryUpdate {
        parent: 2,
        changes: serials
            .iter()
            .take(5)
            .enumerate()
            .map(|(index, serial)| (name(&format!("f{index:04}")), Some(*serial)))
            .collect(),
    }];
    let inodes = [InodeUpdate {
        serial: serials[0],
        value: regular("timing/content-changed"),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(base)),
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let recorded = layerfs_telemetry::timer::Timing::record("filesystem.update", |timing| {
        let reader = session.store.clone();
        let mut sink = TreeStore::new();
        let outcome = {
            let phases = FilesystemPhases::new(timing);
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            update_filesystem_timed(&mut objects, &input, None, &phases)
        };
        outcome.map(|result| (result, sink))
    });
    let (result, report) = recorded;
    let (result, sink) = result.expect("recorded update");
    assert!(report.node_count() >= 2, "coarse phases are recorded");
    let names = report
        .root()
        .expect("root node")
        .children()
        .iter()
        .map(|child| child.name().to_owned())
        .collect::<Vec<_>>();
    assert!(names.contains(&"validate".to_owned()));
    assert!(names.contains(&"directories".to_owned()));
    assert!(
        report.node_count() <= 16,
        "a bounded report, not one node per inode: {}",
        report.node_count()
    );

    // The same operation with timing disabled produces the same root and bytes.
    let reader = session.store.clone();
    let mut plain = TreeStore::new();
    let disabled_result = {
        let mut objects = FilesystemObjects::new(&reader, &mut plain);
        layerfs_content::filesystem::update_filesystem(&mut objects, &input, None)
    }
    .expect("disabled update");
    assert_eq!(disabled_result.root, result.root);
    assert_eq!(disabled_result.counters, result.counters);
    let mut recorded_ids = sink
        .order()
        .iter()
        .map(|(id, role)| (*id, role.code()))
        .collect::<Vec<_>>();
    let mut plain_ids = plain
        .order()
        .iter()
        .map(|(id, role)| (*id, role.code()))
        .collect::<Vec<_>>();
    recorded_ids.sort();
    plain_ids.sort();
    assert_eq!(
        recorded_ids, plain_ids,
        "enabled and disabled timing emit the same objects"
    );
}

#[test]
fn a_recorded_construction_reports_its_phases() {
    let scope = layerfs_content::filesystem::scope_for_seed([0x33; 32]);
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: (0..50)
            .map(|index| (name(&format!("e{index:03}")), Some(index as u64 + 2)))
            .collect(),
    }];
    let inodes = (0..51)
        .map(|serial| InodeUpdate {
            serial: serial + 1,
            value: if serial == 0 {
                directory()
            } else {
                regular(&format!("timing/build-{serial:03}"))
            },
        })
        .collect::<Vec<_>>();
    let new_inodes = (1..=51).collect::<Vec<u64>>();
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let (outcome, report) =
        layerfs_telemetry::timer::Timing::record("filesystem.build", |timing| {
            let empty = TreeStore::new();
            let mut sink = TreeStore::new();
            let phases = FilesystemPhases::new(timing);
            let mut objects = FilesystemObjects::new(&empty, &mut sink);
            build_filesystem_timed(&mut objects, &input, None, &phases)
        });
    let result = outcome.expect("recorded build");
    assert!(report.node_count() >= 3);
    let reader = TreeStore::new();
    let mut sink = TreeStore::new();
    let plain = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        layerfs_content::filesystem::build_filesystem(&mut objects, &input, None)
    }
    .expect("disabled build");
    assert_eq!(plain.root, result.root);
    assert!(
        sink.order()
            .iter()
            .any(|(_, role)| *role == ObjectRole::FilesystemRoot),
        "the root is emitted through the boundary"
    );
    assert_eq!(
        FilesystemRoot::decode(sink.canonical(result.root.0).expect("root bytes")).expect("decode"),
        result.value
    );
    let _ = ObjectId::for_bytes(b"timing");
}
