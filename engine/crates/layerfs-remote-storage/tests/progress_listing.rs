//! #506 E4/E9: the progress cell and its throttle, the phases an upload and
//! a download report, the folder listing, the size metadata and the two
//! S3 timestamp forms.
use layerfs_remote_storage::local_folder::LocalFolder;
use layerfs_remote_storage::pieces;
use layerfs_remote_storage::progress::{self, Cell, Progress, Sample};
use layerfs_remote_storage::{time, Driver, Limits, Metadata};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn scratch(tag: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("progress-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("remote")).unwrap();
    path
}

fn folder(root: &std::path::Path, max_file: u64, piece_size: u64) -> LocalFolder {
    let limits = Limits {
        max_file,
        max_request: max_file,
        piece_size,
    };
    LocalFolder::new(&root.join("remote"), limits).unwrap()
}

#[test]
fn cell_publishes_phases_at_once_and_bytes_per_interval() {
    // an interval no test reaches: bytes stay at the phase's start
    let held = Cell::new(Duration::from_secs(3600));
    assert_eq!(held.sample(), Sample::default());
    held.begin("upload", Some(100));
    held.add(40);
    held.add(10);
    assert_eq!(
        held.sample(),
        Sample {
            phase: Some("upload"),
            bytes: Some(0),
            total: Some(100),
            pieces: None
        }
    );
    held.pieces(1, 3);
    assert_eq!(held.sample().pieces, Some((1, 3)));
    // a phase without a total shows no bytes, and a new phase resets
    held.begin("record", None);
    assert_eq!(
        held.sample(),
        Sample {
            phase: Some("record"),
            bytes: None,
            total: None,
            pieces: None
        }
    );

    // a zero interval publishes every addition
    let live = Cell::new(Duration::ZERO);
    live.begin("hash", Some(100));
    live.add(40);
    assert_eq!(live.sample().bytes, Some(40));
    live.add(60);
    assert_eq!(live.sample().bytes, Some(100));
    live.begin("upload", Some(100));
    assert_eq!(live.sample().bytes, Some(0));
}

/// Every report, in order, without a throttle.
#[derive(Default)]
struct Log(Mutex<Vec<String>>);

impl Progress for Log {
    fn begin(&self, phase: &'static str, total: Option<u64>) {
        self.0.lock().unwrap().push(format!("{phase} {total:?}"));
    }
    fn add(&self, bytes: u64) {
        let mut log = self.0.lock().unwrap();
        match log.last_mut() {
            Some(last) if last.starts_with('+') => {
                let sum: u64 = last[1..].parse().unwrap();
                *last = format!("+{}", sum + bytes);
            }
            _ => log.push(format!("+{bytes}")),
        }
    }
    fn pieces(&self, done: u64, of: u64) {
        self.0.lock().unwrap().push(format!("piece {done}/{of}"));
    }
}

#[test]
fn upload_and_download_report_their_phases_and_bytes() {
    let root = scratch("phases");
    let remote = folder(&root, 1000, 400);
    fs::write(root.join("file"), vec![7u8; 1000]).unwrap();
    let log = Arc::new(Log::default());
    let sent = progress::observe(log.clone(), || {
        pieces::upload(
            &remote,
            "s/store.sqlite",
            &root.join("file"),
            &Metadata::new(),
        )
    })
    .unwrap();
    assert_eq!(sent.pieces, 0);
    assert_eq!(
        *log.0.lock().unwrap(),
        ["hash Some(1000)", "+1000", "upload Some(1000)", "+1000"]
    );
    // the size goes with the file, for one stat to tell it
    let stat = remote.stat("s/store.sqlite").unwrap().unwrap();
    assert_eq!(stat.metadata[pieces::BYTES], "1000");
    assert!(stat.modified.is_some());

    let log = Arc::new(Log::default());
    fs::write(root.join("big"), vec![9u8; 1100]).unwrap();
    let sent = progress::observe(log.clone(), || {
        pieces::upload(
            &remote,
            "t/store.sqlite",
            &root.join("big"),
            &Metadata::new(),
        )
    })
    .unwrap();
    assert_eq!(sent.pieces, 3);
    assert_eq!(
        *log.0.lock().unwrap(),
        [
            "hash Some(1100)",
            "+1100",
            "upload Some(1100)",
            "piece 0/3",
            "+400",
            "piece 1/3",
            "+400",
            "piece 2/3",
            "+300",
            "piece 3/3",
        ]
    );
    let stat = remote.stat("t/store.sqlite").unwrap().unwrap();
    assert_eq!(stat.metadata[pieces::BYTES], "1100");

    let log = Arc::new(Log::default());
    let mut out = fs::File::create(root.join("down")).unwrap();
    progress::observe(log.clone(), || {
        pieces::download(&remote, "t/store.sqlite", &mut out)
    })
    .unwrap();
    let seen = log.0.lock().unwrap().clone();
    assert_eq!(
        seen,
        [
            "download Some(1100)",
            "piece 0/3",
            "+400",
            "piece 1/3",
            "+400",
            "piece 2/3",
            "+300",
            "piece 3/3",
        ]
    );
    // nothing reports where no progress is installed
    pieces::upload(
        &remote,
        "u/store.sqlite",
        &root.join("file"),
        &Metadata::new(),
    )
    .unwrap();
    assert_eq!(log.0.lock().unwrap().clone(), seen);
}

#[test]
fn folder_lists_by_prefix_with_metadata() {
    let root = scratch("list");
    let remote = folder(&root, 1 << 20, 1 << 20);
    fs::write(root.join("file"), b"0123456789").unwrap();
    for name in ["a/store.sqlite", "b/store.sqlite", "b/other", "c"] {
        pieces::upload(&remote, name, &root.join("file"), &Metadata::new()).unwrap();
    }
    let all: Vec<_> = remote.list("").unwrap();
    let names: Vec<_> = all.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["a/store.sqlite", "b/other", "b/store.sqlite", "c"]);
    assert!(all.iter().all(|l| l.bytes == 10 && l.modified.is_some()));
    assert_eq!(all[0].metadata.as_ref().unwrap()[pieces::BYTES], "10");
    let b: Vec<_> = remote
        .list("b/")
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(b, ["b/other", "b/store.sqlite"]);
    assert!(remote.list("none/").unwrap().is_empty());
}

#[test]
fn timestamps_in_both_s3_forms() {
    let expected = 1_445_412_480_000;
    assert_eq!(
        time::http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
        Some(expected)
    );
    assert_eq!(time::iso8601("2015-10-21T07:28:00Z"), Some(expected));
    assert_eq!(
        time::iso8601("2015-10-21T07:28:00.123Z"),
        Some(expected + 123)
    );
    assert_eq!(
        time::iso8601("2015-10-21T07:28:00.5Z"),
        Some(expected + 500)
    );
    assert_eq!(time::http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(time::iso8601("2000-02-29T00:00:00Z"), Some(951_782_400_000));
    for bad in [
        "",
        "Wed, 21 Oct 2015 07:28:00",
        "Wed, 21 Foo 2015 07:28:00 GMT",
    ] {
        assert_eq!(time::http_date(bad), None, "{bad}");
    }
    for bad in [
        "2015-10-21 07:28:00Z",
        "2015-13-21T07:28:00Z",
        "2015-10-21T07:28:00.Z",
    ] {
        assert_eq!(time::iso8601(bad), None, "{bad}");
    }
}
