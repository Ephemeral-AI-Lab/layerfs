//! #41: the files a mark sorts through, and the lists it no longer holds.
//! What a sorter cannot hold goes to a file with no name beside the Store.
//! The mark reports the most those files held at one time, removes a name an
//! interrupted mark left, and fails as refused I/O when it may not write
//! there. A Store of many Commits and many packs is marked through the same
//! bounded lists: its history closure in batches, its retained packs as one
//! bit each and its partly dead packs through a sort file.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_content::{FinalizedObject, ObjectId};
use layerfs_history::HistoryError;
use layerfs_persistence::{
    CompactPath, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use support::doctor::Fixture;
use support::marks::{leaf, left_beside, publish_files, publish_packs, retain, small_file};

/// Payload objects one extent page of these Stores names.
const EXTENTS: usize = 120;

/// A Store of `pages` extent pages, each a retained root naming 120 payload
/// objects of its own: the wave after the roots holds every payload object.
/// Returns how many payload objects there are.
fn pages(f: &Fixture, pages: usize) -> usize {
    let files: Vec<_> = (0..(pages * EXTENTS) as u64).map(small_file).collect();
    publish_files(f, &files);
    let leaves: Vec<ObjectId> = files
        .chunks(EXTENTS)
        .map(|group| {
            leaf(
                f,
                &group.iter().map(FinalizedObject::id).collect::<Vec<_>>(),
            )
        })
        .collect();
    retain(&f.handles, &leaves);
    files.len()
}

/// A mark's sort files do not all exist at one time: the identities a wave
/// reached are dropped once they are located, and the located rows once they
/// are walked. The mark reports the most the files held together, which is
/// the disk it needs. One reached payload object is 33 bytes while it waits
/// to be located, 54 bytes while it waits to be walked and 22 bytes once
/// kept, and no more than the first two are held together: adding every
/// file's own largest size instead, as the mark once reported, gives more
/// than this bound.
#[test]
fn the_sort_files_are_counted_as_held_together() {
    const PAGES: usize = 9_200;
    let f = Fixture::new();
    let payload = pages(&f, PAGES) as u64;
    let mark = f.handles.history.mark().unwrap();
    assert_eq!(mark.objects().len() as u64, payload + PAGES as u64);
    assert!(
        mark.scratch_bytes() > 54 * payload / 2,
        "the located rows alone are more than the sorters hold"
    );
    assert!(
        mark.scratch_bytes() <= (33 + 54) * payload,
        "{} bytes for {payload} objects",
        mark.scratch_bytes()
    );
    assert!(left_beside(&f).is_empty(), "no sort file keeps a name");
}

/// Where a sort file needs a name for an instant, a mark that was killed in
/// that instant leaves the name behind. The next mark that sorts through
/// files removes every such name before it creates its own.
#[test]
fn a_name_an_interrupted_mark_left_is_removed() {
    let f = Fixture::new();
    pages(&f, 920);
    let left = [
        ".layerfs-mark-4194000-0",
        ".layerfs-mark-4194000-00000000deadbeef-7",
    ];
    for name in left {
        std::fs::write(f.temp.join(name), [0u8; 4096]).unwrap();
    }
    // A file that is not a sort file stays.
    std::fs::write(f.temp.join(".layerfs-marker"), b"kept").unwrap();
    let mark = f.handles.history.mark().unwrap();
    assert!(mark.scratch_bytes() > 0, "the mark sorted through files");
    assert_eq!(left_beside(&f), vec![".layerfs-marker".to_owned()]);
}

/// A mark that may not create its sort files fails as refused I/O, the class
/// a read-only Store reports, not as a damaged Store. It leaves nothing
/// beside the Store, and a mark that may write again answers as before.
#[test]
fn a_mark_refused_its_sort_files_fails_as_refused_io() {
    let f = Fixture::new();
    pages(&f, 920);
    let before = f.handles.history.mark().unwrap();
    assert!(before.scratch_bytes() > 0, "the mark sorted through files");
    let directory = f.temp.join(".");
    let allow = |mode| {
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    allow(0o555);
    if std::fs::write(f.temp.join("probe"), b"").is_ok() {
        // A superuser is not refused: there is nothing to observe here.
        std::fs::remove_file(f.temp.join("probe")).unwrap();
        allow(0o755);
        return;
    }
    let refused = f.handles.history.mark();
    allow(0o755);
    assert_eq!(refused.unwrap_err(), HistoryError::ContinuityUnavailable);
    assert!(
        left_beside(&f).is_empty(),
        "nothing is left beside the Store"
    );
    let after = f.handles.history.mark().unwrap();
    assert_eq!(after.objects(), before.objects());
    assert_eq!(after.history(), before.history());
    assert_eq!(after.live_heads_digest(), before.live_heads_digest());
}

/// A Store of 20,000 Commits and 20,000 packs, each pack holding one
/// retained and one dead record. The history closure is longer than one of
/// its statements follows; the retained packs are a set of ids; the partly
/// dead packs are more than their sorter holds and come back in pack order,
/// streamed and listed alike. A compaction of this Store rebuilds it from
/// that mark and keeps exactly the retained objects.
#[test]
fn many_commits_and_packs_are_marked_through_bounded_lists() {
    const PACKS: usize = 20_000;
    let f = Fixture::new();
    let files: Vec<_> = (0..2 * PACKS as u64).map(small_file).collect();
    let packs = publish_packs(&f, &files, 2);
    assert_eq!(packs.len(), PACKS);
    let roots: Vec<ObjectId> = files.iter().step_by(2).map(FinalizedObject::id).collect();
    retain(&f.handles, &roots);
    let kept: BTreeSet<ObjectId> = roots.iter().copied().collect();

    let mark = f.handles.history.mark().unwrap();
    assert_eq!(
        mark.history().commits().count(),
        PACKS - 1,
        "every Commit of the Branch is kept"
    );
    assert_eq!(
        mark.objects().objects(),
        kept.iter().copied().collect::<Vec<_>>()
    );
    let objects = mark.objects();
    assert_eq!(objects.retained_packs().len(), PACKS as u64);
    assert_eq!(objects.packs(), packs, "every pack holds a root");
    // The list is read from its file, and may be read again.
    for _ in 0..2 {
        let mut read = Vec::new();
        objects
            .retained_packs()
            .each(&mut |pack| {
                read.push(pack as i64);
                Ok(())
            })
            .unwrap();
        assert_eq!(read, packs);
    }
    assert_eq!(objects.partly_dead_count(), PACKS as u64);
    let mut streamed = Vec::new();
    objects
        .each_partly_dead(&mut |pack, bytes| {
            streamed.push((pack, bytes));
            Ok(())
        })
        .unwrap();
    assert_eq!(streamed, objects.partly_dead_packs());
    assert!(streamed
        .iter()
        .map(|(pack, _)| *pack)
        .eq(packs.iter().copied()));
    assert!(streamed.iter().all(|(_, bytes)| *bytes > 0));
    assert_eq!(
        streamed.iter().map(|(_, bytes)| *bytes).sum::<u64>(),
        objects.waste().dead_in_retained_packs_bytes()
    );
    assert!(left_beside(&f).is_empty(), "no sort file keeps a name");

    let store = f.temp.join("store.sqlite");
    let Fixture { handles, temp, .. } = f;
    let receipt = handles
        .compact(&CompactRequest {
            op_id: [11; 16],
            ..CompactRequest::default()
        })
        .unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let reopened = SqliteHandles::open_writable(
        PersistenceConfig::sqlite(store).with_sqlite_profile(SqlitePersistenceProfile::Normal),
        b"doctor-adversarial",
        [71; 32],
    )
    .unwrap();
    let after = reopened.history.mark().unwrap();
    assert_eq!(
        after.objects().objects(),
        kept.iter().copied().collect::<Vec<_>>(),
        "the rebuilt Store keeps the same objects"
    );
    assert_eq!(after.history().commits().count(), PACKS - 1);
    assert_eq!(after.objects().partly_dead_count(), 0);
    let located: i64 = rusqlite::Connection::open(temp.join("store.sqlite"))
        .unwrap()
        .query_row("SELECT count(*) FROM object_location", [], |row| row.get(0))
        .unwrap();
    assert_eq!(located as usize, kept.len(), "and locates no other");
}
