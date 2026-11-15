//! The call log from the `layerfs` command (#659): its lines name the
//! client `cli`, `LAYERFS_LOGS_TAG` lands in `tag`, `layerfs logs list` and
//! `layerfs logs reset` answer as JSON like every call, and a usage error
//! (exit 2, nothing called) writes no line. And `layerfs logs machine` and
//! `layerfs logs sandbox` (layerfs-project#178): their usage, a Sandbox's
//! daemon log read by cursor on a native home, their refusals, and no
//! call-log line.
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

/// A native Sandbox `s1` as its home records it, with `log` as the
/// `daemon.log` of its run folder; answers the folder.
fn native_sandbox(home: &Path, log: &str) -> std::path::PathBuf {
    use layerfs_api::home::{paths, write_record};
    let t = |text: &str| SdkValue::text(text);
    let folder = paths::run_root(home).join("layerfs-0a1b2c3d-s1");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("daemon.log"), log).unwrap();
    let daemon = SdkValue::map([
        ("name", t("layerfs-0a1b2c3d-s1")),
        ("runtime", t("native")),
        ("directory", t(&folder.display().to_string())),
    ]);
    write_record(
        &paths::sandbox_record(home, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t("s1")),
            ("runtime", t("native")),
            ("daemon", daemon),
        ]),
    )
    .unwrap();
    folder
}

fn run_cli(home: &Path, words: &[&str]) -> (Option<i32>, String, String) {
    let mut command = layerfs(home, words);
    command
        .env_remove("LAYERFS_LOGS")
        .env_remove("LAYERFS_LOGS_TAG");
    let out = run(&mut command);
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn the_cli_reads_a_sandbox_s_daemon_log_by_cursor_and_logs_no_line() {
    let home = tempdir("logs-inspect");
    // the home's first call: it is logged, as no `logs` call is
    assert_eq!(cli(&home, &["sandbox", "list"], None).0, Some(0));
    // help: both calls, with the cursor window
    for (call, usage) in [
        (
            "machine",
            "layerfs logs machine <machine> [--cursor <cursor>] [--before <before>] [--limit <limit>]\n",
        ),
        (
            "sandbox",
            "layerfs logs sandbox <sandbox> [--cursor <cursor>] [--before <before>] [--limit <limit>]\n",
        ),
    ] {
        let (code, out, _) = run_cli(&home, &["help", "logs", call]);
        assert_eq!(code, Some(0));
        assert!(out.starts_with(usage), "{out}");
        let (_, family, _) = run_cli(&home, &["help", "logs"]);
        assert!(family.contains(usage), "{family}");
    }
    // a wrong command line is exit 2 and calls nothing
    for usage in [
        &["logs", "machine"][..],
        &["logs", "sandbox"],
        &["logs", "sandbox", "s1", "--limit", "many"],
        &["logs", "machine", "native", "--bogus"],
    ] {
        assert_eq!(run_cli(&home, usage).0, Some(2), "{usage:?}");
    }

    let folder = native_sandbox(
        &home,
        "daemon up\nlistening on 127.0.0.1:4100\n\x1b[31mwarn\x1b[0m\ttab\n",
    );
    let (code, out, err) = run_cli(&home, &["logs", "sandbox", "s1"]);
    assert_eq!(code, Some(0), "{err}");
    let tail = json::parse(out.trim()).unwrap();
    let texts = |answer: &SdkValue| -> Vec<String> {
        let Some(SdkValue::List(lines)) = answer.get("lines") else {
            panic!("{answer:?}")
        };
        lines
            .iter()
            .map(|line| line.get_text("text").unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        texts(&tail),
        [
            "daemon up",
            "listening on 127.0.0.1:4100",
            "\\u{1b}[31mwarn\\u{1b}[0m\\ttab"
        ]
    );
    assert_eq!(tail.get("has_more"), Some(&SdkValue::Bool(false)));
    assert!(tail.get("before").is_none(), "{out}");
    let cursor = tail.get_text("cursor").unwrap().to_owned();
    // the daemon writes on; the cursor follows, one line at a time
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(folder.join("daemon.log"))
        .unwrap();
    std::io::Write::write_all(&mut log, b"mounted /w\nunmounted /w\n").unwrap();
    let (code, out, err) = run_cli(
        &home,
        &["logs", "sandbox", "s1", "--cursor", &cursor, "--limit", "1"],
    );
    assert_eq!(code, Some(0), "{err}");
    let next = json::parse(out.trim()).unwrap();
    assert_eq!(texts(&next), ["mounted /w"]);
    assert_eq!(next.get("has_more"), Some(&SdkValue::Bool(true)));
    // and back from the first answer's cursor
    let (code, out, err) = run_cli(
        &home,
        &["logs", "sandbox", "s1", "--before", &cursor, "--limit", "2"],
    );
    assert_eq!(code, Some(0), "{err}");
    let earlier = json::parse(out.trim()).unwrap();
    assert_eq!(texts(&earlier).len(), 2);
    assert_eq!(earlier.get("has_more"), Some(&SdkValue::Bool(true)));

    // refusals are JSON on stderr, exit 125
    for (words, code) in [
        (&["logs", "machine", "native"][..], "Unsupported"),
        (&["logs", "machine", "nobody"], "NotFound"),
        (&["logs", "sandbox", "nobody"], "NoSuchSandbox"),
        (&["logs", "sandbox", "s1", "--limit", "0"], "InvalidInput"),
        (
            &["logs", "sandbox", "s1", "--cursor", "c1.0.1.2.3"],
            "InvalidInput",
        ),
        (
            &[
                "logs", "sandbox", "s1", "--cursor", &cursor, "--before", &cursor,
            ],
            "InvalidInput",
        ),
    ] {
        let (exit, _, err) = run_cli(&home, words);
        assert_eq!(exit, Some(125), "{words:?}: {err}");
        assert!(err.contains(&format!("\"{code}\"")), "{words:?}: {err}");
    }
    // none of it wrote a call-log line: only `sandbox list` did
    assert_eq!(lines(&home).len(), 1);
    std::fs::remove_dir_all(home).unwrap();
}
