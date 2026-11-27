//! #818 W1 (lane W): a file call's hidden-path check answers from the
//! markers its private bind kept (`KnownRemovals`), and reads them on the
//! control reader as before when none are kept. Both regimes answer the
//! same: `edit` of a hidden path is `Removed` with its frontier, of a path
//! that never existed `NoSuchPath`, and a write elsewhere commits.
//!
//! Over the bound: markers charging more than `KEPT_REMOVAL_BYTES` (256 KiB)
//! are not kept, so the call takes the fallback reads (the eager marker read
//! SPEC D16 keeps); its control reader then does strictly more work than in
//! the kept regime. Runs on SQLite, and on PostgreSQL with
//! `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_protocol::provision::install::SdkValue;
use rt::{branch_arg as branch, path_arg as path, t, Backend, Opened, Store};

/// The control reader's statements of one call, and its answer.
fn control_of(
    opened: &Opened,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> (u64, Result<SdkValue, String>) {
    let before = opened.work();
    let reply = opened.call(method, fields);
    let work = rt::since(&opened.work(), &before);
    (rt::counts(&work[1])[2], reply)
}

/// One regime: `edit` of the hidden file and of an absent one, then a write.
/// Returns the control reader's statements of the edit refusal and the write.
fn regime(opened: &Opened, round: u32) -> (u64, u64) {
    let edit = |file: &str| {
        vec![
            branch(),
            path(file),
            ("oldText", t("h\n")),
            ("newText", t("H\n")),
        ]
    };
    let (refusal_control, hidden) = control_of(opened, "fs.edit", edit("/h/f.txt"));
    let hidden = hidden.expect_err("a hidden edit");
    assert!(hidden.starts_with("Removed: "), "{hidden}");
    let (_, absent) = control_of(opened, "fs.edit", edit("/h/never.txt"));
    let absent = absent.expect_err("an absent edit");
    assert!(absent.starts_with("NoSuchPath: "), "{absent}");
    // The write goes to Branch `other`: its Commit records the whiteout, so
    // `work` keeps the hidden path for the next regime.
    let (write_control, written) = control_of(
        opened,
        "fs.write",
        vec![
            ("branch", t("other")),
            path("/w.txt"),
            ("bytes", SdkValue::Bytes(format!("{round}\n").into_bytes())),
        ],
    );
    let written = written.expect("a write beside the markers");
    assert_eq!(written.get_text("kind"), Some("Committed"), "{written:?}");
    (refusal_control, write_control)
}

/// A marker path of about 3.8 KB that names no entry.
fn long_path(n: usize) -> String {
    let component = "x".repeat(250);
    let mut path = format!("/long{n:03}");
    for _ in 0..15 {
        path.push('/');
        path.push_str(&component);
    }
    path
}

fn scenario(store: &mut Store, backend: Backend) {
    let source = store.dir.join("source");
    std::fs::create_dir_all(source.join("h")).unwrap();
    std::fs::write(source.join("h/f.txt"), "h\n").unwrap();
    std::fs::write(source.join("w.txt"), "w\n").unwrap();
    store.init(&source, &mut |_, _| {});
    let opened = store.open("kept");
    rt::fork_work(&opened);
    opened
        .call("branch.fork", vec![("name", t("other"))])
        .unwrap();
    let removed = opened
        .call("project.remove_file", vec![("path", t("/h/f.txt"))])
        .unwrap();
    assert_eq!(removed.get_text("kind"), Some("Removed"), "{removed:?}");

    // One marker: kept by the bind, so the write reads no marker.
    let (kept_refusal, kept_write) = regime(&opened, 1);

    // 70 more markers of about 3.8 KB each: over the 256 KiB kept bound.
    for n in 0..70 {
        opened
            .call("project.remove_file", vec![("path", t(&long_path(n)))])
            .unwrap();
    }
    let (read_refusal, read_write) = regime(&opened, 2);
    println!(
        "W1_KEPT {{\"backend\":\"{}\",\"kept\":[{kept_refusal},{kept_write}],\"fallback\":[{read_refusal},{read_write}]}}",
        backend.name()
    );
    assert!(
        read_write > kept_write && read_refusal > kept_refusal,
        "the fallback reads the markers on the control reader: kept \
         [{kept_refusal}, {kept_write}], fallback [{read_refusal}, {read_write}]"
    );
    opened.stop();
}

#[test]
fn sqlite_hidden_check_from_kept_markers_and_fallback() {
    let mut store = Store::sqlite("w1-kept-sqlite");
    scenario(&mut store, Backend::Sqlite);
}

#[test]
fn postgresql_hidden_check_from_kept_markers_and_fallback() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("w1kept") else {
        return;
    };
    scenario(&mut store, Backend::Postgresql);
}
