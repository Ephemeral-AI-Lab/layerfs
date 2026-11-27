//! The overlay template of a Store-calls service is created by its first
//! file-changing call, not by the open (#819): a process that only reads
//! never creates or deletes it, and the first write starts the owner, which
//! later writes reuse.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_protocol::sdk::{SdkCall, SdkValue};
use layerfs_store_service::StoreService;

#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;

fn call(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut args: Vec<(String, SdkValue)> =
        fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    args.push(("project".to_owned(), SdkValue::text("installed")));
    let call = SdkCall {
        method: method.to_owned(),
        args: SdkValue::Map(args),
    };
    layerfs_store_service::methods::call(service, &call)
        .unwrap_or_else(|refusal| panic!("{method}: {refusal:?}"))
}

#[test]
fn reads_create_no_overlay_template_and_the_first_write_does() {
    let fixture = installed::Fixture::new(1, "overlay-template");
    let template = fixture.directory.join("overlay.sqlite");
    let service = StoreService::open_direct(
        &fixture.config.path,
        installed::BINDING,
        installed::CURSOR,
        &template,
    )
    .unwrap();
    assert!(!template.exists(), "the open creates no overlay template");

    let main = ("branch", SdkValue::text("main"));
    call(
        &service,
        "fs.list",
        vec![main.clone(), ("path", SdkValue::text("/"))],
    );
    assert!(!template.exists(), "a read creates no overlay template");

    call(
        &service,
        "branch.fork",
        vec![("name", SdkValue::text("work"))],
    );
    for step in 0..2 {
        call(
            &service,
            "fs.write",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text(format!("/w{step}.txt"))),
                ("bytes", SdkValue::Bytes(b"x\n".to_vec())),
            ],
        );
        assert!(template.exists(), "a write starts the overlay owner");
    }
    service.close().unwrap();
    fixture.cleanup();
}
