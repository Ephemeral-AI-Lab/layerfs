//! #510: a pinned file reads byte ranges of the version it opened, whole
//! or as pieces (a range across two pieces is two requests), and a file
//! replaced since is `Changed`, never mixed bytes.
use layerfs_remote_storage::local_folder::LocalFolder;
use layerfs_remote_storage::pieces;
use layerfs_remote_storage::time::{http_date, http_date_text};
use layerfs_remote_storage::{Condition, Driver, Limits, Metadata, PinnedFile, RemoteError, Span};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch(tag: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "pinned-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("remote")).unwrap();
    path
}

fn folder(root: &Path, piece_size: u64) -> LocalFolder {
    let limits = Limits {
        max_file: piece_size,
        max_request: piece_size,
        piece_size,
    };
    LocalFolder::new(&root.join("remote"), limits).unwrap()
}

fn bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| ((i * 7919) as u8).wrapping_add(seed))
        .collect()
}

fn upload(root: &Path, remote: &LocalFolder, name: &str, data: &[u8]) {
    let source = root.join(format!("up-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::write(&source, data).unwrap();
    pieces::upload(remote, name, &source, &Metadata::new()).unwrap();
}

fn read(file: &PinnedFile, offset: u64, length: usize) -> Result<Vec<u8>, RemoteError> {
    let mut out = vec![0; length];
    file.read(offset, &mut out)?;
    Ok(out)
}

#[test]
fn a_whole_file_reads_its_version_and_refuses_a_replacement() {
    let root = scratch("whole");
    let data = bytes(10_000, 1);
    upload(&root, &folder(&root, 1 << 20), "s/store.sqlite", &data);
    let file = PinnedFile::open(Box::new(folder(&root, 1 << 20)), "s/store.sqlite").unwrap();
    assert_eq!(file.bytes(), 10_000);
    assert_eq!(file.requests(), 1);
    assert_eq!(read(&file, 1234, 4000).unwrap(), &data[1234..5234]);
    assert_eq!((file.requests(), file.fetched()), (2, 4000));
    // beyond the end is refused before any request
    assert!(matches!(
        read(&file, 9000, 2000),
        Err(RemoteError::Invalid(_))
    ));
    assert_eq!(file.requests(), 2);

    std::thread::sleep(Duration::from_millis(5));
    upload(
        &root,
        &folder(&root, 1 << 20),
        "s/store.sqlite",
        &bytes(10_000, 2),
    );
    assert_eq!(read(&file, 0, 100), Err(RemoteError::Changed));
}

#[test]
fn pieces_map_each_range_and_pin_each_piece() {
    let root = scratch("pieces");
    let piece = 4096;
    let data = bytes(3 * 4096 + 500, 3);
    upload(&root, &folder(&root, piece), "s/store.sqlite", &data);
    let file = PinnedFile::open(Box::new(folder(&root, piece)), "s/store.sqlite").unwrap();
    // the stat and the manifest
    assert_eq!(file.requests(), 2);
    assert_eq!(file.bytes(), data.len() as u64);
    // within one piece: one request
    assert_eq!(read(&file, 100, 200).unwrap(), &data[100..300]);
    assert_eq!(file.requests(), 3);
    // across two pieces: two requests; to the end of the last piece
    assert_eq!(read(&file, 4000, 300).unwrap(), &data[4000..4300]);
    assert_eq!(file.requests(), 5);
    let tail = data.len() as u64 - 600;
    assert_eq!(read(&file, tail, 600).unwrap(), &data[tail as usize..]);

    // A later upload replaces the pieces: a piece already read is pinned by
    // its tag, one never read by the manifest's time.
    upload(
        &root,
        &folder(&root, piece),
        "s/store.sqlite",
        &bytes(data.len(), 4),
    );
    let later = SystemTime::now() + Duration::from_secs(5);
    let names = (0..4).map(|index| pieces::piece_name("s/store.sqlite", index));
    // the new upload's pieces and manifest, written well after the old manifest
    for name in names.chain(["s/store.sqlite".to_owned()]) {
        let path = root.join("remote/objects").join(name);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(later)
            .unwrap();
    }
    assert_eq!(read(&file, 0, 10), Err(RemoteError::Changed));
    let fresh = PinnedFile::open(Box::new(folder(&root, piece)), "s/store.sqlite").unwrap();
    assert_eq!(read(&fresh, 0, 10).unwrap(), &bytes(data.len(), 4)[..10]);
}

#[test]
fn conditions_and_dates() {
    let root = scratch("conditions");
    let remote = folder(&root, 1 << 20);
    upload(&root, &remote, "f", &bytes(100, 5));
    let stat = remote.stat("f").unwrap().unwrap();
    let tag = stat.etag.clone().unwrap();
    let span = Span {
        offset: 10,
        length: 20,
    };
    let mut out = Vec::new();
    let read = remote
        .download_range("f", span, &Condition::Match(tag.clone()), &mut out)
        .unwrap();
    assert_eq!((read.bytes, read.etag), (20, Some(tag)));
    assert_eq!(out, &bytes(100, 5)[10..30]);
    let stale = Condition::Match("\"other\"".to_owned());
    assert_eq!(
        remote.download_range("f", span, &stale, &mut Vec::new()),
        Err(RemoteError::Changed)
    );
    let before = Condition::UnmodifiedSince(stat.modified.unwrap() - 2000);
    assert_eq!(
        remote.download_range("f", span, &before, &mut Vec::new()),
        Err(RemoteError::Changed)
    );
    assert_eq!(
        remote.download_range("missing", span, &stale, &mut Vec::new()),
        Err(RemoteError::Missing)
    );
    // HTTP dates round-trip to the second
    for millis in [0, 1_445_412_480_000, 1_793_000_000_999, 951_782_400_000] {
        assert_eq!(
            http_date(&http_date_text(millis)),
            Some(millis / 1000 * 1000)
        );
    }
    assert_eq!(
        http_date_text(1_445_412_480_000),
        "Wed, 21 Oct 2015 07:28:00 GMT"
    );
    assert_eq!(
        http_date_text(951_782_400_000),
        "Tue, 29 Feb 2000 00:00:00 GMT"
    );
}
