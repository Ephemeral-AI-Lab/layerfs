//! AppleDouble companions decoded into their main inode's own record: a
//! `._<name>` the client writes is served byte for byte, takes no slot of the
//! index bound, is shared by every name of the inode, follows the inode
//! through RENAME and goes with its last name. A body that is not a whole
//! image (a resource fork, another layout, a half-written one) is stored as
//! the client wrote it, which is the #636 fallback.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/spy.rs"]
mod spy;
#[allow(dead_code)]
#[path = "support/tree.rs"]
mod tree;
use client::{Client, Set, NOENT, STALE};
use layerfs_nfs::apple_double::SERIALS;
use layerfs_nfs::apple_double_codec::{Attribute, Image};
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

const NOSPC: u32 = 28;
const UID: u32 = 501;
const GID: u32 = 20;

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-ad{}-{label}", std::process::id()))
}
fn source(root: &Path) {
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), b"# fixture\nhello nfs\n").unwrap();
    fs::write(root.join("docs/guide.md"), b"guide\n").unwrap();
}
fn pattern(seed: u32, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(seed * 97) as u8)
        .collect()
}
/// The image a macOS client writes for these attributes.
fn image(finder: [u8; 32], attrs: &[(&[u8], Vec<u8>)]) -> Vec<u8> {
    let attrs = attrs
        .iter()
        .map(|(name, value)| Attribute::new(name, value))
        .collect();
    Image::new(finder, 0, attrs)
        .expect("an entry can describe these attributes")
        .encode()
}

#[test]
fn a_whole_image_is_read_back_as_written_from_every_name_of_the_inode() {
    let base = temp("decode");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // What `cp` does: the data file, then its companion with the attributes.
    let main = c.touch(&root, b"f", 0o644).unwrap();
    c.write(&main.handle, 0, b"the data file").unwrap();
    let ad = c.touch(&root, b"._f", 0o644).unwrap();
    let finder = [0x11; 32];
    let first = image(
        finder,
        &[
            (b"com.apple.provenance", pattern(1, 40)),
            (b"com.apple.quarantine", pattern(2, 150)),
            (b"com.apple.FinderInfo", finder.to_vec()),
            (b"user.custom", pattern(3, 4096)),
        ],
    );
    c.write(&ad.handle, 0, &first).unwrap();
    assert_eq!(c.getattr(&ad.handle).unwrap().size, first.len() as u64);
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), first);
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (1, 1, 0),
        "a whole image is decoded and takes no slot"
    );
    // The decoded form is memory the budget sees: its names and values, the
    // 4 KiB one included, are what it is counted as.
    assert!(held.companion_bytes >= 4096);

    // A second name of the same inode reads the same companion.
    c.link(&main.handle, &root, b"g").unwrap();
    let (alias, attr) = c.lookup(&root, b"._g").unwrap();
    assert_eq!(alias, ad.handle, "hard links share one companion");
    assert_eq!(attr.size, first.len() as u64);
    assert_eq!(c.read_all(&alias, 64 * 1024), first);

    // A write under the alias changes the inode's attributes for every name.
    let second = image(
        [0x22; 32],
        &[
            (b"com.apple.provenance", pattern(4, 40)),
            (b"user.custom", pattern(5, 100 * 1024)),
        ],
    );
    c.write(&alias, 0, &second).unwrap();
    assert_eq!(c.read_all(&ad.handle, 256 * 1024), second);

    // RENAME of the main file carries the companion to the new name.
    c.rename(&root, b"f", &root, b"h").unwrap();
    let (renamed, _) = c.lookup(&root, b"._h").unwrap();
    assert_eq!(renamed, ad.handle);
    assert_eq!(c.read_all(&renamed, 256 * 1024), second);

    // The companion goes with the inode's last name, not with one name.
    c.remove(&root, b"h").unwrap();
    assert_eq!(server.held().companions, 1, "g still names the inode");
    assert_eq!(c.read_all(&alias, 256 * 1024), second);
    c.remove(&root, b"g").unwrap();
    assert_eq!(server.held().companions, 0);
    assert_eq!(c.getattr(&alias), Err(STALE));
    assert_eq!(c.lookup(&root, b"._h").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

/// The companions whose bodies the spill directory holds, by serial.
fn spill_serials(dir: &Path) -> Vec<u64> {
    let mut serials: Vec<u64> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    entry.file_type().ok().filter(std::fs::FileType::is_file)?;
                    entry.file_name().to_str()?.parse().ok()
                })
                .collect()
        })
        .unwrap_or_default();
    serials.sort_unstable();
    serials
}

#[test]
fn a_read_racing_a_truncation_is_a_short_read_not_an_io_error() {
    let base = temp("read-truncate");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(base.join("spill"));
    // Smaller than one body, so every read of the body is a `pread` of its file.
    rig.companion_resident_bytes = 1024;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut reader = Client::connect(server.local_addr(), UID, GID);
    let mut writer = Client::connect(server.local_addr(), UID, GID);
    let root = reader.mnt("/");
    reader.touch(&root, b"raced", 0o644).unwrap();
    let ad = reader.touch(&root, b"._raced", 0o644).unwrap();
    let body = pattern(60, 64 * 1024);
    reader.write(&ad.handle, 0, &body).unwrap();
    assert_eq!(server.held().companion_bytes, 0, "the body spilled");

    // A length read under the companion lock and the bytes read outside it can
    // disagree: the truncation below may land in between. The reply is then the
    // bytes the file still holds, a short read, never `NFS3ERR_IO`.
    let empty = Set {
        size: Some(0),
        ..Set::default()
    };
    for round in 0..512 {
        let start = std::sync::Barrier::new(2);
        let (read, truncate) = std::thread::scope(|scope| {
            let read = scope.spawn(|| {
                start.wait();
                reader.read_all(&ad.handle, 64 * 1024)
            });
            let truncate = scope.spawn(|| {
                start.wait();
                writer.setattr(&ad.handle, empty, None)
            });
            (read.join().unwrap(), truncate.join().unwrap())
        });
        truncate.unwrap();
        assert!(
            body.starts_with(&read),
            "round {round}: {} bytes read of the body or its truncation",
            read.len()
        );
        // The next round starts from the whole body again: nothing else runs
        // while it is written back.
        writer.write(&ad.handle, 0, &body).unwrap();
        assert_eq!(
            reader.getattr(&ad.handle).unwrap().size as usize,
            body.len()
        );
    }

    drop((reader, writer));
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn eviction_follows_the_order_of_use_not_the_shape_of_the_index() {
    const HELD: usize = 64;
    let base = temp("use-order");
    let tree = base.join("tree");
    source(&tree);
    let spill = base.join("spill");
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(spill.clone());
    // Room for exactly 64 bodies of 16 bytes, so every body after the 64th
    // takes the room of one other.
    rig.companion_resident_bytes = 16 * HELD;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let body = pattern(50, 16);
    let mut handles = Vec::with_capacity(HELD);
    for n in 0..HELD {
        c.touch(&root, format!("c{n}").as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, format!("._c{n}").as_bytes(), 0o644).unwrap();
        c.write(&ad.handle, 0, &body).unwrap();
        handles.push(ad.handle);
    }
    assert_eq!(server.held().companion_bytes, 16 * HELD);
    assert!(spill_serials(&spill).is_empty(), "nothing is on disk yet");
    // Three bodies are read again, so they are the most recently used ones.
    for n in [0, 1, 2] {
        assert_eq!(c.read_all(&handles[n], 4096), body);
    }
    // Eight more bodies: the eight least recently used ones leave memory, in
    // that order, and the three read again stay.
    for n in HELD..HELD + 8 {
        c.touch(&root, format!("c{n}").as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, format!("._c{n}").as_bytes(), 0o644).unwrap();
        c.write(&ad.handle, 0, &body).unwrap();
    }
    let expected: Vec<u64> = (3..11).map(|n| SERIALS + n).collect();
    assert_eq!(spill_serials(&spill), expected);
    assert_eq!(server.held().companion_bytes, 16 * HELD);
    assert_eq!(server.held().companions, HELD + 8);
    // The evicted bodies still hold their bytes, and the kept ones never left
    // memory.
    for n in [3, 7, 10, 0, 1, 2] {
        assert_eq!(c.read_all(&handles[n], 4096), body, "companion {n}");
    }

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn the_clients_write_sequence_is_served_byte_for_byte() {
    let base = temp("sequence");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    let main = c.touch(&root, b"seq", 0o644).unwrap();
    c.write(&main.handle, 0, b"data").unwrap();
    let ad = c.touch(&root, b"._seq", 0o644).unwrap();
    let whole = image(
        [0x33; 32],
        &[
            (b"com.apple.provenance", pattern(6, 40)),
            (b"user.big", pattern(7, 4096)),
        ],
    );
    // The client writes the header first: a half-written image is served
    // exactly as written and never claimed as a whole one.
    c.write(&ad.handle, 0, &whole[..50]).unwrap();
    assert_eq!(c.getattr(&ad.handle).unwrap().size, 50);
    assert_eq!(c.read_all(&ad.handle, 4096), whole[..50]);
    assert_eq!(server.held().companions_decoded, 0);
    // The rest arrives: the image is whole now and reads synthesize it.
    c.write(&ad.handle, 50, &whole[50..]).unwrap();
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
    assert_eq!(server.held().companions_decoded, 1);
    // The decoded form is memory too: it is counted against the same resident
    // budget as a body kept as written, at what its attributes cost.
    let decoded = Image::compact(&whole).expect("the client's image decodes");
    assert_eq!(server.held().companion_bytes, decoded.cost());

    // A delete and rebuild starts empty and decodes the rewrite again.
    c.remove(&root, b"._seq").unwrap();
    assert_eq!(c.getattr(&ad.handle), Err(STALE));
    let again = c.touch(&root, b"._seq", 0o644).unwrap();
    assert_eq!(again.attr.size, 0);
    assert_eq!(
        server.held().companion_bytes,
        0,
        "the removed companion gave its bytes back"
    );
    assert_eq!(c.read_all(&again.handle, 4096), Vec::<u8>::new());
    let rebuilt = image(
        [0x44; 32],
        &[
            (b"com.apple.provenance", pattern(8, 40)),
            (b"user.other", pattern(9, 64 * 1024)),
        ],
    );
    c.write(&again.handle, 0, &rebuilt).unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), rebuilt);
    let rebuilt_cost = Image::compact(&rebuilt).expect("decodes").cost();
    assert_eq!(server.held().companion_bytes, rebuilt_cost);

    // An edit that leaves the image not whole is served as written, and a
    // later edit that completes it decodes again: both are the client's own
    // bytes.
    let mut edited = rebuilt.clone();
    edited[24..28].copy_from_slice(b"EDIT");
    c.write(&again.handle, 24, b"EDIT").unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), edited);
    assert_eq!(server.held().companions_decoded, 0);
    assert_eq!(
        server.held().companion_bytes,
        edited.len(),
        "a body kept as written counts its length"
    );
    c.write(&again.handle, 24, &rebuilt[24..28]).unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), rebuilt);
    assert_eq!(server.held().companions_decoded, 1);
    assert_eq!(server.held().companion_bytes, rebuilt_cost);

    // Truncating below the image is served as the client's truncation, and
    // resizing back up serves zeros, as on any file.
    let shrink = Set {
        size: Some(64),
        ..Set::default()
    };
    c.setattr(&again.handle, shrink, None).unwrap();
    assert_eq!(c.read_all(&again.handle, 4096), rebuilt[..64]);
    let grow = Set {
        size: Some(rebuilt.len() as u64),
        ..Set::default()
    };
    c.setattr(&again.handle, grow, None).unwrap();
    let mut zeros = rebuilt[..64].to_vec();
    zeros.resize(rebuilt.len(), 0);
    assert_eq!(c.read_all(&again.handle, 128 * 1024), zeros);
    c.write(&again.handle, 0, &rebuilt).unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), rebuilt);

    // The companion never reaches the Workspace.
    let (entries, _) = c.list(&root, 64 * 1024, true);
    assert!(entries.iter().all(|entry| !entry.name.starts_with(b"._")));
    drop(c);
    let released = server.stop();
    assert_eq!(released.companions_decoded, 1);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_resource_fork_keeps_the_stored_as_written_fallback() {
    let base = temp("fork");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // A file with a resource fork: the fork's entry carries 286 bytes of the
    // client's own, which the decoded form does not keep.
    c.touch(&root, b"rf", 0o644).unwrap();
    let ad = c.touch(&root, b"._rf", 0o644).unwrap();
    let plain = image([0x55; 32], &[(b"com.apple.provenance", pattern(10, 40))]);
    let mut forked = plain.clone();
    let fork = pattern(11, 286);
    forked[plain.len() - 286..].copy_from_slice(&fork);
    c.write(&ad.handle, 0, &forked).unwrap();
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), forked);
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (1, 0, 1),
        "a resource fork takes the stored-as-written path and its slot"
    );
    // A companion whose main file the export never saw keeps that path too.
    let _unknown = c.touch(&root, b"._no-main", 0o644).unwrap();
    let unknown = c.lookup(&root, b"._no-main").unwrap().0;
    c.write(&unknown, 0, b"not an image").unwrap();
    assert_eq!(server.held().companions_slotted, 2);

    drop(c);
    let released = server.stop();
    assert_eq!(released.companions_decoded, 0);
    assert_eq!(released.companions_slotted, 2);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_rename_onto_an_unknown_main_keeps_the_resident_budget() {
    let base = temp("rename-budget");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(base.join("spill"));
    // Nearly full: a decoded image of a few hundred bytes and a written body
    // of 700 bytes.
    rig.companion_resident_bytes = 1024;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    c.touch(&root, b"a", 0o644).unwrap();
    let image_name = c.touch(&root, b"._a", 0o644).unwrap();
    let whole = image([0x31; 32], &[(b"user.small", pattern(80, 64))]);
    c.write(&image_name.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions_decoded, 1);
    c.touch(&root, b"b", 0o644).unwrap();
    let other = c.touch(&root, b"._b", 0o644).unwrap();
    c.write(&other.handle, 0, &pattern(81, 700)).unwrap();
    let before = server.held();
    assert!(before.companion_bytes <= 1024, "{}", before.companion_bytes);

    // Onto a name whose main file the export does not know: the decoded body is
    // the client's bytes again, which the budget does not take, so it leaves
    // memory first and the rename still happens.
    c.rename(&root, b"._a", &root, b"._zzz").unwrap();
    let after = server.held();
    assert_eq!(
        after.companion_bytes, 700,
        "only the written body takes the budget: the image left memory"
    );
    assert_eq!(after.companions_decoded, 0);
    let (moved, attr) = c.lookup(&root, b"._zzz").unwrap();
    assert_eq!(attr.size as usize, whole.len());
    assert_eq!(c.read_all(&moved, 64 * 1024), whole);
    assert_eq!(c.lookup(&root, b"._a").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_companion_of_a_main_file_the_name_index_missed_is_kept() {
    let base = temp("orphan-live");
    let tree = base.join("tree");
    source(&tree);
    // The main file is part of the tree the export is initialized from, and
    // this export never looks its name up, so its name index does not hold it:
    // the companion created for it has no main inode, which the export's own
    // index reads as an orphan.
    fs::write(tree.join("live"), b"the data file").unwrap();
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // One slot, so the next CREATE has to reclaim before it can ask.
    rig.companions_held = 1;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let ad = c.touch(&root, b"._live", 0o644).unwrap();
    let whole = image([0x51; 32], &[(b"user.kept", pattern(90, 128))]);
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions_slotted, 1);

    // The bound is full, so the next CREATE reclaims the orphan slots first.
    // The main file is there, so the lookup keeps the companion and binds it to
    // that inode, and the CREATE is then refused.
    c.touch(&root, b"other", 0o644).unwrap();
    assert_eq!(c.touch(&root, b"._other", 0o644).map(|_| ()), Err(NOSPC));
    let held = server.held();
    assert_eq!((held.companions, held.companions_slotted), (1, 1));
    assert_eq!(held.names_refused, 1);
    assert_eq!(c.getattr(&ad.handle).unwrap().size as usize, whole.len());
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole, "attributes kept");

    // It belongs to that inode now: another name of the inode reaches it.
    let live = c.lookup(&root, b"live").unwrap().0;
    c.link(&live, &root, b"alias").unwrap();
    let (shared, attr) = c.lookup(&root, b"._alias").unwrap();
    assert_eq!(shared, ad.handle, "hard links share one companion");
    assert_eq!(attr.size as usize, whole.len());
    assert_eq!(c.read_all(&shared, 64 * 1024), whole);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_companion_bound_back_to_its_main_file_survives_the_next_reclaim() {
    let base = temp("orphan-bound");
    let tree = base.join("tree");
    source(&tree);
    // As in the test above: the main file is part of the tree the export was
    // initialized from, and never looked up, so the companion starts with no
    // main inode and takes the one slot.
    fs::write(tree.join("live"), b"the data file").unwrap();
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companions_held = 1;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let ad = c.touch(&root, b"._live", 0o644).unwrap();
    let whole = image([0x52; 32], &[(b"user.kept", pattern(91, 128))]);
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions_slotted, 1);

    // The bound is full, so this CREATE reclaims first: the lookup finds the
    // main file, the companion becomes that inode's, and the name the visit
    // found is the export's one reference (not a second one it never gives
    // back). The CREATE is then refused.
    c.touch(&root, b"other", 0o644).unwrap();
    let lookups = server.held().lookups;
    assert_eq!(c.touch(&root, b"._other", 0o644).map(|_| ()), Err(NOSPC));
    assert_eq!(server.held().lookups, lookups + 1, "one reference, kept");
    assert_eq!(server.held().companions_slotted, 1);

    // A second reclamation runs without the client ever looking the main file
    // up: the bound companion holds a name in the export's table now, so it is
    // not an orphan, and its attributes stay.
    c.touch(&root, b"other2", 0o644).unwrap();
    let lookups = server.held().lookups;
    assert_eq!(c.touch(&root, b"._other2", 0o644).map(|_| ()), Err(NOSPC));
    assert_eq!(
        server.held().lookups,
        lookups,
        "the live name needs no visit"
    );
    assert_eq!(server.held().companions, 1);
    assert_eq!(server.held().companions_slotted, 1);
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole, "attributes kept");

    // It follows that inode: it goes with the inode's last name.
    c.remove(&root, b"live").unwrap();
    assert_eq!(server.held().companions, 0);
    assert_eq!(c.getattr(&ad.handle), Err(STALE));
    assert_eq!(c.lookup(&root, b"._live").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_write_that_takes_the_last_slot_is_refused_by_the_bound() {
    let base = temp("write-bound");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // Two slots, reached through WRITE and not through CREATE: each companion
    // is created as a whole image, which takes none, and then broken by a write
    // that leaves it not whole, which takes one.
    rig.companions_held = 2;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let mut images = Vec::new();
    for i in 0..3 {
        c.touch(&root, format!("m{i}").as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, format!("._m{i}").as_bytes(), 0o644).unwrap();
        let whole = image(
            [i as u8 + 1; 32],
            &[(b"user.value", pattern(70 + i, 512 + i as usize))],
        );
        c.write(&ad.handle, 0, &whole).unwrap();
        assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
        images.push((ad.handle, whole));
    }
    assert_eq!(server.held().companions, 3);
    assert_eq!(
        server.held().companions_slotted,
        0,
        "no companion takes a slot"
    );
    // Two writes that break an image take the two slots.
    for (taken, (handle, _)) in images.iter().take(2).enumerate() {
        c.write(handle, 24, b"EDIT").unwrap();
        assert_eq!(server.held().companions_slotted, taken + 1);
    }
    // The third would take one more, and the bound has none: the whole write is
    // refused, and no companion loses a byte.
    assert_eq!(c.write(&images[2].0, 24, b"EDIT").map(|_| ()), Err(NOSPC));
    assert_eq!(server.held().companions, 3, "no companion was dropped");
    assert_eq!(server.held().companions_slotted, 2);
    for (handle, whole) in &images {
        assert_eq!(c.getattr(handle).unwrap().size as usize, whole.len());
    }
    assert_eq!(
        c.read_all(&images[2].0, 64 * 1024),
        images[2].1,
        "the refused companion still holds its image"
    );
    for (handle, whole) in images.iter().take(2) {
        let mut broken = whole.clone();
        broken[24..28].copy_from_slice(b"EDIT");
        assert_eq!(c.read_all(handle, 64 * 1024), broken);
    }

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn whole_images_take_no_slot_of_the_index_bound() {
    let base = temp("slots");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // Two slots, and a body budget small enough that raw bodies spill.
    rig.companions_held = 2;
    rig.companion_resident_bytes = 1024;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // More whole images than slots: every one is accepted.
    let mut images = Vec::new();
    for i in 0..5 {
        let main = format!("live-{i}");
        c.touch(&root, main.as_bytes(), 0o644).unwrap();
        let ad = c
            .touch(&root, format!("._{main}").as_bytes(), 0o644)
            .unwrap();
        let bytes = image(
            [0; 32],
            &[(b"user.value", format!("value-{i}").into_bytes())],
        );
        c.write(&ad.handle, 0, &bytes).unwrap();
        images.push((ad.handle, bytes));
    }
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (5, 5, 0)
    );
    assert_eq!(held.names_refused, 0);

    // Bodies that are not whole images do take slots: the bound still answers
    // NOSPC once they fill it, and its rate-limited log is the same.
    for i in 0..2 {
        let main = format!("raw-{i}");
        c.touch(&root, main.as_bytes(), 0o644).unwrap();
        let ad = c
            .touch(&root, format!("._{main}").as_bytes(), 0o644)
            .unwrap();
        c.write(&ad.handle, 0, b"stored as written").unwrap();
    }
    assert_eq!(server.held().companions_slotted, 2);
    c.touch(&root, b"raw-extra", 0o644).unwrap();
    assert_eq!(
        c.touch(&root, b"._raw-extra", 0o644).map(|_| ()),
        Err(NOSPC)
    );
    assert_eq!(server.held().names_refused, 1);
    // Removing a raw companion's main file frees its slot, and a new whole
    // image is then accepted again without taking one.
    c.remove(&root, b"raw-0").unwrap();
    assert_eq!(server.held().companions_slotted, 1);
    c.touch(&root, b"new-image", 0o644).unwrap();
    let ad = c.touch(&root, b"._new-image", 0o644).unwrap();
    let fresh = image([1; 32], &[(b"user.fresh", pattern(20, 64))]);
    c.write(&ad.handle, 0, &fresh).unwrap();
    assert_eq!(server.held().companions_slotted, 1);
    assert_eq!(server.held().names_refused, 1);
    assert_eq!(c.read_all(&ad.handle, 4096), fresh);
    // The refused raw companion now fits.
    c.touch(&root, b"._raw-extra", 0o644).unwrap();
    assert_eq!(server.held().companions_slotted, 2);
    // Every whole image still reads back byte for byte.
    for (handle, bytes) in &images {
        assert_eq!(&c.read_all(handle, 64 * 1024), bytes);
    }

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_decoded_companion_is_gone_once_its_inode_is_silly_renamed() {
    let base = temp("silly");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let main = c.touch(&root, b"open", 0o644).unwrap();
    c.write(&main.handle, 0, b"still open").unwrap();
    let ad = c.touch(&root, b"._open", 0o644).unwrap();
    let bytes = image([2; 32], &[(b"user.a", pattern(12, 32))]);
    c.write(&ad.handle, 0, &bytes).unwrap();
    assert_eq!(server.held().companions_decoded, 1);
    // A client that unlinks a file it keeps open renames it to a silly name.
    let silly: &[u8] = b".nfs.20051025.b896";
    c.rename(&root, b"open", &root, silly).unwrap();
    assert_eq!(server.held().companions, 0, "the last name went");
    assert_eq!(c.getattr(&ad.handle), Err(STALE));
    c.remove(&root, silly).unwrap();
    assert_eq!(c.lookup(&root, silly).map(|_| ()), Err(NOENT));
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_replaced_main_file_takes_its_companion_with_it() {
    let base = temp("replace");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    c.touch(&root, b"dst", 0o644).unwrap();
    c.touch(&root, b"src", 0o644).unwrap();
    let dst = c.touch(&root, b"._dst", 0o644).unwrap();
    c.write(
        &dst.handle,
        0,
        &image([3; 32], &[(b"user.dst", pattern(13, 16))]),
    )
    .unwrap();
    let src = c.touch(&root, b"._src", 0o644).unwrap();
    c.write(
        &src.handle,
        0,
        &image([4; 32], &[(b"user.src", pattern(14, 16))]),
    )
    .unwrap();
    assert_eq!(server.held().companions_decoded, 2);
    // RENAME over the destination detaches it as an unlink would.
    c.rename(&root, b"src", &root, b"dst").unwrap();
    assert_eq!(c.getattr(&dst.handle), Err(STALE));
    assert_eq!(server.held().companions_decoded, 1, "only src's is left");
    // The client moves the source's companion to the destination's name.
    c.rename(&root, b"._src", &root, b"._dst").unwrap();
    let (moved, _) = c.lookup(&root, b"._dst").unwrap();
    assert_eq!(moved, src.handle);
    assert_eq!(
        c.read_all(&moved, 4096),
        image([4; 32], &[(b"user.src", pattern(14, 16))])
    );
    c.remove(&root, b"dst").unwrap();
    assert_eq!(server.held().companions, 0);
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn eviction_takes_the_decoded_body_before_the_written_ones() {
    let base = temp("decoded-evict");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(base.join("spill"));
    // Room for two 2,000-byte bodies and one decoded image of a few hundred
    // bytes.
    rig.companion_resident_bytes = 4096;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // A whole image, decoded: it costs its attributes, not its 4 KiB.
    let whole = image([0x21; 32], &[(b"com.apple.provenance", pattern(40, 40))]);
    c.touch(&root, b"old", 0o644).unwrap();
    let old = c.touch(&root, b"._old", 0o644).unwrap();
    c.write(&old.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions_decoded, 1);
    let decoded_cost = server.held().companion_bytes;
    assert!(
        decoded_cost < 4096,
        "the image is counted as its attributes"
    );

    // Two bodies kept as written fill the budget: the decoded body is the least
    // recently used memory there is, so it is the one spilled, and its file
    // holds the image the client wrote.
    let first = pattern(41, 2000);
    let second = pattern(42, 2000);
    c.touch(&root, b"one", 0o644).unwrap();
    let one = c.touch(&root, b"._one", 0o644).unwrap();
    c.write(&one.handle, 0, &first).unwrap();
    c.touch(&root, b"two", 0o644).unwrap();
    let two = c.touch(&root, b"._two", 0o644).unwrap();
    c.write(&two.handle, 0, &second).unwrap();
    assert_eq!(
        server.held().companions_decoded,
        0,
        "the decoded body was the memory evicted"
    );
    assert_eq!(server.held().companion_bytes, 4000);
    assert_eq!(
        c.read_all(&old.handle, 64 * 1024),
        whole,
        "from its spill file"
    );
    assert_eq!(c.read_all(&one.handle, 64 * 1024), first);
    assert_eq!(c.read_all(&two.handle, 64 * 1024), second);
    assert_eq!(c.getattr(&old.handle).unwrap().size as usize, whole.len());

    drop(c);
    let released = server.stop();
    assert_eq!(released.companions_decoded, 0);
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_decoded_companion_is_edited_under_a_full_resident_budget() {
    let base = temp("decoded-budget");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // A body budget that takes the image's attributes but not its bytes: a
    // decoded image costs `Image::cost`, so it fits where its 8 KiB does not,
    // and an edit that leaves the image not whole has to spill rather than grow
    // memory.
    rig.companion_resident_bytes = 5120;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    let main = c.touch(&root, b"full", 0o644).unwrap();
    c.write(&main.handle, 0, b"the data file").unwrap();
    let ad = c.touch(&root, b"._full", 0o644).unwrap();
    let whole = image(
        [0x77; 32],
        &[
            (b"com.apple.provenance", pattern(30, 40)),
            (b"user.big", pattern(31, 4096)),
        ],
    );
    // Another companion's body stays in memory; truncating a decoded body needs
    // no room for its image, so that body must not be evicted for it.
    c.touch(&root, b"other", 0o644).unwrap();
    let other = c.touch(&root, b"._other", 0o644).unwrap();
    c.write(&other.handle, 0, &pattern(35, 100)).unwrap();
    assert_eq!(server.held().companion_bytes, 100);

    // A whole image costs its attributes, not its bytes: it is decoded.
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
    assert_eq!(server.held().companions_decoded, 1);

    // Truncating a decoded body to nothing is the client's truncation, with no
    // resident room for its image, and a rewrite decodes again.
    let empty = Set {
        size: Some(0),
        ..Set::default()
    };
    c.setattr(&ad.handle, empty, None).unwrap();
    assert_eq!(c.getattr(&ad.handle).unwrap().size, 0);
    assert_eq!(c.read_all(&ad.handle, 4096), Vec::<u8>::new());
    assert_eq!(server.held().companions_decoded, 0);
    assert_eq!(
        server.held().companion_bytes,
        100,
        "the image was never resident, so nothing had to be evicted for it"
    );
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
    assert_eq!(server.held().companions_decoded, 1);

    // An edit that leaves the image not whole cannot grow memory: the decoded
    // body is spilled and the write applied to its file, not refused.
    let mut edited = whole.clone();
    edited[24..28].copy_from_slice(b"EDIT");
    c.write(&ad.handle, 24, b"EDIT").unwrap();
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), edited);
    assert_eq!(server.held().companions_decoded, 0);

    // Truncating the spilled body is the client's truncation.
    c.setattr(&ad.handle, empty, None).unwrap();
    assert_eq!(c.getattr(&ad.handle).unwrap().size, 0);
    assert_eq!(c.read_all(&ad.handle, 4096), Vec::<u8>::new());

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_companion_without_a_main_file_takes_a_slot_and_is_reclaimed() {
    let base = temp("no-main");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companions_held = 3;
    // Too small for the image written below, so that write needs the spill path.
    rig.companion_resident_bytes = 1024;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // A whole image under a name whose main file the export does not know is
    // not decoded: it keeps the client's bytes and a slot of the index bound,
    // both when it fits the resident budget and when it does not.
    let small = image([0x88; 32], &[(b"user.small", pattern(32, 16))]);
    c.touch(&root, b"._small", 0o644).unwrap();
    let fits = c.lookup(&root, b"._small").unwrap().0;
    c.write(&fits, 0, &small).unwrap();
    assert_eq!(c.read_all(&fits, 4096), small);
    let big = image([0x89; 32], &[(b"user.big", pattern(33, 4096))]);
    c.touch(&root, b"._no-main", 0o644).unwrap();
    let spilled = c.lookup(&root, b"._no-main").unwrap().0;
    c.write(&spilled, 0, &big).unwrap();
    assert_eq!(c.read_all(&spilled, 64 * 1024), big);
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (2, 0, 2),
        "a companion with no main file is stored as written and takes its slot"
    );

    // A decoded companion renamed onto a name whose main file is unknown is no
    // longer an inode's: it keeps the bytes as written and takes its slot too.
    let main = c.touch(&root, b"owned", 0o644).unwrap();
    c.write(&main.handle, 0, b"owned by an inode").unwrap();
    let owned = c.touch(&root, b"._owned", 0o644).unwrap();
    c.write(&owned.handle, 0, &small).unwrap();
    assert_eq!(server.held().companions_decoded, 1);
    c.rename(&root, b"._owned", &root, b"._gone").unwrap();
    assert_eq!(c.read_all(&owned.handle, 4096), small);
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (3, 0, 3),
        "a companion with no main file is never a whole image"
    );

    // The bound reclaims them as orphans instead of refusing the next CREATE.
    c.touch(&root, b"next", 0o644).unwrap();
    c.touch(&root, b"._next", 0o644).unwrap();
    let held = server.held();
    assert_eq!(
        (held.companions, held.companions_slotted, held.names_refused),
        (1, 1, 0),
        "the three companions with no main file were reclaimed"
    );
    assert_eq!(c.getattr(&spilled), Err(STALE));
    assert_eq!(c.getattr(&owned.handle), Err(STALE));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_spilled_body_renamed_off_its_inode_takes_its_slot() {
    let base = temp("spilled-rename");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(base.join("spill-rename"));
    rig.companion_resident_bytes = 512 * 1024;
    rig.companions_held = 4;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // A whole image with an attribute larger than the decoded-entry bound is
    // kept as written, takes no slot, and fits the resident budget.
    c.touch(&root, b"m", 0o644).unwrap();
    let ad = c.touch(&root, b"._m", 0o644).unwrap();
    let whole = image(
        [0x5a; 32],
        &[
            (b"com.apple.provenance", pattern(40, 40)),
            (b"user.bulk", pattern(41, 200 * 1024)),
        ],
    );
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(
        (
            server.held().companions_decoded,
            server.held().companions_slotted
        ),
        (0, 0),
        "a whole image takes no slot even when it is kept as written"
    );

    // Another companion's larger body evicts it to disk, still whole.
    c.touch(&root, b"other", 0o644).unwrap();
    let other = c.touch(&root, b"._other", 0o644).unwrap();
    let stored = pattern(42, 400 * 1024);
    c.write(&other.handle, 0, &stored).unwrap();
    assert_eq!(server.held().companion_bytes, stored.len());
    assert_eq!(
        server.held().companions_slotted,
        1,
        "only ._other takes a slot"
    );

    // RENAME onto a name whose main file the export never saw: the body stops
    // being an inode's companion, so it may not keep a whole image's
    // exemption from the index bound, even on disk.
    c.rename(&root, b"._m", &root, b"._gone").unwrap();
    let held = server.held();
    assert_eq!(
        (held.companions_decoded, held.companions_slotted),
        (0, 2),
        "a spilled body renamed off its inode takes its slot"
    );
    assert_eq!(c.read_all(&ad.handle, 256 * 1024), whole);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn whole_images_take_no_slot_and_the_bound_reclaims_only_slots() {
    let base = temp("whole-images-bound");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companions_held = 2;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    // Two companions that decode into whole images: neither takes a slot, so a
    // full table may hold any number of them.
    let mut whole = Vec::new();
    for (main, seed) in [(b"a".as_slice(), 1u32), (b"b".as_slice(), 2u32)] {
        let data = c.touch(&root, main, 0o644).unwrap();
        c.write(&data.handle, 0, b"data").unwrap();
        let mut name = b"._".to_vec();
        name.extend_from_slice(main);
        let ad = c.touch(&root, &name, 0o644).unwrap();
        let bytes = image([seed as u8; 32], &[(b"user.small", pattern(seed, 16))]);
        c.write(&ad.handle, 0, &bytes).unwrap();
        whole.push((ad.handle, bytes));
    }
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted
        ),
        (2, 2, 0),
        "whole images take no slot"
    );

    // Both slots go to companions whose main file the export knows, so there is
    // no orphan to reclaim: the bound refuses instead of touching a whole image.
    for index in 0..2 {
        let main = format!("live-{index}");
        let companion = format!("._live-{index}");
        c.touch(&root, main.as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, companion.as_bytes(), 0o644).unwrap();
        c.write(&ad.handle, 0, &[b'L'; 64]).unwrap();
    }
    c.touch(&root, b"next", 0o644).unwrap();
    assert_eq!(c.touch(&root, b"._next", 0o644).map(|_| ()), Err(NOSPC));
    let held = server.held();
    assert_eq!(
        (
            held.companions,
            held.companions_decoded,
            held.companions_slotted,
            held.names_refused
        ),
        (4, 2, 2, 1),
        "the refusal kept both whole images and both slots"
    );
    for (handle, bytes) in &whole {
        assert_eq!(c.read_all(handle, 4096), *bytes);
    }

    // Freeing a slot, not a whole image, lets the next companion in.
    c.remove(&root, b"._live-0").unwrap();
    c.touch(&root, b"._next", 0o644).unwrap();
    assert_eq!(server.held().companions_slotted, 2);
    assert_eq!(server.held().companions, 4);
    assert_eq!(server.held().companions_decoded, 2);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_rename_keeps_the_other_names_of_the_inode_companion() {
    let base = temp("aliases");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");

    let main = c.touch(&root, b"a", 0o644).unwrap();
    c.write(&main.handle, 0, b"two names, one inode").unwrap();
    let ad = c.touch(&root, b"._a", 0o644).unwrap();
    let bytes = image([0x99; 32], &[(b"user.link", pattern(34, 64))]);
    c.write(&ad.handle, 0, &bytes).unwrap();
    c.link(&main.handle, &root, b"b").unwrap();
    assert_eq!(c.lookup(&root, b"._b").unwrap().0, ad.handle);

    // RENAME moves one name of the inode: every other name still reaches the
    // companion, before and after the moved name goes.
    c.rename(&root, b"a", &root, b"c").unwrap();
    assert_eq!(
        c.lookup(&root, b"._b").unwrap().0,
        ad.handle,
        "b still names the inode"
    );
    assert_eq!(c.lookup(&root, b"._c").unwrap().0, ad.handle);
    assert_eq!(c.read_all(&ad.handle, 4096), bytes);
    c.remove(&root, b"c").unwrap();
    assert_eq!(
        c.lookup(&root, b"._b").unwrap().0,
        ad.handle,
        "b is the inode's last name"
    );
    assert_eq!(c.read_all(&ad.handle, 4096), bytes);

    // The companion goes with the inode's last name.
    c.remove(&root, b"b").unwrap();
    assert_eq!(server.held().companions, 0);
    assert_eq!(c.lookup(&root, b"._b").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn an_eviction_victim_that_cannot_be_written_out_does_not_fail_the_writer() {
    let base = temp("victim-error");
    let tree = base.join("tree");
    source(&tree);
    let spill = base.join("spill");
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companion_spill_dir = Some(spill.clone());
    // Room for two bodies of 1,000 bytes: a third one makes the two oldest
    // give their room back.
    rig.companion_resident_bytes = 2000;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let mut bodies = Vec::new();
    for i in 0..2 {
        c.touch(&root, format!("m{i}").as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, format!("._m{i}").as_bytes(), 0o644).unwrap();
        let body = pattern(60 + i, 1000);
        c.write(&ad.handle, 0, &body).unwrap();
        bodies.push((ad.handle, body));
    }
    assert_eq!(server.held().companion_bytes, 2000);

    // The oldest body's spill file is a directory with a file in it, so no
    // file can be renamed onto it: evicting that body fails.
    let oldest = format!("{SERIALS}");
    fs::create_dir_all(spill.join(&oldest)).unwrap();
    fs::write(spill.join(&oldest).join("kept"), b"not a spill file").unwrap();

    // A third body needs room. The victim that cannot be written out is
    // passed over, the next one gives its room back, and this call's own
    // write lands: the failure belonged to a companion this writer never
    // named.
    c.touch(&root, b"m2", 0o644).unwrap();
    let third = c.touch(&root, b"._m2", 0o644).unwrap();
    let body = pattern(62, 1000);
    c.write(&third.handle, 0, &body).unwrap();
    assert_eq!(c.read_all(&third.handle, 4096), body);
    assert_eq!(
        spill_serials(&spill),
        vec![SERIALS + 1],
        "the victim whose file cannot be written stays in memory"
    );
    assert_eq!(server.held().companion_bytes, 2000);
    // Both other bodies still read back byte for byte: the spilled one from
    // its file, the passed-over one from memory.
    for (handle, want) in &bodies {
        assert_eq!(&c.read_all(handle, 4096), want);
    }

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_hard_link_keeps_the_companion_under_its_other_names() {
    let base = temp("hard-link");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    // What `cp` leaves: the data file and its attributes alongside.
    let a = c.touch(&root, b"a", 0o644).unwrap();
    c.write(&a.handle, 0, b"the data file").unwrap();
    let ad = c.touch(&root, b"._a", 0o644).unwrap();
    let whole = image([0x61; 32], &[(b"user.kept", pattern(11, 64))]);
    c.write(&ad.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions, 1);

    // `ln a b`, and the client drops the `._` name of the old name it no
    // longer uses: the attributes belong to the inode `b` still names, so the
    // companion stays.
    c.link(&a.handle, &root, b"b").unwrap();
    let (shared, attr) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(
        shared, ad.handle,
        "the linked name reads the same companion"
    );
    assert_eq!(attr.size as usize, whole.len());
    c.remove(&root, b"._a").unwrap();
    assert_eq!(server.held().companions, 1);
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole, "attributes kept");

    // `rm a`: one name of the inode goes, the companion stays with `b`.
    c.remove(&root, b"a").unwrap();
    assert_eq!(server.held().companions, 1);
    let (still, _) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(still, ad.handle);
    assert_eq!(c.read_all(&still, 64 * 1024), whole);

    // The inode's last name takes it with it.
    c.remove(&root, b"b").unwrap();
    assert_eq!(server.held().companions, 0);
    assert_eq!(c.lookup(&root, b"._b").map(|_| ()), Err(NOENT));
    assert_eq!(c.getattr(&ad.handle), Err(STALE));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

/// `name` and its `._<name>` companion holding a whole image, as `cp` leaves
/// them: the main file's handle, the companion's, and the bytes written.
fn pair(c: &mut Client, root: &[u8], name: &[u8], seed: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let main = c.touch(root, name, 0o644).unwrap();
    c.write(&main.handle, 0, b"the data file").unwrap();
    let mut paired = b"._".to_vec();
    paired.extend_from_slice(name);
    let ad = c.touch(root, &paired, 0o644).unwrap();
    let whole = image([seed as u8; 32], &[(b"user.kept", pattern(seed, 64))]);
    c.write(&ad.handle, 0, &whole).unwrap();
    (main.handle, ad.handle, whole)
}

#[test]
fn a_removed_name_still_reaches_the_companion_of_its_inode() {
    let base = temp("removed-name");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    // `cp a`, then `ln a b`: one inode behind two names, the attributes under
    // `._a`. This is the matrix's g2 order: `ln`, `rm` of the original before
    // the linked name was read once.
    let (a, ad, whole) = pair(&mut c, &root, b"a", 13);
    c.link(&a, &root, b"b").unwrap();
    c.remove(&root, b"a").unwrap();
    assert_eq!(server.held().companions, 1, "b still names the inode");
    assert_eq!(c.lookup(&root, b"a").map(|_| ()), Err(NOENT));

    // The name is gone, the inode is not: `._a` is still the attributes of the
    // file `b` names. A client that still holds `._a`, or asks for it again
    // once its `actimeo` expired, reads them there instead of finding nothing.
    let (named, attr) = c.lookup(&root, b"._a").unwrap();
    assert_eq!(
        named, ad,
        "a name of the inode that is gone still answers its `._` name"
    );
    assert_eq!(attr.size as usize, whole.len());
    assert_eq!(c.read_all(&named, 64 * 1024), whole);
    assert_eq!(
        server.held().companions_unreachable,
        0,
        "the name key reaches it"
    );
    let (linked, attr) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(linked, ad, "the surviving name answers it too");
    assert_eq!(attr.size as usize, whole.len());
    assert_eq!(c.read_all(&linked, 64 * 1024), whole);
    assert_eq!(server.held().companions, 1);

    drop(c);
    let released = server.stop();
    assert_eq!(released.companions, 1, "the last name stays to the export");
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_name_the_link_read_before_its_removal_keeps_its_companion_name() {
    let base = temp("removed-name-read");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    // The matrix's g3 order: `ln`, one read of the linked name and of its `._`
    // name, then `rm` of the original.
    let (a, ad, whole) = pair(&mut c, &root, b"a", 14);
    c.link(&a, &root, b"b").unwrap();
    let (read, _) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(read, ad, "the linked name reaches the inode's companion");
    assert_eq!(c.lookup(&root, b"b").unwrap().0, a);
    c.remove(&root, b"a").unwrap();

    // Every name of the inode keeps answering, the removed one included.
    let (named, _) = c.lookup(&root, b"._a").unwrap();
    assert_eq!(named, ad);
    assert_eq!(c.read_all(&named, 64 * 1024), whole);
    assert_eq!(c.lookup(&root, b"._b").unwrap().0, ad);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_name_asked_right_after_its_removal_still_reaches_the_companion() {
    let base = temp("removed-name-after");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    // The matrix's g5 order: `ln`, `rm` of the original, the linked name read
    // right after, and then the removed name's `._` name.
    let (a, ad, whole) = pair(&mut c, &root, b"a", 15);
    c.link(&a, &root, b"b").unwrap();
    c.remove(&root, b"a").unwrap();
    let (linked, _) = c.lookup(&root, b"b").unwrap();
    assert_eq!(linked, a, "b is the inode's remaining name");
    let (named, _) = c.lookup(&root, b"._a").unwrap();
    assert_eq!(named, ad);
    assert_eq!(c.read_all(&named, 64 * 1024), whole);
    assert_eq!(c.lookup(&root, b"._b").unwrap().0, ad);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_new_file_under_a_removed_name_does_not_reach_the_old_companion() {
    let base = temp("removed-name-reused");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let (a, ad, whole) = pair(&mut c, &root, b"a", 16);
    c.link(&a, &root, b"b").unwrap();
    c.remove(&root, b"a").unwrap();

    // Another inode takes the freed name. The `._a` name the old companion
    // still holds is the one the inode that had it answers, not this file `a`
    // is now: `Companions::resolve` takes a name key only for the inode the
    // companion belongs to.
    let fresh = c.touch(&root, b"a", 0o644).unwrap();
    c.write(&fresh.handle, 0, b"the new data file").unwrap();
    assert_eq!(
        c.lookup(&root, b"._a").map(|_| ()),
        Err(NOENT),
        "the new file `a` has no attributes yet"
    );
    // The old attributes are still the file `b` names, and the old body is
    // untouched by the file that took the name.
    let (kept, _) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(kept, ad);
    assert_eq!(c.read_all(&kept, 64 * 1024), whole);

    // A companion created under the name is the new inode's: it starts empty
    // and never reads the old body.
    let new_ad = c.touch(&root, b"._a", 0o644).unwrap();
    assert_ne!(new_ad.handle, ad, "another companion for another inode");
    assert_eq!(c.getattr(&new_ad.handle).unwrap().size, 0);
    let fresh_whole = image([0x17; 32], &[(b"user.new", pattern(17, 32))]);
    c.write(&new_ad.handle, 0, &fresh_whole).unwrap();
    assert_eq!(c.read_all(&new_ad.handle, 64 * 1024), fresh_whole);
    assert_eq!(server.held().companions, 2);
    assert_eq!(c.lookup(&root, b"._b").unwrap().0, ad);
    assert_eq!(c.read_all(&ad, 64 * 1024), whole);

    // The old inode's last name takes only the old companion and the `._a`
    // name it held, so the new file's attributes stay.
    c.remove(&root, b"b").unwrap();
    assert_eq!(server.held().companions, 1, "only the new file's companion");
    assert_eq!(c.getattr(&ad), Err(STALE));
    assert_eq!(c.lookup(&root, b"._b").map(|_| ()), Err(NOENT));
    assert_eq!(c.lookup(&root, b"._a").unwrap().0, new_ad.handle);
    assert_eq!(c.read_all(&new_ad.handle, 64 * 1024), fresh_whole);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn the_last_name_of_a_hard_link_reclaims_its_companion_and_every_name_key() {
    let base = temp("removed-name-last");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let (a, ad, whole) = pair(&mut c, &root, b"a", 18);
    c.link(&a, &root, b"b").unwrap();
    c.remove(&root, b"a").unwrap();
    assert_eq!(c.lookup(&root, b"._a").unwrap().0, ad);
    assert_eq!(c.read_all(&ad, 64 * 1024), whole);

    // The inode's last name goes: the companion goes with it, and so does the
    // `._a` name key it kept for the name that went first.
    c.remove(&root, b"b").unwrap();
    assert_eq!(server.held().companions, 0);
    assert_eq!(server.held().companions_unreachable, 0);
    assert_eq!(c.getattr(&ad), Err(STALE));
    assert_eq!(c.lookup(&root, b"._a").map(|_| ()), Err(NOENT));
    assert_eq!(c.lookup(&root, b"._b").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    assert_eq!(
        (released.companions, released.companions_unreachable),
        (0, 0)
    );
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_linked_companion_name_is_another_name_of_the_same_companion() {
    let base = temp("companion-link");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let a = c.touch(&root, b"a", 0o644).unwrap();
    c.write(&a.handle, 0, b"the data file").unwrap();
    let ad = c.touch(&root, b"._a", 0o644).unwrap();
    let whole = image([0x62; 32], &[(b"user.kept", pattern(12, 80))]);
    c.write(&ad.handle, 0, &whole).unwrap();

    // What a client that carries the attributes to the new name does: the
    // main file first, then its companion, one companion behind both names.
    c.link(&a.handle, &root, b"b").unwrap();
    let linked = c.link(&ad.handle, &root, b"._b").unwrap();
    assert_eq!(linked.fileid, ad.attr.fileid);
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
    assert_eq!(server.held().companions, 1);
    // A name already held is not linked again, and a companion is never
    // linked onto a name of the Workspace.
    assert_eq!(c.link(&ad.handle, &root, b"._b").map(|_| ()), Err(17));
    assert_eq!(c.link(&ad.handle, &root, b"plain").map(|_| ()), Err(13));

    // The client drops the old name: the companion keeps the new one and its
    // bytes, and reads through the inode as well.
    c.remove(&root, b"._a").unwrap();
    assert_eq!(server.held().companions, 1);
    let (kept, _) = c.lookup(&root, b"._b").unwrap();
    assert_eq!(kept, ad.handle);
    assert_eq!(c.read_all(&kept, 64 * 1024), whole);
    assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);

    // An inode with one name has only one `._` name: removing it is the client
    // clearing its attributes, which takes the companion.
    let alone = c.touch(&root, b"alone", 0o644).unwrap();
    c.write(&alone.handle, 0, b"data").unwrap();
    let one = c.touch(&root, b"._alone", 0o644).unwrap();
    c.write(&one.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions, 2);
    c.remove(&root, b"._alone").unwrap();
    assert_eq!(server.held().companions, 1);
    assert_eq!(c.getattr(&one.handle), Err(STALE));
    assert_eq!(c.lookup(&root, b"._alone").map(|_| ()), Err(NOENT));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn the_slot_bound_holds_when_writes_claim_the_last_slot_together() {
    const WRITERS: usize = 8;
    let base = temp("slot-race");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // One slot, and a body budget smaller than the images below, so each
    // breaking write spills its body and needs the slot after I/O.
    rig.companions_held = 1;
    rig.companion_resident_bytes = 1024;
    rig.companion_spill_dir = Some(base.join("spill"));
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let address = server.local_addr();
    let mut c = Client::connect(address, UID, GID);
    let root = c.mnt("/");
    let mut images = Vec::new();
    for i in 0..WRITERS {
        c.touch(&root, format!("m{i}").as_bytes(), 0o644).unwrap();
        let ad = c.touch(&root, format!("._m{i}").as_bytes(), 0o644).unwrap();
        // A whole image is 4,096 bytes however small its attributes are: it
        // costs a few dozen bytes of the budget and is broken by one byte of
        // its header, which leaves 4,096 bytes that cannot stay resident here.
        let whole = image(
            [i as u8 + 7; 32],
            &[(b"user.value", pattern(70 + i as u32, 64))],
        );
        assert_eq!(whole.len(), 4096);
        c.write(&ad.handle, 0, &whole).unwrap();
        assert_eq!(c.read_all(&ad.handle, 64 * 1024), whole);
        images.push((ad.handle.clone(), whole));
    }
    assert_eq!(server.held().companions, WRITERS);
    assert_eq!(
        server.held().companions_slotted,
        0,
        "a whole image takes no slot"
    );

    // Every companion is broken at once, each on its own connection: one slot
    // is free, so exactly one may take it. A bound decided before the I/O and
    // taken after it lets more than one through.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(WRITERS + 1));
    let writers: Vec<_> = images
        .iter()
        .map(|(handle, _)| {
            let (barrier, mut writer) = (barrier.clone(), Client::connect(address, UID, GID));
            let handle = handle.clone();
            std::thread::spawn(move || {
                barrier.wait();
                writer.write(&handle, 0, b"X").map(|_| ())
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<Result<(), u32>> = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "one slot, one writer: {results:?}"
    );
    assert_eq!(server.held().companions_slotted, 1, "{results:?}");
    assert_eq!(server.held().companions, WRITERS);
    // The refused ones kept their image, the taken one holds the edit.
    let refused = results.iter().filter(|result| result.is_err()).count();
    assert_eq!(refused, WRITERS - 1);
    for (handle, whole) in &images {
        let bytes = c.read_all(handle, 64 * 1024);
        let mut edited = whole.clone();
        edited[0] = b'X';
        assert!(
            bytes == *whole || bytes == edited,
            "every companion is its image or the edit"
        );
    }

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_companion_of_a_removed_directory_is_reclaimed() {
    let base = temp("orphan-dir-gone");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // One slot, and both companions below take one: a whole image whose main
    // file the export never saw stays as the client wrote it.
    rig.companions_held = 1;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    c.mkdir(&root, b"gone", 0o755).unwrap();
    let gone = c.lookup(&root, b"gone").unwrap().0;
    let held = c.touch(&gone, b"._x", 0o644).unwrap();
    c.write(&held.handle, 0, &pattern(80, 64)).unwrap();
    assert_eq!(server.held().companions_slotted, 1);

    // The client removes the directory: nothing the engine can see is in it,
    // so the removal succeeds and leaves the companion it never listed behind,
    // with a main file whose parent the engine no longer has.
    c.rmdir(&root, b"gone").unwrap();
    assert_eq!(server.held().companions, 1);

    // The next CREATE at the bound reclaims first. The lookup of the vanished
    // main file answers `NFS3ERR_STALE` (its parent is gone), which is as much
    // "there is no such file" as `NFS3ERR_NOENT`, so the slot comes free and
    // this CREATE lands instead of the bound staying full for good.
    let made = c.touch(&root, b"._y", 0o644).unwrap();
    assert_eq!(server.held().companions, 1);
    assert_eq!(server.held().companions_slotted, 1);
    assert_eq!(c.getattr(&held.handle), Err(STALE));
    c.write(&made.handle, 0, &pattern(81, 32)).unwrap();
    assert_eq!(c.read_all(&made.handle, 4096), pattern(81, 32));

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_companion_reached_by_one_of_its_names_is_not_an_orphan() {
    let base = temp("orphan-alias");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    rig.companions_held = 1;
    let server = rig.export(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID, false);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    c.mkdir(&root, b"d", 0o755).unwrap();
    let d = c.lookup(&root, b"d").unwrap().0;
    // Nothing names `ghost`, so this companion starts with no main inode and
    // takes the one slot.
    let ghost = c.touch(&d, b"._ghost", 0o644).unwrap();
    let body = pattern(82, 64);
    c.write(&ghost.handle, 0, &body).unwrap();
    assert_eq!(server.held().companions_slotted, 1);

    // The client links the companion name, then creates the main file of the
    // linked name: one of the companion's names now has a main file the export
    // knows.
    c.link(&ghost.handle, &d, b"._kept").unwrap();
    c.touch(&d, b"kept", 0o644).unwrap();
    assert_eq!(c.read_all(&ghost.handle, 4096), body);

    // A CREATE at the bound reclaims first. The companion is reached by
    // `._kept`, whose main file the export knows, so it is no orphan: nothing
    // is dropped and the CREATE is refused.
    c.touch(&root, b"y", 0o644).unwrap();
    assert_eq!(c.touch(&root, b"._y", 0o644).map(|_| ()), Err(NOSPC));
    assert_eq!(server.held().companions, 1);
    assert_eq!(c.read_all(&ghost.handle, 4096), body);
    let (kept, attr) = c.lookup(&d, b"._kept").unwrap();
    assert_eq!(kept, ghost.handle);
    assert_eq!(attr.size as usize, body.len());

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}
