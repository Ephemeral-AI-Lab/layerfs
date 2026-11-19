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
        for name in ["commit.list", "commit.get", "commit.diff", "file.history"] {
            let args = SdkValue::map([
                ("project", SdkValue::text("shop/api")),
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/src/app.js")),
            ]);
            let call = layerfs_api::find(name).unwrap_or_else(|| panic!("{name} is missing"));
            let error = scope.apply(call, args).unwrap_err();
            assert_eq!(error.code, "InvalidInput", "{name}");
            assert_eq!(error.message, "out of scope: history", "{name}");
        }
    }
}
