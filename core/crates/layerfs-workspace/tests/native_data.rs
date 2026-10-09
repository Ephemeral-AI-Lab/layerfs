//! READ and READLINK as one owner visit that records nothing, over a real
//! canonical namespace and Overlay. The visit copies the window's local
//! bytes and names the base root of the rest; the reply is then finished
//! over the base only when an inherited byte or an unknown fact needs it.
//! Every answer is compared with the ordinary source-holding read.
mod common;
mod harness;
use common::name;
use harness::{create, resize, write, Bench, T1};
use layerfs_content::filesystem::SymlinkTarget;
use layerfs_overlay::{
    Inode, InodeKind, NativeDecision, NativeMount, StatementKind, StoredCounts, WorkspaceState,
    READ_WINDOW,
};
use layerfs_workspace::{CanonicalCache, CanonicalClient, NativeWindow, Operation, Refusal};
use std::sync::Arc;

const SMALL: &[u8] = b"original\0\xff";
const TARGET: &[u8] = b"../.git/index";

/// A memory-only client over a cache that holds nothing.
fn empty() -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(Arc::new(CanonicalCache::new(
        4 * 1024 * 1024,
    ))))
}
/// The memory-only client of the cache the Workspace's own client fills.
fn resident(b: &Bench) -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(b.cache.clone()))
}
/// Everything a visit could leave behind: the Workspace row, the ownership
/// rows and the transactions begun so far.
fn snapshot(b: &Bench) -> (WorkspaceState, StoredCounts, u64) {
    (
        b.overlay.state(b.route()).unwrap(),
        b.overlay.resources(Some(b.route())).unwrap().counts,
        b.overlay.diagnostics().statements[StatementKind::Begin as usize].executions,
    )
}
/// One kernel lookup count on `serial`, as a LOOKUP reply leaves it.
fn hold(b: &Bench, mount: NativeMount, serial: u64, kind: InodeKind) {
    let inode = Inode {
        serial,
        kind,
        mode: if kind == InodeKind::Symlink {
            0o777
        } else {
            0o644
        },
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    };
    b.overlay
        .observe_native_visit(mount, 1, None, true, |_, _| {
            Ok(NativeDecision::Finished {
                inode: Some(inode),
                value: (),
            })
        })
        .result
        .unwrap();
}
/// One visit as one owner job: it asks the provider nothing and leaves
/// nothing behind.
fn visit(
    b: &Bench,
    client: Arc<CanonicalClient>,
    mount: NativeMount,
    serial: u64,
    offset: u64,
    length: u32,
) -> NativeWindow {
    let visit = b
        .workspace
        .native_data_visit(client, mount, serial, None, offset, length)
        .unwrap();
    assert_eq!(visit.mount(), mount);
    assert_eq!(
        visit.charge(),
        length as usize + (length as usize).div_ceil(8)
    );
    let (demand, before) = (b.demand(), snapshot(b));
    let window = visit.perform(&b.overlay).unwrap();
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    assert_eq!(snapshot(b), before, "an owner visit wrote");
    window
}
/// The reply as the request driver computes it: the base is taken only when
/// the window says it inherits, and a window that does not asks the provider
/// nothing. Returns the reply and whether the base was taken.
fn reply(
    b: &Bench,
    window: NativeWindow,
    serial: u64,
    offset: u64,
    length: u32,
    link: bool,
) -> (Result<Vec<u8>, Refusal>, bool) {
    let inherits = window.inherits(offset, link);
    let whole = window.whole(link).map(<[u8]>::to_vec);
    assert!(whole.is_none() || !inherits);
    let base = b.workspace.base().unwrap();
    let demand = b.demand();
    let answer = window
        .finish(inherits.then_some(&base), serial, offset, length, link)
        .unwrap();
    if !inherits {
        assert_eq!(b.demand(), demand, "a decided window asked the provider");
    }
    if let Some(whole) = whole {
        assert_eq!(answer.as_ref(), Ok(&whole));
    }
    (answer, inherits)
}

#[test]
fn every_window_is_the_ordinary_read_and_takes_the_base_only_for_what_it_inherits() {
    let b = Bench::new("native-data-windows");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let base = b.workspace.base().unwrap();
    let root = base.identity().0.to_bytes();
    for (serial, kind) in [
        (2, InodeKind::File),
        (3, InodeKind::Symlink),
        (4, InodeKind::Directory),
        (8, InodeKind::File),
    ] {
        hold(&b, mount, serial, kind);
    }
    let large = b.fixture.bytes.clone();
    let end = large.len() as u64;

    // Nothing resident and no local row: the visit names the base root and
    // knows nothing else, so the one base read answers everything.
    let window = visit(&b, empty(), mount, 8, 100, 50);
    assert_eq!((window.root, window.local.is_none()), (root, true));
    assert_eq!((window.base, window.length), (None, None));
    assert!(window.whole(false).is_none());
    // A window that inherits cannot be finished without its base.
    assert!(window.clone().finish(None, 8, 100, 50, false).is_err());
    assert_eq!(
        reply(&b, window, 8, 100, 50, false),
        (Ok(large[100..150].to_vec()), true)
    );
    let window = visit(&b, empty(), mount, 8, end, 50);
    assert_eq!(reply(&b, window, 8, end, 50, false), (Ok(Vec::new()), true));
    // Kind refusals are found by the same base read.
    for (serial, link, refusal) in [
        (3, false, Refusal::Invalid),
        (4, false, Refusal::IsDirectory),
        (4, true, Refusal::Invalid),
        (8, true, Refusal::Invalid),
    ] {
        let window = visit(&b, empty(), mount, serial, 0, 4096);
        assert_eq!(
            reply(&b, window, serial, 0, 4096, link),
            (Err(refusal), true),
            "{serial} {link}"
        );
    }
    let window = visit(&b, empty(), mount, 3, 0, 4096);
    assert_eq!(
        reply(&b, window, 3, 0, 4096, true),
        (Ok(TARGET.to_vec()), true)
    );
    // An inode the kernel holds that neither the base nor a local row has.
    hold(&b, mount, 99, InodeKind::File);
    let window = visit(&b, empty(), mount, 99, 0, 4096);
    assert_eq!(
        reply(&b, window, 99, 0, 4096, false),
        (Err(Refusal::Missing), true)
    );

    // Once the daemon has seen the files, memory answers the inode and the
    // length inside the job: a window at or beyond the end, and a refusal
    // by kind, take no base at all.
    for serial in [2, 8] {
        base.stat(serial).unwrap();
    }
    let window = visit(&b, resident(&b), mount, 8, 0, 4096);
    assert_eq!((window.root, window.length), (root, Some(end)));
    assert!(window.base.is_some() && window.local.is_none());
    assert_eq!(
        reply(&b, window, 8, 0, 4096, false),
        (Ok(large[..4096].to_vec()), true)
    );
    for offset in [end, end + 1, end + 500_000] {
        let window = visit(&b, resident(&b), mount, 8, offset, 4096);
        assert_eq!(
            reply(&b, window, 8, offset, 4096, false),
            (Ok(Vec::new()), false),
            "{offset}"
        );
    }
    let tail = end - 10;
    let window = visit(&b, resident(&b), mount, 8, tail, 4096);
    assert_eq!(
        reply(&b, window, 8, tail, 4096, false),
        (Ok(large[large.len() - 10..].to_vec()), true)
    );
    let window = visit(&b, resident(&b), mount, 2, 0, 4096);
    assert_eq!(
        reply(&b, window, 2, 0, 4096, false),
        (Ok(SMALL.to_vec()), true)
    );
    for (serial, link, refusal) in [
        (3, false, Refusal::Invalid),
        (4, false, Refusal::IsDirectory),
        (4, true, Refusal::Invalid),
        (8, true, Refusal::Invalid),
    ] {
        let window = visit(&b, resident(&b), mount, serial, 0, 4096);
        assert_eq!(
            reply(&b, window, serial, 0, 4096, link),
            (Err(refusal), false),
            "{serial} {link}"
        );
    }
    let window = visit(&b, resident(&b), mount, 3, 0, 4096);
    assert_eq!(
        reply(&b, window, 3, 0, 4096, true),
        (Ok(TARGET.to_vec()), true)
    );

    // Local rows: a file created here, a base file partly overwritten, a
    // base file cut short and a local symlink.
    let created = b.applied(create(1, "new"), T1).unwrap().serial;
    b.applied(write(created, 0, b"entirely local bytes"), T1);
    b.applied(write(8, 5000, b"LOCAL-BYTES"), T1);
    b.applied(resize(2, 4), T1);
    let link = b
        .applied(
            Operation::Symlink {
                parent: 1,
                name: name("link"),
                target: SymlinkTarget::new(b"new".to_vec()).unwrap(),
            },
            T1,
        )
        .unwrap()
        .serial;
    hold(&b, mount, created, InodeKind::File);
    hold(&b, mount, link, InodeKind::Symlink);
    let window = READ_WINDOW as u32;
    for client in [empty(), resident(&b)] {
        for (serial, offset, length, inherits) in [
            // Decided by the visit: the local file, the cut file and windows
            // at or beyond a local row's size.
            (created, 0, window, false),
            (created, 9, 5, false),
            (created, 20, 5, false),
            (2, 0, window, true),
            (2, 4, 1, false),
            (8, end, 5, false),
            // The overwritten span alone is local; around it is inherited.
            (8, 5000, 11, false),
            (8, 4990, 30, true),
            (8, 0, window, true),
            (8, end - 100, window, true),
        ] {
            let found = visit(&b, client.clone(), mount, serial, offset, length);
            assert_eq!(found.root, root);
            assert!(found.local.is_some() && found.base.is_none());
            let (answer, took) = reply(&b, found, serial, offset, length, false);
            assert_eq!(
                answer,
                Ok(b.read_at(serial, offset, length)),
                "{serial} {offset}"
            );
            assert_eq!(took, inherits, "{serial} {offset} {length}");
        }
        let found = visit(&b, client.clone(), mount, link, 0, 4096);
        assert_eq!(
            reply(&b, found, link, 0, 4096, true),
            (Ok(b"new".to_vec()), false)
        );
        // By kind, from the local row.
        for (serial, as_link, refusal) in [
            (link, false, Refusal::Invalid),
            (created, true, Refusal::Invalid),
            (1, false, Refusal::IsDirectory),
        ] {
            let found = visit(&b, client.clone(), mount, serial, 0, 4096);
            assert_eq!(
                reply(&b, found, serial, 0, 4096, as_link),
                (Err(refusal), false),
                "{serial} {as_link}"
            );
        }
    }
    let mut expect = large.clone();
    expect[5000..5011].copy_from_slice(b"LOCAL-BYTES");
    assert_eq!(b.content(8), expect);
    assert_eq!(b.content(2), SMALL[..4]);
}

#[test]
fn a_visit_is_refused_for_another_workspace_and_fenced_by_the_kernel_reference() {
    let b = Bench::new("native-data-fence");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    // The root is held by the mount itself; nothing else is yet.
    let window = visit(&b, empty(), mount, 1, 0, 4096);
    assert!(window.local.is_none() && window.base.is_none());
    let job = b
        .workspace
        .native_data_visit(empty(), mount, 8, None, 0, 4096)
        .unwrap();
    assert!(matches!(
        job.perform(&b.overlay),
        Err(layerfs_overlay::OverlayError::Stale)
    ));
    // A mount of another Workspace is refused before any job is made.
    let other = b.overlay.open_workspace([61; 32], [62; 32]).unwrap();
    let elsewhere = b.overlay.create_native_mount(other, 1).unwrap();
    assert!(b
        .workspace
        .native_data_visit(empty(), elsewhere, 1, None, 0, 4096)
        .is_err());
    // After revocation every visit of the mount is stale.
    hold(&b, mount, 8, InodeKind::File);
    visit(&b, empty(), mount, 8, 0, 4096);
    b.overlay.revoke_native_mount(mount).unwrap();
    let job = b
        .workspace
        .native_data_visit(empty(), mount, 8, None, 0, 4096)
        .unwrap();
    assert!(matches!(
        job.perform(&b.overlay),
        Err(layerfs_overlay::OverlayError::Stale)
    ));
}
