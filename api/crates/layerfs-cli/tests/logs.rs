//! The call log from the `layerfs` command (#659): its lines name the
//! client `cli`, `LAYERFS_LOGS_TAG` lands in `tag`, `layerfs logs list` and
//! `layerfs logs reset` answer as JSON like every call, and a usage error
//! (exit 2, nothing called) writes no line.
mod common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn cli(home: &Path, words: &[&str], tag: Option<&str>) -> (Option<i32>, String) {
    let mut command = layerfs(home, words);
    command
        .env_remove("LAYERFS_LOGS")
        .env_remove("LAYERFS_LOGS_TAG");
    if let Some(tag) = tag {
        command.env("LAYERFS_LOGS_TAG", tag);
    }
    let out = run(&mut command);
    (out.status.code(), text(&out.stdout))
}

fn lines(home: &Path) -> Vec<SdkValue> {
    std::fs::read_to_string(home.join("trace/calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| json::parse(line).unwrap())
        .collect()
}

fn words(line: &SdkValue) -> Vec<&str> {
    match line.get("argv") {
        Some(SdkValue::List(words)) => words
            .iter()
            .map(|w| match w {
                SdkValue::Text(text) => text.as_str(),
                _ => "",
            })
            .collect(),
        other => panic!("no argv: {other:?}"),
    }
}

#[test]
fn the_cli_logs_its_calls_and_lists_and_resets_them() {
    let home = tempdir("logs");
    assert_eq!(cli(&home, &["store", "create", "shop"], None).0, Some(0));
    assert_eq!(
        cli(&home, &["store", "get", "nope"], Some("t:1")).0,
        Some(125)
    );
    let all = lines(&home);
    assert_eq!(all.len(), 2);
    assert_eq!(words(&all[0]), ["store", "create", "shop"]);
    assert_eq!(all[0].get_text("client"), Some("cli"));
    assert_eq!(all[0].get_text("outcome"), Some("ok"));
    assert!(all[0].get("tag").is_none());
    assert_eq!(words(&all[1]), ["store", "get", "nope"]);
    assert_eq!(all[1].get_text("outcome"), Some("NotFound"));
    assert_eq!(all[1].get_text("tag"), Some("t:1"));
    // usage errors call nothing and write nothing
    for usage in [
        &["store", "get"][..],
        &["store", "list", "--bogus"],
        &["nope", "x"],
        &["logs", "list", "--limit", "many"],
    ] {
        assert_eq!(cli(&home, usage, None).0, Some(2), "{usage:?}");
    }
    assert_eq!(lines(&home).len(), 2);
    // logs list: newest first, filtered, as JSON; it writes no line
    let (code, out) = cli(&home, &["logs", "list", "--op", "store.get"], None);
    assert_eq!(code, Some(0), "{out}");
    let page = json::parse(out.trim()).unwrap();
    let Some(SdkValue::List(items)) = page.get("lines") else {
        panic!("{out}")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], all[1]);
    assert_eq!(page.get("has_more"), Some(&SdkValue::Bool(false)));
    assert_eq!(page.get_int("skipped"), Some(0));
    let (code, out) = cli(
        &home,
        &["logs", "list", "--tag", "t:", "--limit", "5"],
        None,
    );
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("\"t:1\""), "{out}");
    assert_eq!(lines(&home).len(), 2);
    // logs reset
    let (code, out) = cli(&home, &["logs", "reset"], None);
    assert_eq!(code, Some(0), "{out}");
    let reset = json::parse(out.trim()).unwrap();
    assert_eq!(reset.get_text("kind"), Some("Reset"));
    assert_eq!(reset.get_int("removed"), Some(2));
    assert!(lines(&home).is_empty());
    std::fs::remove_dir_all(home).unwrap();
}
