//! A read-only Store open answers Store reads and refuses mutation `ReadOnly`
//! before it binds or writes (#485).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::{bootstrap::open_store_read_only, StoreService};
use support::Fixture;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn try_call(
    service: &StoreService,
    method: &str,
    mut fields: Vec<(&str, SdkValue)>,
) -> Result<SdkValue, SdkRefusal> {
    fields.push(("project", t("installed")));
    layerfs_api::call(
        service,
        &SdkCall {
            method: method.into(),
            args: SdkValue::Map(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        },
    )
}

fn call(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    try_call(service, method, fields)
        .unwrap_or_else(|refusal| panic!("{method} refused: {refusal:?}"))
}

fn refused(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkRefusal {
    try_call(service, method, fields).expect_err("the call is refused")
}

fn read_only_is(code: &str, message: &str) {
    assert_eq!(
        (code, message),
        ("ReadOnly", "this Store is open read-only")
    );
}

#[test]
fn a_read_only_store_serves_reads_and_refuses_writes() {
    let fixture = Fixture::new("store-read-only", |source| {
        std::fs::write(source.join("hello.txt"), b"hello\n").unwrap();
        1
    });
    fixture.fork("feat");
    fixture.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/hello.txt")),
            ("content", t("hello\nworld\n")),
        ],
    );
    let host = fixture.close();

    let opened = open_store_read_only(
        host.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        1,
        64 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .expect("read-only Store sessions");
    let service = StoreService::from_read_only(opened);
    assert!(service.is_read_only());

    let read = call(
        &service,
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/hello.txt")),
            ("startLine", SdkValue::Int(1)),
            ("endLine", SdkValue::Int(20)),
        ],
    );
    assert_eq!(read.get_text("text"), Some("hello\nworld\n"));

    let listed = call(&service, "project.list_branches", vec![]);
    let names: Vec<_> = support::items(&listed)
        .iter()
        .map(|item| item.get_text("name").unwrap())
        .collect();
    assert!(
        names.contains(&"main") && names.contains(&"feat"),
        "{names:?}"
    );

    let commits = call(&service, "branch.list_commits", vec![("branch", t("feat"))]);
    assert_eq!(support::items(&commits).len(), 1);

    let write = refused(
        &service,
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/hello.txt")),
            ("content", t("nope\n")),
        ],
    );
    read_only_is(&write.code, &write.message);
    let mkdir = refused(
        &service,
        "fs.mkdir",
        vec![("branch", t("feat")), ("path", t("/nope"))],
    );
    read_only_is(&mkdir.code, &mkdir.message);
    let fork = refused(&service, "branch.fork", vec![("name", t("other"))]);
    read_only_is(&fork.code, &fork.message);
    let merge = refused(
        &service,
        "branch.merge",
        vec![
            ("branch", t("feat")),
            ("with", t("main")),
            ("name", t("merged")),
        ],
    );
    read_only_is(&merge.code, &merge.message);
    let publish = refused(&service, "branch.publish", vec![("branch", t("feat"))]);
    read_only_is(&publish.code, &publish.message);

    let again = call(
        &service,
        "fs.read",
        vec![
            ("branch", t("feat")),
            ("path", t("/hello.txt")),
            ("startLine", SdkValue::Int(1)),
            ("endLine", SdkValue::Int(20)),
        ],
    );
    assert_eq!(again.get_text("text"), Some("hello\nworld\n"));
    assert_eq!(
        support::items(&call(
            &service,
            "branch.list_commits",
            vec![("branch", t("feat"))]
        ))
        .len(),
        1
    );

    service.close().unwrap();
    host.cleanup();
}
