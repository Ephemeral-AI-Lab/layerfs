//! The `store.file` frame (layerfs-project#109): the typed step encodes the
//! frame its callers built by hand, and the worker's decode refuses in the
//! order it always did.
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::backend::sqlite::step_request::{StepFrame, StoreStep};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn frame(op: &str, args: Vec<(&str, SdkValue)>) -> SdkValue {
    let args = args.into_iter().map(|(n, v)| (n.to_owned(), v)).collect();
    SdkValue::map([("op", t(op)), ("args", SdkValue::Map(args))])
}

fn refusal(value: &SdkValue) -> String {
    StepFrame::decode(value).unwrap_err().message
}

#[test]
fn a_step_encodes_the_frame_its_callers_built() {
    let on = |step| StepFrame::on("s", step).encode();
    assert_eq!(on(StoreStep::Stat), frame("stat", vec![("store", t("s"))]));
    let read = StoreStep::Read {
        offset: 8,
        len: 16,
        staged: false,
    };
    assert_eq!(
        on(read),
        frame(
            "read",
            vec![
                ("store", t("s")),
                ("offset", SdkValue::Int(8)),
                ("len", SdkValue::Int(16)),
            ],
        )
    );
    let write = StoreStep::Write {
        offset: 0,
        bytes: vec![1, 2],
    };
    assert_eq!(
        on(write),
        frame(
            "write",
            vec![
                ("store", t("s")),
                ("offset", SdkValue::Int(0)),
                ("bytes", SdkValue::Bytes(vec![1, 2])),
            ],
        )
    );
    // a field the snapshot lacked is sent null
    let verify = StoreStep::Verify {
        bytes: Some(3),
        sha256: Some("ab".to_owned()),
        heads: None,
    };
    assert_eq!(
        on(verify),
        frame(
            "verify",
            vec![
                ("store", t("s")),
                ("bytes", SdkValue::Int(3)),
                ("sha256", t("ab")),
                ("heads", SdkValue::Null),
            ],
        )
    );
    // a backup's steps carry the Store record, after the Store
    let record = SdkValue::map([("name", t("s"))]);
    let staged = StepFrame {
        store: "s".to_owned(),
        store_record: Some(record.clone()),
        step: StoreStep::Read {
            offset: 0,
            len: 4,
            staged: true,
        },
    };
    let encoded = staged.encode();
    assert_eq!(
        encoded,
        frame(
            "read",
            vec![
                ("store", t("s")),
                ("store_record", record),
                ("offset", SdkValue::Int(0)),
                ("len", SdkValue::Int(4)),
                ("staged", SdkValue::Bool(true)),
            ],
        )
    );
    let decoded = StepFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.step, staged.step);
    assert_eq!(decoded.store_record, staged.store_record);
}

#[test]
fn the_worker_refuses_a_frame_in_its_old_order() {
    let none = SdkValue::map([("args", SdkValue::Null)]);
    assert_eq!(refusal(&none), "Store step name");
    assert_eq!(
        refusal(&frame("nope", vec![])),
        "a Store step names its Store"
    );
    // a bad Store name before an unknown step
    assert!(refusal(&frame("nope", vec![("store", t("../x"))])).contains("store"));
    assert_eq!(
        refusal(&frame("nope", vec![("store", t("s"))])),
        "no Store step nope"
    );
    let read = |args| refusal(&frame("read", args));
    assert_eq!(
        read(vec![("store", t("s")), ("len", SdkValue::Int(1))]),
        "`offset` must be a whole number"
    );
    assert_eq!(
        read(vec![("store", t("s")), ("offset", SdkValue::Int(-1))]),
        "`offset` must be a whole number"
    );
    assert_eq!(
        read(vec![("store", t("s")), ("offset", SdkValue::Int(0))]),
        "`len` must be a whole number"
    );
    let write = |args| refusal(&frame("write", args));
    assert_eq!(
        write(vec![("store", t("s")), ("bytes", SdkValue::Bytes(vec![]))]),
        "`offset` must be a whole number"
    );
    assert_eq!(
        write(vec![("store", t("s")), ("offset", SdkValue::Int(0))]),
        "`bytes` must be bytes"
    );
}

/// A compaction record beside Store file `file`, owned by this process: a
/// run that is live on this machine unless `returned`.
fn compact_record(file: &std::path::Path, returned: bool) {
    // where the kernel names no boot the owner is matched by its id alone
    let (start, boot) = match layerfs_home::record::process::me() {
        Some((start, boot)) => (start.to_string(), format!("{boot:032x}")),
        None => (String::new(), String::new()),
    };
    let zeros = |n: usize| "0".repeat(n);
    let text = format!(
        "version=1\nop_id={}\ngeneration=4\nphase=build\nstarted_ms=1794993419000\n\
owner_pid={}\nowner_pid_start={start}\nowner_pid_boot={boot}\nowner_pid_namespace=\n\
catalog_id={}\nnext_history_serial=1\nnext_pack_id=1\nnext_ordinal=1\n\
mark_serial=0\nmark_pack_id=1\nmark_digest={}\nmark_totals=0,0,0,0\nmark_stamp=0\nhistory_rows=0\n\
live_heads_digest={}\nreturned={}\n",
        zeros(32),
        std::process::id(),
        zeros(32),
        zeros(64),
        zeros(64),
        u8::from(returned),
    );
    let mut path = file.as_os_str().to_owned();
    path.push(".compact-op");
    std::fs::write(path, text).unwrap();
}

/// Plan Part 3 fix 1: after a lost reply the caller's lease is gone, so a
/// move's or remove's steps on the machine that compacts are refused `Busy`,
/// naming the run as `op`, while its record names a live run; a returned
/// run's record no longer holds them.
#[test]
fn a_live_compaction_on_the_machine_refuses_the_steps_that_copy_or_delete_the_file() {
    use layerfs_store::backend::sqlite::step_run;
    let root = std::env::temp_dir().join(format!("layerfs-steps-busy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let file = layerfs_home::disk::paths::store_file(&root, "s");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"not a store").unwrap();
    compact_record(&file, false);
    for step in [
        StoreStep::Snapshot,
        StoreStep::Stage,
        StoreStep::Install,
        StoreStep::Retire,
        StoreStep::Remove,
    ] {
        let name = step.name();
        let refusal = step_run::run(&root, &StepFrame::on("s", step)).unwrap_err();
        assert_eq!(refusal.code, "Busy", "{name}: {}", refusal.message);
        let op = refusal.detail.get("next").and_then(|next| next.get("op"));
        let op = op.unwrap_or_else(|| panic!("{name}: no op in {:?}", refusal.detail));
        assert_eq!(op.get_text("name"), Some("compact"), "{name}");
        assert_eq!(op.get_text("phase"), Some("build"), "{name}");
        assert_eq!(op.get_text("started_at"), Some("2026-11-18T09:16:59Z"));
        assert!(file.is_file(), "{name} left the file in place");
    }
    // a read is not held, and a run that returned holds nothing
    assert!(step_run::run(&root, &StepFrame::on("s", StoreStep::Stat)).is_ok());
    compact_record(&file, true);
    let removed = step_run::run(&root, &StepFrame::on("s", StoreStep::Remove)).unwrap();
    assert_eq!(removed.get("removed"), Some(&SdkValue::Bool(true)));
    let _ = std::fs::remove_dir_all(&root);
}
