//! External actual descriptor/fixed SQL image and independent small alias-root encoder.
use layerfs_bridge::contract::Root;
use layerfs_content::ObjectId;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Debug)]
pub struct Image {
    pub header: Vec<u8>,
    pub site: Vec<u8>,
    pub graph: Vec<u8>,
    pub solver: Vec<u8>,
    pub scalars: Vec<u64>,
}
fn unhex(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0);
    s.as_bytes()
        .chunks(2)
        .map(|p| {
            let d = |b: u8| match b {
                b'0'..=b'9' => b - b'0',
                b'A'..=b'F' => b - b'A' + 10,
                _ => panic!("hex"),
            };
            d(p[0]) * 16 + d(p[1])
        })
        .collect()
}
fn canonical_owned(path: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let before = std::fs::symlink_metadata(path).unwrap();
    assert!(
        before.is_file() && !before.file_type().is_symlink(),
        "actualownedregularfile"
    );
    let canonical = std::fs::canonicalize(path).unwrap();
    let after = std::fs::symlink_metadata(&canonical).unwrap();
    assert_eq!(
        (before.dev(), before.ino()),
        (after.dev(), after.ino()),
        "canonicalpathmustpreservesameselectedinode"
    );
    canonical
}
pub fn image(library: &Path, path: &Path) -> Image {
    let selected = canonical_owned(path);
    let output = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scratch_reuse_query.py"))
        .arg(library)
        .arg(&selected)
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let values: Vec<_> = text.lines().collect();
    assert_eq!(values.len(), 18);
    Image {
        header: unhex(values[0]),
        site: unhex(values[1]),
        graph: unhex(values[2]),
        solver: unhex(values[3]),
        scalars: values[4..].iter().map(|n| n.parse().unwrap()).collect(),
    }
}
pub fn descriptors(path: &Path) -> BTreeSet<i32> {
    use std::os::unix::fs::MetadataExt;
    let expected = std::fs::metadata(path).unwrap();
    let directory = if cfg!(target_os = "linux") {
        PathBuf::from("/proc/self/fd")
    } else {
        PathBuf::from("/dev/fd")
    };
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let Some(fd) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        // Darwin /dev/fd entries belong to fdescfs, so their path metadata's
        // device does not identify the underlying file. Observe the existing
        // descriptor itself; never open or duplicate the selected scratch file.
        let mut metadata = std::mem::MaybeUninit::<nix::libc::stat>::uninit();
        // SAFETY: fstat only reads fd and writes the valid stat-sized storage.
        // A descriptor closed since enumeration returns an error and is skipped.
        if unsafe { nix::libc::fstat(fd, metadata.as_mut_ptr()) } == 0 {
            // SAFETY: successful fstat initialized the complete stat structure.
            let metadata = unsafe { metadata.assume_init() };
            if u64::try_from(metadata.st_dev).ok() == Some(expected.dev())
                && metadata.st_ino == expected.ino()
            {
                found.insert(fd);
            }
        }
    }
    assert!(!found.is_empty(), "real owned scratch descriptor visible");
    found
}
fn object(value: &[u8]) -> Root {
    let mut bytes = b"LFSO\x01".to_vec();
    bytes.extend_from_slice(&(value.len() as u32 + 4).to_be_bytes());
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
    ObjectId::for_bytes(&bytes).to_bytes()
}
fn leaf(magic: &[u8; 8], role: u8, count: usize, rows: &[u8]) -> Root {
    let mut value = magic.to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(&[role, 0, 0]);
    value.extend_from_slice(&(count as u16).to_be_bytes());
    value.extend_from_slice(&(count as u64).to_be_bytes());
    value.extend_from_slice(&(rows.len() as u64).to_be_bytes());
    value.extend_from_slice(rows);
    object(&value)
}
/// Independent single-leaf v1 encoder with one unique file and repeated bindings.
/// Only ObjectId hashing is shared; no C1 page/builder/candidate supplies expected bytes.
pub fn alias_root(
    scope: Root,
    root_serial: u64,
    root_metadata: Root,
    file_serial: u64,
    file_content: Root,
    file_metadata: Root,
    names: &[Vec<u8>],
) -> Root {
    assert!(names.len() < 50 && root_serial < file_serial);
    assert!(names.windows(2).all(|p| p[0] < p[1]));
    let mut rows = Vec::new();
    for name in names {
        rows.extend_from_slice(&(name.len() as u16).to_be_bytes());
        rows.extend_from_slice(name);
        rows.extend_from_slice(&file_serial.to_be_bytes());
    }
    let directory = leaf(b"LFS6NSP\0", 1, names.len(), &rows);
    let mut inodes = Vec::new();
    for (serial, kind, refs, content, metadata) in [
        (root_serial, 2, 0, directory, root_metadata),
        (
            file_serial,
            1,
            names.len() as u64,
            file_content,
            file_metadata,
        ),
    ] {
        inodes.extend_from_slice(&serial.to_be_bytes());
        inodes.push(kind);
        inodes.extend_from_slice(&refs.to_be_bytes());
        inodes.extend_from_slice(&content);
        inodes.extend_from_slice(&metadata);
    }
    let table = leaf(b"LFS6INT\0", 7, 2, &inodes);
    let profile=ObjectId::for_bytes(b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5");
    let mut root = b"LFS6FSR\0".to_vec();
    root.extend_from_slice(&1u16.to_be_bytes());
    root.extend_from_slice(&[6, 0]);
    root.extend_from_slice(profile.as_bytes());
    root.extend_from_slice(&scope);
    root.extend_from_slice(&root_serial.to_be_bytes());
    root.extend_from_slice(&table);
    object(&root)
}

/// External selected-library Store SHARED barrier, without a product fault hook.
pub struct StoreReader {
    child: Option<std::process::Child>,
    control: std::os::unix::net::UnixStream,
    directory: PathBuf,
}
impl StoreReader {
    pub fn acquire(library: &Path, database: &Path) -> Self {
        let selected = canonical_owned(database);
        use nix::poll::{poll, PollFd, PollFlags};
        use std::{
            io::Read,
            os::{fd::AsFd, unix::net::UnixListener},
            process::Stdio,
            sync::atomic::{AtomicU64, Ordering},
            time::Duration,
        };
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "lfsp-reader-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let socket = directory.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = Command::new("python3")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scratch_reuse_reader.py"),
            )
            .arg(library)
            .arg(&selected)
            .arg(&socket)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pollfd = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
        assert_eq!(poll(&mut pollfd, 5000u16).unwrap(), 1);
        let mut control = listener.accept().unwrap().0;
        // Darwin inherits the listener flag; use the declared bounded reads.
        control.set_nonblocking(false).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ack = [0];
        control.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [1]);
        let mut bytes = [0; 4];
        control.read_exact(&mut bytes).unwrap();
        let count = u32::from_be_bytes(bytes) as usize;
        assert!(count <= 8192);
        let mut info = vec![0; count];
        control.read_exact(&mut info).unwrap();
        assert!(String::from_utf8(info)
            .unwrap()
            .contains("activeSave=1 SHARED held"));
        Self {
            child: Some(child),
            control,
            directory,
        }
    }
    pub fn release(&mut self) {
        use std::io::{Read, Write};
        self.control.write_all(&[2]).unwrap();
        let mut ack = [0];
        self.control.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [3]);
        let output = self.child.take().unwrap().wait_with_output().unwrap();
        assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
impl Drop for StoreReader {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
