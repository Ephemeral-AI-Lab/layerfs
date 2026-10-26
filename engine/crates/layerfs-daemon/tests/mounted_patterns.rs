//! Real kernel mount (issue 318, phase B): writes under 8 KiB and stores
//! through a shared mapping, which the kernel writes back from its page cache
//! as page-aligned WRITE requests, over a file that has reference pieces. The
//! pieces come from a process launched outside the daemon that shifts the
//! file's tail. The tree is compared with a kernel replay of the same writes,
//! read again through daemon READs, committed, read from the published root
//! and through a fresh mount. Counts are printed; nothing is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use model::assert_same;
use mounted::COMMAND;
use nix::libc;
use rig::{bash_in, clock_masked, host, passed, root, stamp_tree, Rig};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, OpenOptionsExt},
    },
    path::Path,
};

const BIG: usize = 3 << 20;
const PAGE: usize = 4096;
/// Where the script inserts 4 KiB into `big`; not a cell boundary.
const P: usize = 1_573_641;
const SHIFT: &str = r#"
set -eu
scratch=$(mktemp)
tail -c +$((1573641 + 1)) big > "$scratch"
head -c 4096 /dev/zero | tr '\0' 'i' |
    dd of=big bs=4k oflag=seek_bytes seek=1573641 conv=notrunc status=none
dd if="$scratch" of=big bs=64k oflag=seek_bytes seek=$((1573641 + 4096)) \
   conv=notrunc status=none
rm -f "$scratch"
"#;
/// Six writes of 1 to 8,191 bytes: inside the moved tail, across the edit, and
/// over the file's end.
const SMALL: &str = r#"
set -eu
put() {
    head -c "$2" /dev/zero | tr '\0' "$1" |
        dd of=big bs="$2" iflag=fullblock oflag=seek_bytes seek="$3" conv=notrunc status=none
}
put x 1 $((1573641 + 102400 + 13))
put a 100 $((1573641 + 204800))
put b 4096 $((1573641 + 524288))
put c 8191 $((1573641 + 700001))
put d 100 $((1573641 - 50))
put e 500 $(($(stat -c %s big) - 10))
"#;

fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}
fn tree(source: &Path) {
    fs::write(source.join("big"), noise(BIG, 3)).unwrap();
    fs::write(source.join("small"), b"a small file\n").unwrap();
    stamp_tree(source);
}
fn direct(path: &Path) -> Vec<u8> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}
struct Mapping {
    base: *mut u8,
    length: usize,
}
impl Mapping {
    fn shared(file: &File, length: usize) -> Self {
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(
            base,
            libc::MAP_FAILED,
            "{}",
            std::io::Error::last_os_error()
        );
        Self {
            base: base.cast(),
            length,
        }
    }
    fn load(&self, offset: usize, length: usize) -> Vec<u8> {
        assert!(offset + length <= self.length);
        unsafe { std::slice::from_raw_parts(self.base.add(offset), length).to_vec() }
    }
    fn store(&self, offset: usize, bytes: &[u8]) {
        assert!(offset + bytes.len() <= self.length);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.base.add(offset), bytes.len()) }
    }
    fn sync(&self) {
        let done = unsafe { libc::msync(self.base.cast(), self.length, libc::MS_SYNC) };
        assert_eq!(done, 0, "{}", std::io::Error::last_os_error());
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.base.cast(), self.length) };
    }
}
/// The stores of the mapping case: a few bytes inside a page, bytes across a
/// page boundary, three whole pages of new bytes, and 256 KiB of the moved
/// tail stored again with the bytes it already has.
fn stores(current: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let inside = P + 300_000 + 5;
    let across = (P + 400_000) / PAGE * PAGE - 4;
    let pages = (P + 900_000) / PAGE * PAGE;
    let again = (P + 1_000_000) / PAGE * PAGE;
    vec![
        (inside, b"in-a-page!".to_vec()),
        (across, b"across-a-page-boundary".to_vec()),
        (pages, noise(3 * PAGE, 71)),
        (again, current[again..again + 256 * 1024].to_vec()),
    ]
}

#[test]
fn small_and_page_cache_writes_over_a_file_that_has_pieces() {
    let rig = Rig::built("mounted-patterns", tree);
    let first = rig.mount(1);
    let mount = root(&first);
    assert_same(&rig.base, &host(mount), "the mounted base");

    passed(&bash_in(COMMAND, mount, SHIFT), "shift on the mount");
    let shifted = rig.harness.engine(first.token);
    println!(
        "MOUNTED_PATTERN shift {:?} piece_rows={} payload_cells={} payload_bytes={}",
        rig.store.recognizer_work(),
        shifted.reference_rows,
        shifted.payload_cells,
        shifted.payload_bytes
    );
    assert!(shifted.reference_rows > 0, "{shifted:?}");

    passed(&bash_in(COMMAND, mount, SMALL), "small writes on the mount");
    let small = rig.harness.engine(first.token);
    println!(
        "MOUNTED_PATTERN small piece_rows={} payload_cells={} payload_bytes={}",
        small.reference_rows, small.payload_cells, small.payload_bytes
    );

    // Stores through a shared mapping, written back by msync.
    let path = mount.join("big");
    let length = fs::metadata(&path).unwrap().len() as usize;
    assert_eq!(length, BIG + 4096 + 490);
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let map = Mapping::shared(&handle, length);
    let before = map.load(0, length);
    assert!(before == direct(&path), "mapped read against daemon READs");
    let planned = stores(&before);
    for (offset, bytes) in &planned {
        map.store(*offset, bytes);
    }
    map.sync();
    drop(map);
    drop(handle);
    let mapped = rig.harness.engine(first.token);
    println!(
        "MOUNTED_PATTERN mapped {:?} piece_rows={} payload_cells={} payload_bytes={}",
        rig.store.recognizer_work(),
        mapped.reference_rows,
        mapped.payload_cells,
        mapped.payload_bytes
    );
    assert_eq!(rig.store.recognizer_work().passes.failed, 0);

    // The same writes on a kernel copy.
    let replay = rig.replay();
    passed(&bash_in(COMMAND, &replay, SHIFT), "shift on the copy");
    passed(
        &bash_in(COMMAND, &replay, SMALL),
        "small writes on the copy",
    );
    let copy = OpenOptions::new()
        .write(true)
        .open(replay.join("big"))
        .unwrap();
    for (offset, bytes) in &planned {
        copy.write_all_at(bytes, *offset as u64).unwrap();
    }
    drop(copy);
    let expected_bytes = fs::read(replay.join("big")).unwrap();
    assert!(direct(&path) == expected_bytes, "big by daemon READs");
    assert!(
        fs::read(&path).unwrap() == expected_bytes,
        "big by cached reads"
    );
    let expected = host(mount);
    assert_same(
        &clock_masked(&host(&replay), rig.started),
        &clock_masked(&expected, rig.started),
        "the mount against the kernel replay",
    );

    let record = rig.committed(first.token, "mounted patterns");
    assert_same(&expected, &rig.published(record.root), "the published root");
    assert_same(&expected, &host(mount), "the same mount after install");
    assert!(direct(&path) == expected_bytes, "big after install");
    rig.unmount(&first);

    let fresh = rig.mount(2);
    assert_same(&expected, &host(root(&fresh)), "the fresh mount");
    assert!(
        direct(&root(&fresh).join("big")) == expected_bytes,
        "big on the fresh mount"
    );
    rig.unmount(&fresh);
    rig.finish();
}
