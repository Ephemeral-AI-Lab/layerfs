//! Family6 mixed mutation/custody proofs using the existing production fixture.
use super::*;
use layerfs_bridge::contract::{CommitWire, HistoryQuery, HistoryResult, HISTORY_PROFILE};
use layerfs_workspace::PortableAttributes;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Expected {
    kind: u8,
    mode: u32,
    bytes: Vec<u8>,
    mtime: Option<(i64, u32)>,
}
type Tree = BTreeMap<Vec<u8>, Expected>;

fn base() -> Tree {
    phase_b_namespace::oracle(0, false, 0)
        .into_iter()
        .map(|(path, bytes)| {
            (
                path,
                Expected {
                    kind: if bytes.is_some() { 1 } else { 2 },
                    mode: if bytes.is_some() { 0o644 } else { 0o755 },
                    bytes: bytes.unwrap_or_default(),
                    mtime: None,
                },
            )
        })
        .collect()
}
fn lookup(f: &Fixture, path: &[u8], serials: &mut Vec<u64>) -> u64 {
    let mut serial = f.workspace.root().serial;
    for name in path
        .split(|byte| *byte == b'/')
        .filter(|name| !name.is_empty())
    {
        serial = f.lookup(serial, name).serial;
        serials.push(serial);
    }
    serial
}
fn names(tree: &Tree, parent: &[u8]) -> BTreeSet<Vec<u8>> {
    tree.keys()
        .filter_map(|path| {
            let tail = if parent.is_empty() {
                path.as_slice()
            } else {
                path.strip_prefix(parent)?.strip_prefix(b"/")?
            };
            (!tail.is_empty() && !tail.contains(&b'/')).then(|| tail.to_vec())
        })
        .collect()
}
fn canonical(f: &Fixture, root: Root, tree: &Tree) {
    for (path, wanted) in tree {
        let Response::Attributes {
            serial,
            kind,
            mode,
            mtime,
            nanoseconds,
            ..
        } = f.attributes(root, path)
        else {
            panic!("canonical attributes")
        };
        assert_eq!((kind, mode), (wanted.kind, wanted.mode), "{path:?}");
        if let Some(time) = wanted.mtime {
            assert_eq!((mtime, nanoseconds), time);
        }
        match kind {
            1 => assert_eq!(f.content_bytes(root, path), wanted.bytes),
            2 => {
                let Response::List {
                    entries,
                    continuation: None,
                } = f
                    .inspect(
                        root,
                        Inspect::InodeList {
                            serial,
                            after: Vec::new(),
                            entries: 128,
                            bytes: 16384,
                        },
                    )
                    .unwrap()
                else {
                    panic!("full directory list")
                };
                assert_eq!(
                    entries
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect::<BTreeSet<_>>(),
                    names(tree, path)
                );
            }
            3 => assert_eq!(
                f.inspect(root, Inspect::InodeReadlink { serial }).unwrap(),
                Response::Link(wanted.bytes.clone())
            ),
            _ => panic!("kind"),
        }
    }
    let Response::Attributes {
        serial: file,
        references,
        ..
    } = f.attributes(root, b"work/file")
    else {
        panic!("mixed file")
    };
    if tree.contains_key(b"work/moved".as_slice()) {
        let Response::Attributes { serial: alias, .. } = f.attributes(root, b"work/moved") else {
            panic!("hardlink")
        };
        assert_eq!((file, references), (alias, 2));
    } else {
        assert_eq!(references, 1);
    }
}
fn pinned(f: &Fixture, token: &[u8; 33], tree: &Tree) {
    let mut identities = BTreeMap::new();
    for (path, wanted) in tree {
        let mut serial = f.workspace.root().serial;
        for name in path
            .split(|byte| *byte == b'/')
            .filter(|name| !name.is_empty())
        {
            let value = f
                .workspace
                .view_lookup(token, serial, name, deadline())
                .unwrap();
            serial = value.serial;
            if name == path.rsplit(|byte| *byte == b'/').next().unwrap() {
                assert_eq!(value.mode, wanted.mode);
                if let Some(time) = wanted.mtime {
                    assert_eq!((value.mtime_seconds, value.mtime_nanoseconds), time);
                }
                identities.insert(path.clone(), (value.serial, value.references));
            }
        }
        match wanted.kind {
            1 => assert_eq!(
                f.workspace
                    .view_read(token, serial, 0, wanted.bytes.len() + 1, deadline())
                    .unwrap()
                    .bytes,
                wanted.bytes
            ),
            2 => {
                let page = f
                    .workspace
                    .view_list(token, serial, None, 128, deadline())
                    .unwrap();
                assert!(page.continuation.is_none());
                assert_eq!(
                    page.entries
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect::<BTreeSet<_>>(),
                    names(tree, path)
                );
            }
            3 => assert_eq!(
                f.workspace
                    .view_readlink(token, serial, deadline())
                    .unwrap(),
                wanted.bytes
            ),
            _ => panic!("kind"),
        }
    }
    if tree.contains_key(b"work/moved".as_slice()) {
        assert_eq!(
            identities[b"work/file".as_slice()],
            identities[b"work/moved".as_slice()]
        );
        assert_eq!(identities[b"work/file".as_slice()].1, 2);
    }
}
fn write(f: &Fixture, handle: HandleId, offset: u64, bytes: &[u8]) {
    let payload = f
        .workspace
        .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
        .unwrap();
    f.workspace
        .write_file(handle, offset, &payload, deadline())
        .unwrap();
}
struct Mixed {
    tree: Tree,
    serials: Vec<u64>,
    work: u64,
    file: u64,
    handle: HandleId,
}
fn prepare(f: &Fixture) -> Mixed {
    let top = f.workspace.root().serial;
    let work = f
        .workspace
        .mkdir(top, b"work", 0o755, 0, deadline())
        .unwrap()
        .serial;
    let file = f
        .workspace
        .mknod(work, b"file", 0o644, 0, deadline())
        .unwrap()
        .serial;
    let mut serials = vec![work, file];
    let handle = f.open(file);
    write(f, handle, 0, b"abcdefghij");
    write(f, handle, 2, b"XYZ");
    f.workspace.set_len(file, 6, deadline()).unwrap();
    f.workspace.set_len(file, 10, deadline()).unwrap();
    f.workspace
        .set_attributes(
            file,
            PortableAttributes {
                mode: Some(0o600),
                mtime: Some((-17, 123)),
                ..Default::default()
            },
            deadline(),
        )
        .unwrap();
    serials.push(
        f.workspace
            .link(work, b"alias", file, deadline())
            .unwrap()
            .serial,
    );
    f.workspace
        .rename(
            work,
            b"alias",
            work,
            b"moved",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    serials.push(
        f.workspace
            .symlink(work, b"link", b"file", deadline())
            .unwrap()
            .serial,
    );
    f.workspace
        .rename(
            work,
            b"link",
            work,
            b"link2",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    let empty = f
        .workspace
        .mkdir(work, b"empty", 0o755, 0, deadline())
        .unwrap()
        .serial;
    serials.push(empty);
    f.workspace.rmdir(work, b"empty", deadline()).unwrap();
    let removed = f
        .workspace
        .mknod(work, b"remove", 0o644, 0, deadline())
        .unwrap()
        .serial;
    serials.push(removed);
    let removed_handle = f.open(removed);
    write(f, removed_handle, 0, b"temporary");
    f.workspace.release(removed_handle).unwrap();
    f.workspace.unlink(work, b"remove", deadline()).unwrap();
    let sibling = lookup(f, b"packages/old/subtree/sibling.txt", &mut serials);
    let sibling_handle = f.open(sibling);
    f.workspace.set_len(sibling, 0, deadline()).unwrap();
    write(f, sibling_handle, 0, b"sibling-new");
    f.workspace.release(sibling_handle).unwrap();
    let revision = f.workspace.status().unwrap().revision;
    assert!(matches!(
        f.workspace.rmdir(top, b"work", deadline()),
        Err(WorkspaceError::NotEmpty)
    ));
    assert!(matches!(
        f.workspace.rename(
            top,
            b"work",
            work,
            b"nested",
            RenameFlags::default(),
            deadline()
        ),
        Err(WorkspaceError::InvalidInput)
    ));
    assert!(matches!(
        f.workspace.link(work, b"dirlink", work, deadline()),
        Err(WorkspaceError::Unsupported)
    ));
    assert_eq!(f.workspace.status().unwrap().revision, revision);
    let mut tree = base();
    tree.insert(
        b"work".to_vec(),
        Expected {
            kind: 2,
            mode: 0o755,
            bytes: Vec::new(),
            mtime: None,
        },
    );
    for name in [b"work/file".as_slice(), b"work/moved"] {
        tree.insert(
            name.to_vec(),
            Expected {
                kind: 1,
                mode: 0o600,
                bytes: [b"abXYZf".as_slice(), &[0; 4]].concat(),
                mtime: Some((-17, 123)),
            },
        );
    }
    tree.insert(
        b"work/link2".to_vec(),
        Expected {
            kind: 3,
            mode: 0o777,
            bytes: b"file".to_vec(),
            mtime: None,
        },
    );
    tree.get_mut(b"packages/old/subtree/sibling.txt".as_slice())
        .unwrap()
        .bytes = b"sibling-new".to_vec();
    Mixed {
        tree,
        serials,
        work,
        file,
        handle,
    }
}
fn later(f: &Fixture, mixed: &mut Mixed) {
    write(f, mixed.handle, 0, b"QQ");
    f.workspace
        .set_attributes(
            mixed.file,
            PortableAttributes {
                mtime: Some((-19, 456)),
                ..Default::default()
            },
            deadline(),
        )
        .unwrap();
    f.workspace
        .unlink(mixed.work, b"moved", deadline())
        .unwrap();
    f.workspace
        .rename(
            mixed.work,
            b"link2",
            mixed.work,
            b"link3",
            RenameFlags::default(),
            deadline(),
        )
        .unwrap();
    mixed.tree.remove(b"work/moved".as_slice());
    let link = mixed.tree.remove(b"work/link2".as_slice()).unwrap();
    mixed.tree.insert(b"work/link3".to_vec(), link);
    let file = mixed.tree.get_mut(b"work/file".as_slice()).unwrap();
    file.bytes[..2].copy_from_slice(b"QQ");
    file.mtime = Some((-19, 456));
}
fn known(f: &Fixture) -> CommitWire {
    let CommitOutcomeWire::Committed(commit) = f.workspace.commit(deadline()).unwrap().outcome
    else {
        panic!("known new Commit")
    };
    commit
}
fn finish(f: &Fixture, mixed: Mixed, tokens: &[[u8; 33]]) {
    for token in tokens {
        assert!(matches!(
            f.workspace.release_view(token).unwrap(),
            layerfs_workspace::ViewRelease::Completed
        ));
    }
    f.workspace.release(mixed.handle).unwrap();
    for serial in mixed.serials {
        f.workspace.forget(serial, u64::MAX, ReferenceScope::Local);
    }
    f.workspace.close_clean().unwrap();
    let status = f.workspace.backing_status().unwrap();
    assert!(status.accounting_complete);
    assert_eq!((status.allocated_bytes, status.reserved_bytes), (0, 0));
    println!("PHASE_B_MUTATIONS full_old_g1_g2_pin_tree=true modes_mtimes_links_targets=true cleanup=true");
}

#[test]
#[ignore = "Family6 explicit prepared fixture selection"]
fn mixed_live() {
    let f = Fixture::new();
    let old = [101; 33];
    f.workspace.pin_view(old, deadline()).unwrap();
    let mut mixed = prepare(&f);
    let g1 = mixed.tree.clone();
    let token = [102; 33];
    f.workspace.pin_view(token, deadline()).unwrap();
    f.gate.arm();
    let first = std::thread::scope(|scope| {
        let saving = scope.spawn(|| known(&f));
        f.gate.wait();
        later(&f, &mut mixed);
        f.gate.release();
        saving.join().unwrap()
    });
    assert!(first.parent.is_none());
    canonical(&f, first.root, &g1);
    let second = known(&f);
    assert_eq!(second.parent, Some(first.commit));
    canonical(&f, second.root, &mixed.tree);
    pinned(&f, &old, &base());
    pinned(&f, &token, &g1);
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 2);
    finish(&f, mixed, &[old, token]);
}

fn branch(f: &Fixture) -> layerfs_bridge::contract::BranchSnapshotWire {
    let mut id = [54; 17];
    id[0] = 0x11;
    let response = f
        .server
        .service()
        .handle(
            &f.server.peer().unwrap(),
            &Request {
                id: 96,
                generation: 1,
                store: f.server.store(),
                profile: HISTORY_PROFILE,
                deadline_ms: 10000,
                response_bytes: 16384,
                operation: Operation::HistoryQuery(HistoryQuery::GetBranch { branch: id }),
            },
            &mut io::empty(),
            &mut io::sink(),
        )
        .0
        .unwrap();
    let Response::History(result) = response else {
        panic!("C5 history reply")
    };
    let HistoryResult::BranchSnapshot(snapshot) = *result else {
        panic!("C5 branch snapshot")
    };
    snapshot
}

#[test]
#[ignore = "Family6 explicit prepared fault selection; retained custody"]
fn mixed_failures() {
    for fault in [1, 2] {
        let f = Fixture::new();
        let mixed = prepare(&f);
        let token = [103; 33];
        f.workspace.pin_view(token, deadline()).unwrap();
        f.fault.store(fault, Ordering::Release);
        let WorkspaceError::Commit(failure) = f.workspace.commit(deadline()).unwrap_err() else {
            panic!("Commit custody")
        };
        assert_eq!(
            failure.disposition,
            if fault == 1 {
                layerfs_workspace::CommitFailureDisposition::KnownBeforeCommit
            } else {
                layerfs_workspace::CommitFailureDisposition::Unknown
            }
        );
        assert!(failure.known_outcome.is_none());
        assert!(failure.installed_revision.is_none());
        assert!(f.workspace.status().unwrap().submission.is_some());
        assert!(matches!(
            f.workspace.commit(deadline()),
            Err(WorkspaceError::Busy)
        ));
        assert!(matches!(
            f.workspace.close_clean(),
            Err(WorkspaceError::Busy)
        ));
        assert_eq!(
            f.canonical_calls.load(Ordering::Acquire),
            u64::from(fault == 2)
        );
        let head = branch(&f);
        if fault == 1 {
            assert!(head.branch.head_commit.is_none());
            assert_eq!(head.effective_root, f.genesis);
        } else {
            assert!(head.branch.head_commit.is_some());
            canonical(&f, head.effective_root, &mixed.tree);
        }
        pinned(&f, &token, &mixed.tree);
        assert_eq!(
            f.workspace
                .read(mixed.handle, 0, 11, deadline())
                .unwrap()
                .as_ref(),
            mixed.tree[b"work/file".as_slice()].bytes
        );
        let status = f.workspace.backing_status().unwrap();
        assert!(
            status.accounting_complete && status.allocated_bytes > 0 && status.reserved_bytes > 0
        );
        println!("PHASE_B_MUTATIONS_FAILURE fault={fault} full_selected_pin=true aliases_metadata_targets=true no_replay=true canonical_calls={} allocated={} reserved={} retained=true", f.canonical_calls.load(Ordering::Acquire), status.allocated_bytes, status.reserved_bytes);
        std::mem::forget(f);
    }
}

#[test]
#[ignore = "Family6 explicit prepared mixed local-C5 failure/resume selection"]
fn mixed_local_resume() {
    let f = Fixture::new();
    let mut mixed = prepare(&f);
    let g1 = mixed.tree.clone();
    let token = [104; 33];
    f.workspace.pin_view(token, deadline()).unwrap();
    let stage = f.workspace.stage(deadline()).unwrap();
    later(&f, &mut mixed);
    f.fault.store(3, Ordering::Release);
    let error = std::thread::scope(|scope| {
        scope
            .spawn(|| f.workspace.commit_staged(&stage, deadline()))
            .join()
            .unwrap()
    })
    .unwrap_err();
    let WorkspaceError::Commit(failure) = error else {
        panic!("local completion failure")
    };
    assert_eq!(
        failure.disposition,
        layerfs_workspace::CommitFailureDisposition::KnownCommitLocalFailure
    );
    let Some(CommitOutcomeWire::Committed(ref first)) = failure.known_outcome else {
        panic!("known canonical Commit")
    };
    canonical(&f, first.root, &g1);
    assert!(failure.installed_revision.is_none());
    assert!(f.workspace.status().unwrap().submission.is_some());
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
    f.fault.store(0, Ordering::Release);
    let CommitOutcomeWire::Committed(completed) = f
        .workspace
        .commit_staged(&stage, deadline())
        .unwrap()
        .outcome
    else {
        panic!("same selector completes")
    };
    assert_eq!(completed.commit, first.commit);
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 1);
    let second = known(&f);
    assert_eq!(second.parent, Some(first.commit));
    canonical(&f, second.root, &mixed.tree);
    pinned(&f, &token, &g1);
    assert_eq!(f.canonical_calls.load(Ordering::Acquire), 2);
    println!("PHASE_B_MUTATIONS_RESUME same_selector=true canonical_calls_before_after=1,1 later_g2_known=true");
    finish(&f, mixed, &[token]);
}
