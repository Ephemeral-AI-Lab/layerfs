//! The folder driver and the pieces layout: whole files, ranged reads,
//! pieces with a manifest, verification and stale-piece cleanup.
use layerfs_remote_storage::local_folder::LocalFolder;
use layerfs_remote_storage::pieces::{self, piece_name, Manifest, Piece};
use layerfs_remote_storage::{Driver, Limits, Metadata, RemoteError, Span};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch(tag: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "remote-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("remote")).unwrap();
    path
}

fn bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn folder(root: &std::path::Path, max_file: u64, piece_size: u64) -> LocalFolder {
    let limits = Limits {
        max_file,
        max_request: max_file,
        piece_size,
    };
    LocalFolder::new(&root.join("remote"), limits).unwrap()
}

fn download(
    driver: &dyn Driver,
    name: &str,
    root: &std::path::Path,
) -> Result<Vec<u8>, RemoteError> {
    let path = root.join(format!("down-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut out = fs::File::create(&path).unwrap();
    pieces::download(driver, name, &mut out)?;
    Ok(fs::read(path).unwrap())
}

#[test]
fn folder_driver_whole_ranged_stat_delete() {
    let root = scratch("driver");
    let remote = folder(&root, 1 << 20, 1 << 20);
    let data = bytes(5000, 7);
    fs::write(root.join("file"), &data).unwrap();
    let metadata = Metadata::from([("purpose".to_owned(), "test".to_owned())]);
    remote
        .upload_file("a/b.bin", &root.join("file"), None, &metadata)
        .unwrap();
    let stat = remote.stat("a/b.bin").unwrap().unwrap();
    assert_eq!(stat.bytes, 5000);
    assert_eq!(stat.metadata, metadata);
    let mut whole = Vec::new();
    assert_eq!(
        remote.download_file("a/b.bin", None, &mut whole).unwrap(),
        5000
    );
    assert_eq!(whole, data);
    let mut part = Vec::new();
    let span = Span {
        offset: 1000,
        length: 123,
    };
    remote
        .download_file("a/b.bin", Some(span), &mut part)
        .unwrap();
    assert_eq!(part, &data[1000..1123]);
    let beyond = Span {
        offset: 4990,
        length: 11,
    };
    assert!(matches!(
        remote.download_file("a/b.bin", Some(beyond), &mut Vec::new()),
        Err(RemoteError::Invalid(_))
    ));
    // a span of the source uploads only those bytes
    remote
        .upload_file("slice", &root.join("file"), Some(span), &Metadata::new())
        .unwrap();
    assert_eq!(remote.stat("slice").unwrap().unwrap().bytes, 123);
    remote.delete("a/b.bin").unwrap();
    remote.delete("a/b.bin").unwrap();
    assert_eq!(remote.stat("a/b.bin").unwrap(), None);
    assert_eq!(
        remote.download_file("a/b.bin", None, &mut Vec::new()),
        Err(RemoteError::Missing)
    );
    for bad in ["", "/x", "x/", "a//b", ".hidden", "a/../b", "sp ace"] {
        assert!(
            matches!(remote.stat(bad), Err(RemoteError::Invalid(_))),
            "{bad:?}"
        );
    }
    let too_big = folder(&root, 100, 100);
    assert!(matches!(
        too_big.upload_file("big", &root.join("file"), None, &Metadata::new()),
        Err(RemoteError::TooLarge {
            bytes: 5000,
            max: 100
        })
    ));
}

#[test]
fn whole_file_round_trip_records_sha256() {
    let root = scratch("whole");
    let remote = folder(&root, 1 << 20, 1 << 20);
    let data = bytes(70_000, 3);
    fs::write(root.join("file"), &data).unwrap();
    let sent = pieces::upload(&remote, "store", &root.join("file"), &Metadata::new()).unwrap();
    assert_eq!((sent.bytes, sent.pieces), (70_000, 0));
    let stat = remote.stat("store").unwrap().unwrap();
    assert_eq!(stat.metadata[pieces::LAYOUT], "whole");
    assert_eq!(stat.metadata[pieces::SHA256], pieces::hex(&sent.sha256));
    assert_eq!(download(&remote, "store", &root).unwrap(), data);
}

#[test]
fn file_over_max_goes_up_as_pieces_and_is_verified() {
    let root = scratch("pieces");
    let remote = folder(&root, 1000, 300);
    let data = bytes(2500, 9);
    fs::write(root.join("file"), &data).unwrap();
    let sent = pieces::upload(&remote, "store", &root.join("file"), &Metadata::new()).unwrap();
    assert_eq!(sent.pieces, 9);
    assert!(sent.cleanup.is_none());
    for index in 0..9 {
        let size = remote
            .stat(&piece_name("store", index))
            .unwrap()
            .unwrap()
            .bytes;
        assert_eq!(size, if index == 8 { 100 } else { 300 });
    }
    assert_eq!(remote.stat(&piece_name("store", 9)).unwrap(), None);
    assert_eq!(
        remote.stat("store").unwrap().unwrap().metadata[pieces::LAYOUT],
        "pieces"
    );
    assert_eq!(download(&remote, "store", &root).unwrap(), data);
    // no staged manifest is left beside the source
    assert_eq!(fs::read_dir(&root).unwrap().count(), 3);

    // a corrupted piece fails verification instead of returning its bytes
    let piece = root.join("remote/objects").join(piece_name("store", 4));
    let mut bad = fs::read(&piece).unwrap();
    bad[0] ^= 1;
    fs::write(&piece, bad).unwrap();
    assert!(matches!(
        download(&remote, "store", &root),
        Err(RemoteError::Integrity(_))
    ));

    // a smaller, whole upload removes the pieces the earlier one left
    fs::write(root.join("small"), bytes(500, 1)).unwrap();
    let sent = pieces::upload(&remote, "store", &root.join("small"), &Metadata::new()).unwrap();
    assert_eq!(sent.pieces, 0);
    assert!(sent.cleanup.is_none());
    for index in 0..9 {
        assert_eq!(remote.stat(&piece_name("store", index)).unwrap(), None);
    }
    assert_eq!(download(&remote, "store", &root).unwrap(), bytes(500, 1));
    pieces::delete(&remote, "store").unwrap();
    assert_eq!(remote.stat("store").unwrap(), None);
}

#[test]
fn manifest_round_trip_and_refusals() {
    let manifest = Manifest {
        bytes: 700,
        sha256: [7; 32],
        piece_size: 300,
        pieces: vec![
            Piece {
                bytes: 300,
                sha256: [1; 32],
            },
            Piece {
                bytes: 300,
                sha256: [2; 32],
            },
            Piece {
                bytes: 100,
                sha256: [3; 32],
            },
        ],
    };
    assert_eq!(Manifest::decode(&manifest.encode()).unwrap(), manifest);
    let mut short = manifest.clone();
    short.pieces.pop();
    assert!(Manifest::decode(&short.encode()).is_err());
    let mut uneven = manifest.clone();
    uneven.pieces[0].bytes = 299;
    uneven.pieces[2].bytes = 101;
    assert!(Manifest::decode(&uneven.encode()).is_err());
    assert!(Manifest::decode("layerfs-pieces 2\n").is_err());
    let text = manifest.encode().replace("sha256 07", "sha256 0g");
    assert!(Manifest::decode(&text).is_err());
}

#[test]
fn reserved_metadata_and_missing_digest_are_refused() {
    let root = scratch("reserved");
    let remote = folder(&root, 1 << 20, 1 << 20);
    fs::write(root.join("file"), b"abc").unwrap();
    let reserved = Metadata::from([(pieces::SHA256.to_owned(), "x".to_owned())]);
    assert!(matches!(
        pieces::upload(&remote, "f", &root.join("file"), &reserved),
        Err(RemoteError::Invalid(_))
    ));
    // a file put by the driver alone has no recorded sha256
    remote
        .upload_file("raw", &root.join("file"), None, &Metadata::new())
        .unwrap();
    assert!(matches!(
        download(&remote, "raw", &root),
        Err(RemoteError::Integrity(_))
    ));
    assert_eq!(
        download(&remote, "absent", &root),
        Err(RemoteError::Missing)
    );
}
