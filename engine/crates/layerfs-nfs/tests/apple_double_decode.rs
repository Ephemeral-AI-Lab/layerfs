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
    assert!(held.companion_decoded_bytes >= 4096);

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

    // A delete and rebuild starts empty and decodes the rewrite again.
    c.remove(&root, b"._seq").unwrap();
    assert_eq!(c.getattr(&ad.handle), Err(STALE));
    let again = c.touch(&root, b"._seq", 0o644).unwrap();
    assert_eq!(again.attr.size, 0);
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

    // An edit that leaves the image not whole is served as written, and a
    // later edit that completes it decodes again: both are the client's own
    // bytes.
    let mut edited = rebuilt.clone();
    edited[24..28].copy_from_slice(b"EDIT");
    c.write(&again.handle, 24, b"EDIT").unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), edited);
    assert_eq!(server.held().companions_decoded, 0);
    c.write(&again.handle, 24, &rebuilt[24..28]).unwrap();
    assert_eq!(c.read_all(&again.handle, 128 * 1024), rebuilt);
    assert_eq!(server.held().companions_decoded, 1);

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
fn a_decoded_companion_is_edited_under_a_full_resident_budget() {
    let base = temp("decoded-budget");
    let tree = base.join("tree");
    source(&tree);
    let mut rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    // A body budget smaller than one encoded image: an edit that leaves the
    // image not whole has to spill rather than grow memory.
    rig.companion_resident_bytes = 1024;
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
