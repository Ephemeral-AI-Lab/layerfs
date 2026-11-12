//! New history queries do not widen folder scopes to project-wide metadata.
use layerfs_api::Scope;
use layerfs_bridge::control::SdkValue;

#[test]
fn saved_history_calls_are_refused_before_dispatch_even_for_root_folder_scopes() {
    for folder in ["/", "/src"] {
        let scope = Scope::parse(&SdkValue::map([
            ("project", SdkValue::text("shop/api")),
            ("folder", SdkValue::text(folder)),
            ("branch", SdkValue::text("agent")),
        ]))
        .unwrap();
        for name in [
            "branch.commits",
            "branch.get_commit",
            "branch.diff",
            "file.revision",
        ] {
            let args = SdkValue::map([
                ("project", SdkValue::text("shop/api")),
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/src/app.js")),
            ]);
            let error = scope.apply(name, args).unwrap_err();
            assert_eq!(error.code, "InvalidInput", "{name}");
            assert_eq!(error.message, "out of scope: history", "{name}");
        }
    }
}

#[test]
fn store_storage_report_does_not_widen_folder_scopes() {
    let scope = Scope::parse(&SdkValue::map([
        ("project", SdkValue::text("shop/api")),
        ("folder", SdkValue::text("/")),
    ]))
    .unwrap();
    let error = scope
        .apply(
            "store.storage_report",
            SdkValue::map([
                ("name", SdkValue::text("shop")),
                ("snapshots", SdkValue::List(vec![])),
            ]),
        )
        .unwrap_err();
    assert_eq!(error.code, "InvalidInput");
}

#[test]
fn project_storage_report_does_not_widen_folder_scopes() {
    let scope = Scope::parse(&SdkValue::map([
        ("project", SdkValue::text("shop/api")),
        ("folder", SdkValue::text("/src")),
    ]))
    .unwrap();
    assert_eq!(
        scope
            .apply(
                "project.storage_report",
                SdkValue::map([
                    ("project", SdkValue::text("shop/api")),
                    ("snapshots", SdkValue::List(vec![])),
                ])
            )
            .unwrap_err()
            .code,
        "InvalidInput"
    );
}
