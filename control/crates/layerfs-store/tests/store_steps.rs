//! The `store.file` frame (layerfs-project#109): the typed step encodes the
//! frame its callers built by hand, and the worker's decode refuses in the
//! order it always did.
use layerfs_bridge::control::SdkValue;
use layerfs_store::sqlite::transfer::step::{StepFrame, StoreStep};

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
