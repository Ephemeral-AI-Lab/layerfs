//! Concurrent Store reads on one home: a call that takes a Store's lease
//! while another process drops its own lease and ends is not refused
//! `Failed` (`Os { NotFound }`) for removing that lease as stale. (A sqlite
//! Store open that meets another process's open may still be refused `Busy`
//! by the engine's cooperative admission; that is not this race.)
mod common;
use common::{layerfs, run, tempdir, text};

#[test]
fn parallel_branch_lists_never_trip_over_a_dropped_lease() {
    let home = tempdir("concurrent-home");
    let source = tempdir("concurrent-source");
    std::fs::write(source.join("a.txt"), b"hi\n").unwrap();
    for words in [
        vec!["store", "create", "s"],
        vec!["project", "init", "s/p", source.to_str().unwrap()],
    ] {
        let out = run(&mut layerfs(&home, &words));
        assert!(out.status.success(), "layerfs {words:?}: {}", text(&out.stderr));
    }
    for round in 0..6 {
        let children: Vec<_> = (0..8)
            .map(|_| {
                layerfs(&home, &["branch", "list", "s/p"])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        for child in children {
            let out = common::finish(child);
            let stderr = text(&out.stderr);
            assert!(
                out.status.success() || stderr.contains("busy before effect"),
                "round {round}: branch list refused: {stderr}"
            );
        }
    }
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&source).unwrap();
}
