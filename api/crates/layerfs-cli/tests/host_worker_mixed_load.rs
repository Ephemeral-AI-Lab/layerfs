#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use layerfs_bridge::control::SdkValue;
use proof::Cli;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SIZE: usize = 8 * 1024 * 1024;
const NEEDLE: &str = "needle-in-last-line";

fn history(home: &Path, branch: &str) -> Vec<SdkValue> {
    let history = Cli::spawn(home, &["commit", "list", "shop/api", branch, "--detail"]).ok();
    let Some(SdkValue::List(items)) = history.get("items") else {
        panic!("missing Commit history: {history:?}");
    };
    items.clone()
}

struct Export(PathBuf);

impl Drop for Export {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn worker_reads_share_a_store_with_direct_grep_and_a_large_write() {
    let fixture = proof::create();
    let home = &fixture.paths.home;
    let source = home.join("proof-input");
    Cli::spawn(home, &["store", "create", "other"]).ok();
    Cli::spawn(
        home,
        &["project", "init", "other/api", source.to_str().unwrap()],
    )
    .ok();
    for branch in ["scan", "writer"] {
        Cli::spawn(home, &["branch", "fork", "shop/api", branch, "main"]).ok();
    }

    let row = b"the witness is at the end of this file\n";
    let rows = SIZE / row.len();
    let mut hay = row.repeat(rows);
    hay.extend_from_slice(NEEDLE.as_bytes());
    hay.push(b'\n');
    let hay_file = home.join("hay-input");
    std::fs::write(&hay_file, hay).unwrap();
    Cli::spawn(
        home,
        &[
            "file",
            "write",
            "shop/api",
            "scan",
            "/hay",
            "--from",
            hay_file.to_str().unwrap(),
        ],
    )
    .ok();

    let payload: Vec<u8> = (0..SIZE)
        .map(|index| ((index ^ (index >> 8) ^ ((index >> 21) * 97)) & 0xff) as u8)
        .collect();
    let input = home.join("write-input");
    std::fs::write(&input, &payload).unwrap();
    let before = history(home, "writer");
    let published = proof::commits(&fixture);

    let mut worker = fixture.start();
    fixture.ping();
    proof::assert_reads(proof::spawn_reads(&fixture, 1, false));
    let other = ["file", "stat", "other/api", "main", "/hello"];
    let other_before = Cli::spawn(home, &other).ok();
    let gate = proof::lock_store(&fixture);
    let mut grep = Cli::spawn(home, &["file", "grep", "shop/api", "scan", NEEDLE]);
    let mut write = Cli::spawn(
        home,
        &[
            "file",
            "write",
            "shop/api",
            "writer",
            "/payload",
            "--from",
            input.to_str().unwrap(),
            "--detail",
        ],
    );
    let reads = proof::spawn_reads(&fixture, 7, false);
    proof::pool(&fixture, 7, 7, 0);
    assert_eq!(Cli::spawn(home, &other).ok(), other_before);
    // The extra ordinary lane finished the unrelated Store before the
    // first Store was released. The final small read can now occupy it.
    let last_read = proof::spawn_reads(&fixture, 1, false);
    let mixed = proof::pool(&fixture, 8, 8, 0);
    assert_eq!(mixed.get_int("stores"), Some(2));
    drop(gate);

    let matches = grep.ok();
    let Some(SdkValue::List(matches)) = matches.get("items") else {
        panic!("missing grep matches: {matches:?}");
    };
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].get_text("path"), Some("/hay"));
    assert_eq!(matches[0].get_text("text"), Some(NEEDLE));
    assert_eq!(matches[0].get_int("line"), Some(rows as i64 + 1));
    proof::assert_reads(reads);
    proof::assert_reads(last_read);
    let completed = write.ok();
    let commits = history(home, "writer");
    assert_eq!(commits.len(), before.len() + 1);
    assert_eq!(&commits[1..], before.as_slice());
    assert_eq!(commits[0].get_text("commit"), completed.get_text("commit"));
    assert!(completed.get_text("commit").is_some());
    assert_eq!(proof::commits(&fixture), published);

    let destination = home.with_extension("mixed-export");
    assert!(!destination.exists());
    let export = Export(destination);
    Cli::spawn(
        home,
        &[
            "project",
            "export",
            "shop/api",
            export.0.to_str().unwrap(),
            "--branch",
            "writer",
        ],
    )
    .ok();
    assert!(std::fs::read(export.0.join("payload")).unwrap() == payload);
    fixture.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
}
