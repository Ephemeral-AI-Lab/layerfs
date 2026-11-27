//! One machine record that cannot be read fails no call about anything else
//! (RF re-check N-D): `machine list` shows that machine as one item,
//! `broken` `failed` with its doctor as the fix; a get of another machine,
//! the Store, project, Sandbox and Workspace lists answer as usual; a Store
//! on that machine is `blocked` by it; and a get of that machine is refused
//! `Failed` with the same fix.
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::json::parse;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn call(home: &Path, words: &[&str]) -> (Option<i32>, SdkValue) {
    let out = run(&mut layerfs(home, words));
    let shown = match out.status.code() {
        Some(0) => text(&out.stdout),
        _ => text(&out.stderr),
    };
    let value = parse(shown.trim()).unwrap_or_else(|_| panic!("{words:?}: {shown}"));
    (out.status.code(), value)
}

fn items(value: &SdkValue) -> Vec<SdkValue> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => panic!("{value:?}"),
    }
}

#[test]
fn an_unreadable_machine_record_blocks_only_what_is_on_it() {
    let home = tempdir("unreadable-machine");
    assert_eq!(call(&home, &["home", "init"]).0, Some(0));
    std::fs::write(home.join("machine/docker/record.json"), "not json").unwrap();
    // a sqlite Store recorded on that machine
    let store = home.join("store/s1");
    std::fs::create_dir_all(&store).unwrap();
    let record = r#"{"v":1,"name":"s1","binding":{"hex":"16b61b4396ae6ec797bf6fb24dcff298d82e3bba8bb1e506cd208d77d0ea6346"},"cursor":{"hex":"2b24a49a19734464215d6242fc42d723a162ca12d94d97e1c2c56ca53a45cf5e"},"kind":"sqlite","machine":"docker","profile":"disposable","created":1795043232865,"sqlite":"3.53.2"}"#;
    std::fs::write(store.join("record.json"), record).unwrap();

    let (code, listed) = call(&home, &["machine", "list"]);
    assert_eq!(code, Some(0), "{listed:?}");
    let listed = items(&listed);
    let docker = (listed.iter())
        .find(|item| item.get_text("machine") == Some("docker"))
        .unwrap_or_else(|| panic!("{listed:?}"));
    assert_eq!(docker.get_text("state"), Some("broken"));
    assert_eq!(docker.get_text("reason"), Some("failed"));
    assert_eq!(
        docker.get_text("fix"),
        Some("layerfs machine doctor docker")
    );
    assert!(listed
        .iter()
        .any(|item| item.get_text("machine") == Some(layerfs_api::names::HOST_MACHINE)));

    let (code, native) = call(&home, &["machine", "get", layerfs_api::names::HOST_MACHINE]);
    assert_eq!((code, native.get_text("state")), (Some(0), Some("ready")));

    let (code, refused) = call(&home, &["machine", "get", "docker"]);
    assert_eq!(code, Some(125), "{refused:?}");
    assert_eq!(refused.get_text("error"), Some("Failed"));
    assert_eq!(
        refused.get_text("fix"),
        Some("layerfs machine doctor docker")
    );

    let (code, stores) = call(&home, &["store", "list"]);
    assert_eq!(code, Some(0), "{stores:?}");
    let s1 = items(&stores).pop().unwrap();
    assert_eq!(s1.get_text("state"), Some("blocked"), "{s1:?}");
    let by = s1.get("blocked_by").unwrap();
    assert_eq!(
        (by.get_text("machine"), by.get_text("reason")),
        (Some("docker"), Some("failed"))
    );
    for words in [
        &["project", "list"][..],
        &["sandbox", "list"],
        &["workspace", "list"],
    ] {
        let (code, value) = call(&home, words);
        assert_eq!(code, Some(0), "{words:?}: {value:?}");
    }
    std::fs::remove_dir_all(home).unwrap();
}
