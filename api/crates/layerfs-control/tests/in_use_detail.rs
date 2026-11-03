//! `InUse` from `project.remove` fills `detail.related` with `{kind,
//! name}` entries (#536 §10b refusal detail): the project's Sandboxes.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::common::context::Context;
use layerfs_control::common::home::{paths, write_entity, write_record};
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("layerfs-in-use-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// `detail.related` as `(kind, name)` pairs.
fn related(refusal: &SdkRefusal) -> Vec<(String, String)> {
    let Some(SdkValue::List(items)) = refusal.detail.get("related") else {
        panic!("no detail.related: {refusal:?}");
    };
    let text = |v: &SdkValue, k: &str| v.get_text(k).unwrap().to_owned();
    items
        .iter()
        .map(|i| (text(i, "kind"), text(i, "name")))
        .collect()
}

fn refused(
    home: &Path,
    run: fn(&mut Context<'_>, &str, &SdkValue) -> layerfs_control::common::outcome::Outcome,
    call: &str,
    args: SdkValue,
) -> SdkRefusal {
    let mut events = |_| {};
    let mut context = Context::new(home.to_owned(), &mut events);
    run(&mut context, call, &args).unwrap_err()
}

#[test]
fn project_remove_names_the_project_s_sandboxes() {
    let home = home("project");
    let store = SdkValue::map([
        ("name", t("shop")),
        ("kind", t("sqlite")),
        ("machine", t("native")),
    ]);
    write_entity(&paths::store_record(&home, "shop"), &store).unwrap();
    let project = SdkValue::map([("name", t("api"))]);
    write_entity(&paths::project_record(&home, "shop", "api"), &project).unwrap();
    for name in ["sa", "sb"] {
        let sandbox = SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
        ]);
        write_record(&paths::sandbox_record(&home, name), &sandbox).unwrap();
    }
    let refusal = refused(
        &home,
        layerfs_control::project::family::run,
        "remove",
        SdkValue::map([("project", t("shop/api"))]),
    );
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    let sandbox = |n: &str| ("sandbox".to_owned(), n.to_owned());
    assert_eq!(related(&refusal), [sandbox("sa"), sandbox("sb")]);
    // nothing was removed
    assert!(paths::project_record(&home, "shop", "api").is_file());
    let _ = std::fs::remove_dir_all(&home);
}
