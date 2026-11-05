//! Live progress of a real backup and restore against MinIO (layerfs-project
//! #5): a Store above the part size goes up in several multipart parts, and
//! `backup.create` and `backup.restore` send their phases in order with byte
//! counts that end on the bytes they moved. Needs the LAYERFS_REMOTE_*
//! variables tests/minio_service.sh of layerfs-store-backup prints; without
//! them the test says NOT_RUN and passes.
use layerfs_api::home;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-progress-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// `(phase, done, total)` of one update.
type Seen = (String, Option<i64>, Option<i64>);

/// Runs `name` and returns its result and the progress it sent.
fn traced(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> (SdkValue, Vec<Seen>) {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    let mut seen = Vec::new();
    let result = layerfs_api::call(Some(home), name, args, &mut |event| {
        if let Some(p) = event.get("progress") {
            seen.push((
                p.get_text("phase").unwrap().to_owned(),
                p.get_int("done"),
                p.get_int("total"),
            ));
        }
    })
    .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message));
    (result, seen)
}

/// The phases in the order they were entered.
fn phases(seen: &[Seen]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (phase, _, _) in seen {
        if out.last() != Some(&phase.as_str()) {
            out.push(phase);
        }
    }
    out
}

/// The last update of `phase`, after checking its counts only grow.
fn ended(seen: &[Seen], phase: &str) -> Seen {
    let of: Vec<&Seen> = seen.iter().filter(|s| s.0 == phase).collect();
    assert!(of.windows(2).all(|p| p[0].1 <= p[1].1), "{seen:?}");
    of.last().map(|s| (*s).clone()).expect(phase)
}

/// Bytes that do not compress, so the Store is as large as the file.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn create_and_restore_send_their_bytes_against_minio() {
    let (Ok(endpoint), Ok(bucket)) = (
        std::env::var("LAYERFS_REMOTE_ENDPOINT"),
        std::env::var("LAYERFS_REMOTE_BUCKET"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_REMOTE_ENDPOINT / LAYERFS_REMOTE_BUCKET unset");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("data.bin"), noise(12 << 20)).unwrap();
    traced(&home, "store.create", &[("name", t("shop"))]);
    traced(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    // a prefix of this run's own: a container's process ids repeat
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let url = format!("{endpoint}/{bucket}/lfs-progress-{run}");
    traced(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            ("key", t("env:LAYERFS_REMOTE")),
            ("part_size", SdkValue::Int(5 << 20)),
        ],
    );

    let (made, seen) = traced(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    let bytes = made.get_int("bytes").unwrap();
    assert!(
        bytes > 2 * (5 << 20),
        "the Store spans several parts: {bytes}"
    );
    assert_eq!(
        phases(&seen),
        ["check", "snapshot", "check", "hash", "upload"],
        "{seen:?}"
    );
    for phase in ["hash", "upload"] {
        let last = ended(&seen, phase);
        assert_eq!((last.1, last.2), (Some(bytes), Some(bytes)), "{seen:?}");
    }

    let (restored, seen) = traced(
        &home,
        "backup.restore",
        &[
            ("store", t("copy")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    assert_eq!(restored.get_int("bytes"), Some(bytes));
    assert_eq!(restored.get_text("sha256"), made.get_text("sha256"));
    assert_eq!(
        phases(&seen),
        ["check", "download", "verify", "install"],
        "{seen:?}"
    );
    let last = ended(&seen, "download");
    assert_eq!((last.1, last.2), (Some(bytes), Some(bytes)), "{seen:?}");
    // byte counts belong to the transfer phases only
    assert!(seen
        .iter()
        .filter(|s| ["check", "snapshot", "verify", "install"].contains(&s.0.as_str()))
        .all(|s| s.1.is_none() && s.2.is_none()));
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}
