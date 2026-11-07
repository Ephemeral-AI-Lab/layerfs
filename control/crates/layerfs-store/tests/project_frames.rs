//! The `project` worker frames (layerfs-project#109): each typed frame
//! encodes the map its caller built by hand, and the worker's decode
//! refuses in the order it always did.
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_store::project::frame::{InitFrame, RemoveFrame, StageFrame, StageStep};

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

const ID: &str = "0123456789abcdef";

fn store() -> StoreName {
    StoreName::parse("shop").unwrap()
}

#[test]
fn each_frame_encodes_the_map_its_caller_built() {
    let record = map(vec![("name", t("shop")), ("kind", t("sqlite"))]);
    let init = InitFrame {
        store: store(),
        name: Some("web".to_owned()),
        staged: ID.to_owned(),
        timeout_ms: Some(5),
        store_record: Some(record.clone()),
    };
    let encoded = init.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("store", t("shop")),
            ("name", t("web")),
            ("staged", t(ID)),
            ("timeoutMs", SdkValue::Int(5)),
            ("store_record", record.clone()),
        ])
    );
    let decoded = InitFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.store, init.store);
    assert_eq!((decoded.name, decoded.timeout_ms), (init.name, Some(5)));
    assert_eq!(decoded.store_record, init.store_record);

    let remove = RemoveFrame {
        store: store(),
        project: "web".to_owned(),
        store_record: Some(record.clone()),
    };
    let encoded = remove.encode();
    assert_eq!(
        encoded,
        map(vec![
            ("store", t("shop")),
            ("project", t("web")),
            ("store_record", record),
        ])
    );
    let decoded = RemoveFrame::decode(&encoded).unwrap();
    assert_eq!(
        (decoded.store, decoded.project),
        (store(), "web".to_owned())
    );

    let stage = |step| StageFrame {
        store: "shop".to_owned(),
        name: "web".to_owned(),
        id: ID.to_owned(),
        step,
    };
    let head = || vec![("store", t("shop")), ("name", t("web")), ("staged", t(ID))];
    let mut write = head();
    write.extend([("step", t("write")), ("part", SdkValue::Bytes(vec![1, 2]))]);
    let encoded = stage(StageStep::Write(vec![1, 2])).encode();
    assert_eq!(encoded, map(write));
    let decoded = StageFrame::decode(&encoded).unwrap();
    assert_eq!(decoded.step, StageStep::Write(vec![1, 2]));
    assert_eq!(decoded.id, ID);
    let mut discard = head();
    discard.push(("step", t("discard")));
    assert_eq!(stage(StageStep::Discard).encode(), map(discard));
}

#[test]
fn the_worker_refuses_a_frame_in_its_old_order() {
    let init = |fields| InitFrame::decode(&map(fields)).unwrap_err().message;
    // the Store, its name, then the copy's id
    assert_eq!(init(vec![]), "missing text argument `store`");
    assert!(init(vec![("store", t("../x"))]).starts_with("store name"));
    assert_eq!(
        init(vec![("store", t("shop")), ("staged", t("ABC"))]),
        "`staged` must be a folder copy's id: 16 lowercase hex digits"
    );
    // name and timeoutMs are the add's to check
    let bare = InitFrame::decode(&map(vec![("store", t("shop")), ("staged", t(ID))])).unwrap();
    assert_eq!((bare.name, bare.timeout_ms), (None, None));

    let remove = |fields| RemoveFrame::decode(&map(fields)).unwrap_err().message;
    assert_eq!(remove(vec![]), "missing text argument `store`");
    // no project before a bad Store name
    assert_eq!(
        remove(vec![("store", t("../x"))]),
        "missing text argument `project`"
    );
    assert!(remove(vec![("store", t("../x")), ("project", t("web"))]).starts_with("store name"));

    let stage = |fields| StageFrame::decode(&map(fields)).unwrap_err().message;
    // a bad id before an unknown step
    assert_eq!(
        stage(vec![("step", t("nope"))]),
        "`staged` must be a folder copy's id: 16 lowercase hex digits"
    );
    assert_eq!(
        stage(vec![("staged", t(ID)), ("step", t("write"))]),
        "`part` must be bytes"
    );
    assert_eq!(
        stage(vec![("staged", t(ID)), ("step", t("nope"))]),
        "no folder copy step `nope`"
    );
    assert_eq!(stage(vec![("staged", t(ID))]), "no folder copy step ``");
}

#[test]
fn a_worker_init_checks_name_and_timeout_after_the_copy_is_found() {
    /// The test's root, removed when the test ends, failing or not.
    struct Root(std::path::PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let temp = Root(std::env::temp_dir().join(format!("layerfs-frames-{}", std::process::id())));
    let root = temp.0.clone();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let frame = InitFrame::decode(&map(vec![("store", t("shop")), ("staged", t(ID))])).unwrap();
    let refusal = layerfs_store::project::init::in_worker(&root, &frame).unwrap_err();
    assert_eq!(refusal.code, "NotFound", "{refusal:?}");
    assert!(refusal.message.starts_with("no folder copy at "));
    // with the copy there, the add refuses the missing name, and the copy
    // is removed
    let staged = layerfs_store::project::staged::folder(&root, ID);
    std::fs::create_dir_all(&staged).unwrap();
    let refusal = layerfs_store::project::init::in_worker(&root, &frame).unwrap_err();
    assert_eq!(refusal.message, "missing text argument `name`");
    assert!(!staged.exists());
    // then a missing time limit
    std::fs::create_dir_all(&staged).unwrap();
    let named = InitFrame {
        name: Some("web".to_owned()),
        ..frame
    };
    let refusal = layerfs_store::project::init::in_worker(&root, &named).unwrap_err();
    assert_eq!(
        refusal.message,
        "timeoutMs is required: a positive whole number of milliseconds"
    );
    assert!(!staged.exists());
}
