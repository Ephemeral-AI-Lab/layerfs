//! The call log from the `layerfs` command (#659): its lines name the
//! client `cli`, `LAYERFS_LOGS_TAG` lands in `tag`, `layerfs home log` and
//! `layerfs home clear-log` (the call surface 2.0's names for `logs list`
//! and `logs reset`) answer as JSON like every call, and a usage error
//! (exit 2, nothing called) writes no line. And `layerfs sandbox log`
//! (`logs sandbox` before the call surface 2.0, layerfs-project#178): its
//! usage, a Sandbox's daemon log read newest page first by cursor on a
//! native home, its refusals, and no call-log line. And `layerfs sandbox
//! files` (#178 step 3), a quiet call: its usage, a Sandbox's roots, a
//! folder page, a file window, its refusals, and no call-log line either. And `layerfs machine files`
//! (#178 step 4), quiet too: the roots of `native`, a page, a window, and
//! a path outside its LayerFS folders refused.
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
fn the_cli_logs_its_calls_and_reads_and_clears_them() {
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
        &["home", "log", "--limit", "many"],
        // the old names are gone, with no alias
        &["logs", "list"],
        &["logs", "reset"],
    ] {
        assert_eq!(cli(&home, usage, None).0, Some(2), "{usage:?}");
    }
    assert_eq!(lines(&home).len(), 2);
    // home log: newest first, filtered, as JSON; it writes no line. The
    // short form is the row's three facts and what went wrong
    let (code, out) = cli(&home, &["home", "log", "--call", "store.get"], None);
    assert_eq!(code, Some(0), "{out}");
    let page = json::parse(out.trim()).unwrap();
    let Some(SdkValue::List(items)) = page.get("items") else {
        panic!("{out}")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_text("call"), Some("store.get"));
    assert_eq!(items[0].get_text("outcome"), Some("refused"));
    assert_eq!(items[0].get_text("error"), Some("NotFound"));
    assert!(items[0].get_text("logged_at").unwrap().ends_with('Z'));
    assert!(items[0].get("argv").is_none() && page.get("next_cursor").is_none());
    // the long form has every key of the row and of the page
    let long = ["home", "log", "--tag", "t:", "--limit", "5", "--detail"];
    let (code, out) = cli(&home, &long, None);
    assert_eq!(code, Some(0), "{out}");
    let page = json::parse(out.trim()).unwrap();
    assert_eq!(page.get("next_cursor"), Some(&SdkValue::Null));
    assert_eq!(page.get_int("skipped"), Some(0));
    let Some(SdkValue::List(items)) = page.get("items") else {
        panic!("{out}")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_text("tag"), Some("t:1"));
    assert_eq!(items[0].get_text("client"), Some("cli"));
    assert_eq!(words(&items[0]), ["store", "get", "nope"]);
    assert_eq!(lines(&home).len(), 2);
    // a section the call does not declare is a wrong command line
    let (code, out) = cli(&home, &["home", "log", "--with", "tree"], None);
    assert_eq!((code, out.as_str()), (Some(2), ""));
    // home clear-log
    let (code, out) = cli(&home, &["home", "clear-log"], None);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(out.trim(), r#"{"ok":true}"#);
    assert!(lines(&home).is_empty());
    let (code, out) = cli(&home, &["home", "clear-log", "--detail"], None);
    assert_eq!(code, Some(0), "{out}");
    assert_eq!(
        out.trim(),
        r#"{"ok":true,"outcome":"cleared","line_count":0}"#
    );
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
    // the home's first call: it is logged, as `sandbox log` is not
    assert_eq!(cli(&home, &["sandbox", "list"], None).0, Some(0));
    // help: the call, with its cursor
    let usage = "layerfs sandbox log <sandbox> [--limit 200] [--cursor C]\n";
    let (code, out, _) = run_cli(&home, &["help", "sandbox", "log"]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with(usage), "{out}");
    let (_, family, _) = run_cli(&home, &["help", "sandbox"]);
    assert!(family.contains(usage), "{family}");
    // a wrong command line is exit 2 and calls nothing; the old name is
    // gone, with no alias
    for usage in [
        &["sandbox", "log"][..],
        &["sandbox", "log", "s1", "--limit", "many"],
        &["sandbox", "log", "s1", "--bogus"],
        &["logs", "sandbox", "s1"],
    ] {
        assert_eq!(run_cli(&home, usage).0, Some(2), "{usage:?}");
    }

    native_sandbox(
        &home,
        "daemon up\nlistening on 127.0.0.1:4100\n\x1b[31mwarn\x1b[0m\ttab\n",
    );
    let read = |words: &[&str]| {
        let mut line = vec!["sandbox", "log", "s1"];
        line.extend(words);
        let (code, out, err) = run_cli(&home, &line);
        assert_eq!(code, Some(0), "{words:?}: {err}");
        json::parse(out.trim()).unwrap()
    };
    let texts = |answer: &SdkValue| -> Vec<String> {
        let Some(SdkValue::List(lines)) = answer.get("items") else {
            panic!("{answer:?}")
        };
        lines
            .iter()
            .map(|line| line.get_text("text").unwrap().to_owned())
            .collect()
    };
    // the last lines, oldest first; the whole log, so no cursor
    let tail = read(&[]);
    assert_eq!(
        texts(&tail),
        [
            "daemon up",
            "listening on 127.0.0.1:4100",
            "\\u{1b}[31mwarn\\u{1b}[0m\\ttab"
        ]
    );
    assert!(tail.get("next_cursor").is_none(), "{tail:?}");
    // one line a page: `next_cursor` goes back to the lines before it
    let last = read(&["--limit", "1"]);
    assert_eq!(texts(&last), ["\\u{1b}[31mwarn\\u{1b}[0m\\ttab"]);
    let cursor = last.get_text("next_cursor").unwrap().to_owned();
    let earlier = read(&["--cursor", &cursor, "--limit", "1"]);
    assert_eq!(texts(&earlier), ["listening on 127.0.0.1:4100"]);
    let cursor = earlier.get_text("next_cursor").unwrap().to_owned();
    let first = read(&["--cursor", &cursor, "--limit", "1"]);
    assert_eq!(texts(&first), ["daemon up"]);
    assert!(first.get("next_cursor").is_none(), "{first:?}");

    // refusals are JSON on stderr, exit 125
    for (words, code) in [
        (&["sandbox", "log", "nobody"][..], "NotFound"),
        (&["sandbox", "log", "s1", "--limit", "0"], "InvalidInput"),
        (
            &["sandbox", "log", "s1", "--cursor", "c1.0.1.2.3"],
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

#[test]
fn the_cli_reads_a_sandbox_s_files_and_logs_no_line() {
    let home = tempdir("files-inspect");
    // the home's first call: it is logged, as `sandbox files` is not
    assert_eq!(cli(&home, &["sandbox", "list"], None).0, Some(0));
    let usage = "layerfs sandbox files <sandbox> [<path>] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]\n";
    let (code, out, _) = run_cli(&home, &["help", "sandbox", "files"]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with(usage), "{out}");
    let (_, family, _) = run_cli(&home, &["help", "sandbox"]);
    assert!(family.contains(usage), "{family}");
    // a wrong command line is exit 2 and calls nothing
    for usage in [
        &["sandbox", "files"][..],
        &["sandbox", "files", "s1", "--depth", "deep"],
        &["sandbox", "files", "s1", "/a", "/b"],
        &["sandbox", "files", "s1", "--bogus"],
        &["sandbox", "files", "s1", "--offset", "1"],
    ] {
        assert_eq!(run_cli(&home, usage).0, Some(2), "{usage:?}");
    }

    let folder = native_sandbox(&home, "listening\n");
    let tmp = folder.join("tmp");
    std::fs::create_dir_all(tmp.join("src")).unwrap();
    std::fs::write(tmp.join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.join("src/b.rs"), "fn b() {}\n").unwrap();
    let read = |words: &[&str]| {
        let mut line = vec!["sandbox", "files", "s1"];
        line.extend(words);
        let (code, out, err) = run_cli(&home, &line);
        assert_eq!(code, Some(0), "{words:?}: {err}");
        json::parse(out.trim()).unwrap()
    };
    let each = |answer: &SdkValue, field: &str| -> Vec<String> {
        let Some(SdkValue::List(entries)) = answer.get("items") else {
            panic!("{answer:?}")
        };
        entries
            .iter()
            .map(|entry| entry.get_text(field).unwrap_or("-").to_owned())
            .collect()
    };
    // no path: the roots, by role (a long-form key)
    let roots = read(&["--detail"]);
    assert_eq!(each(&roots, "role"), ["home", "log"]);
    let home_path = each(&roots, "path")[0].clone();
    assert_eq!(home_path, tmp.to_str().unwrap());
    // a folder page, continued from its `next_cursor`
    let page = read(&[&home_path, "--depth", "2", "--limit", "2"]);
    let name = |path: &str| format!("{home_path}/{path}");
    assert_eq!(each(&page, "path"), [name("a.txt"), name("src")]);
    assert_eq!(each(&page, "type"), ["file", "dir"]);
    let next = page.get_text("next_cursor").unwrap().to_owned();
    let rest = read(&[&home_path, "--depth", "2", "--cursor", &next]);
    assert_eq!(each(&rest, "path"), [name("src/b.rs")]);
    assert_eq!(rest.get("next_cursor"), None);
    // a file window
    let window = read(&[&name("a.txt"), "--byte-offset", "1", "--byte-length", "3"]);
    assert_eq!(
        json::print(&window),
        r#"{"text":"lph","next_byte_offset":4}"#
    );

    // refusals are JSON on stderr, exit 125
    let log = folder.join("daemon.log").display().to_string();
    for (words, code) in [
        (&["sandbox", "files", "nobody"][..], "NotFound"),
        (&["sandbox", "files", "s1", "/etc/hosts"], "InvalidInput"),
        (&["sandbox", "files", "s1", &log], "InvalidInput"),
        (&["sandbox", "files", "s1", "relative"], "InvalidInput"),
        (&["sandbox", "files", "s1", &name("nothing")], "NotFound"),
        (
            &["sandbox", "files", "s1", &home_path, "--limit", "0"],
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

#[test]
fn the_cli_reads_a_machine_s_files_and_logs_no_line() {
    let home = tempdir("machine-files");
    // the home's first call: it is logged, as `machine files` is not
    assert_eq!(cli(&home, &["sandbox", "list"], None).0, Some(0));
    let usage = "layerfs machine files <machine> [<path>] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]\n";
    let (code, out, _) = run_cli(&home, &["help", "machine", "files"]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with(usage), "{out}");
    let (_, family, _) = run_cli(&home, &["help", "machine"]);
    assert!(family.contains(usage), "{family}");
    // a wrong command line is exit 2 and calls nothing
    for usage in [
        &["machine", "files"][..],
        &["machine", "files", "native", "--depth", "deep"],
        &["machine", "files", "native", "/a", "/b"],
        &["machine", "files", "native", "--bogus"],
    ] {
        assert_eq!(run_cli(&home, usage).0, Some(2), "{usage:?}");
    }

    let folder = native_sandbox(&home, "listening\n");
    for part in ["tmp", "config"] {
        std::fs::create_dir_all(folder.join(part)).unwrap();
    }
    std::fs::write(folder.join("tmp/a.txt"), "alpha\n").unwrap();
    std::fs::write(folder.join("config/x"), "key").unwrap();
    // only the folders that are there are listed: a home with no Store
    // yet has no `store`
    let (_, early, _) = run_cli(&home, &["machine", "files", "native"]);
    assert!(!early.contains("\"stores\""), "{early}");
    for part in ["log", "store"] {
        std::fs::create_dir_all(home.join(part)).unwrap();
    }
    let read = |words: &[&str]| {
        let mut line = vec!["machine", "files", "native"];
        line.extend(words);
        let (code, out, err) = run_cli(&home, &line);
        assert_eq!(code, Some(0), "{words:?}: {err}");
        json::parse(out.trim()).unwrap()
    };
    let each = |answer: &SdkValue, field: &str| -> Vec<String> {
        let Some(SdkValue::List(entries)) = answer.get("items") else {
            panic!("{answer:?}")
        };
        entries
            .iter()
            .map(|entry| entry.get_text(field).unwrap_or("-").to_owned())
            .collect()
    };
    // no path: the LayerFS folders of the home, by role, and no `/`
    let roots = read(&["--detail"]);
    assert_eq!(
        each(&roots, "role"),
        ["stores", "sandbox_records", "run_folders", "logs"]
    );
    let run_root = each(&roots, "path")[2].clone();
    assert_eq!(Path::new(&run_root), folder.parent().unwrap());
    // a folder page, continued from its `next_cursor`
    let run = folder.display().to_string();
    let name = |path: &str| format!("{run}/{path}");
    let page = read(&[&run, "--limit", "2", "--detail"]);
    assert_eq!(each(&page, "path"), [name("config"), name("daemon.log")]);
    assert_eq!(each(&page, "type"), ["dir", "file"]);
    assert_eq!(each(&page, "not_read"), ["restricted", "-"]);
    assert_eq!(each(&page, "role"), ["-", "log"]);
    let next = page.get_text("next_cursor").unwrap().to_owned();
    let rest = read(&[&run, "--cursor", &next, "--depth", "2"]);
    assert!(each(&rest, "path").contains(&name("tmp/a.txt")), "{rest:?}");
    assert_eq!(rest.get("next_cursor"), None);
    // a file window
    let window = read(&[
        &name("tmp/a.txt"),
        "--byte-offset",
        "1",
        "--byte-length",
        "3",
    ]);
    assert_eq!(
        json::print(&window),
        r#"{"text":"lph","next_byte_offset":4}"#
    );

    // refusals are JSON on stderr, exit 125
    for (words, code) in [
        (&["machine", "files", "nobody"][..], "NotFound"),
        (
            &["machine", "files", "native", "/etc/hosts"],
            "InvalidInput",
        ),
        (&["machine", "files", "native", "/"], "InvalidInput"),
        (
            &["machine", "files", "native", &name("config/x")],
            "InvalidInput",
        ),
        (&["machine", "files", "native", "relative"], "InvalidInput"),
        (
            &["machine", "files", "native", &name("nothing")],
            "NotFound",
        ),
        (
            &["machine", "files", "native", &run, "--limit", "0"],
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
