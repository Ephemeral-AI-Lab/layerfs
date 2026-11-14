//! AppleDouble companions decoded into their attributes: a `._<name>` a macOS
//! client writes as a whole image is kept as its FinderInfo and extended
//! attributes, and every read synthesizes the very bytes the client wrote. A
//! body that is not a whole image (a resource fork, another layout, a
//! half-written one) is stored as the client wrote it.
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
use client::{Client, Set, STALE};
use layerfs_nfs::apple_double_codec::{Attribute, Image};
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

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
    let whole = image(
        [0x21; 32],
        &[(b"com.apple.provenance", pattern(40, 40))],
    );
    c.touch(&root, b"old", 0o644).unwrap();
    let old = c.touch(&root, b"._old", 0o644).unwrap();
    c.write(&old.handle, 0, &whole).unwrap();
    assert_eq!(server.held().companions_decoded, 1);
    let decoded_cost = server.held().companion_bytes;
    assert!(decoded_cost < 4096, "the image is counted as its attributes");

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
    assert_eq!(c.read_all(&old.handle, 64 * 1024), whole, "from its spill file");
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

