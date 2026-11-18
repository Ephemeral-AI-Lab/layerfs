//! The call log from the `layerfs` command (#659): its lines name the
//! client `cli`, `LAYERFS_LOGS_TAG` lands in `tag`, `layerfs home log` and
//! `layerfs home clear-log` (the call surface 2.0's names for `logs list`
//! and `logs reset`) answer as JSON like every call, and a usage error
//! (exit 2, nothing called) writes no line. And `layerfs logs machine` and
//! `layerfs logs sandbox` (layerfs-project#178): their usage, a Sandbox's
//! daemon log read by cursor on a native home, their refusals, and no
//! call-log line. And `layerfs sandbox files` (#178 step 3), a quiet call:
//! its usage, a Sandbox's roots, a folder page, a file window, its
//! refusals, and no call-log line either. And `layerfs machine files`
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
    // a section the call does not declare is refused by the library
    let (code, out) = cli(&home, &["home", "log", "--with", "tree"], None);
    assert_eq!((code, out.as_str()), (Some(125), ""));
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
        (&["logs", "sandbox", "nobody"], "NotFound"),
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

#[test]
fn the_cli_reads_a_sandbox_s_files_and_logs_no_line() {
    let home = tempdir("files-inspect");
    // the home's first call: it is logged, as `sandbox files` is not
    assert_eq!(cli(&home, &["sandbox", "list"], None).0, Some(0));
    let usage = "layerfs sandbox files <sandbox> [<path>] [--depth <depth>] [--cursor <cursor>] [--limit <limit>] [--offset <offset>] [--length <length>]\n";
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
        let Some(SdkValue::List(entries)) = answer.get("entries") else {
            panic!("{answer:?}")
        };
        entries
            .iter()
            .map(|entry| entry.get_text(field).unwrap().to_owned())
            .collect()
    };
    // no path: the roots, by role
    let roots = read(&[]);
    assert_eq!(each(&roots, "role"), ["home", "log"]);
    let home_path = each(&roots, "path")[0].clone();
    assert_eq!(home_path, tmp.to_str().unwrap());
    // a folder page, continued from its `next`
    let page = read(&[&home_path, "--depth", "2", "--limit", "2"]);
    let name = |path: &str| format!("{home_path}/{path}");
    assert_eq!(each(&page, "path"), [name("a.txt"), name("src")]);
    assert_eq!(each(&page, "kind"), ["file", "folder"]);
    assert_eq!(page.get("has_more"), Some(&SdkValue::Bool(true)));
    assert_eq!(page.get_text("reason"), Some("limit"));
    let next = page.get_text("next").unwrap().to_owned();
    let rest = read(&[&home_path, "--depth", "2", "--cursor", &next]);
    assert_eq!(each(&rest, "path"), [name("src/b.rs")]);
    assert_eq!(rest.get("has_more"), Some(&SdkValue::Bool(false)));
    // a file window
    let window = read(&[&name("a.txt"), "--offset", "1", "--length", "3"]);
    assert_eq!(
        json::print(&window),
        r#"{"kind":"file","size":6,"offset":1,"content":"lph","binary":false,"next":4}"#
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
    let usage = "layerfs machine files <machine> [<path>] [--depth <depth>] [--limit <limit>] [--cursor <cursor>] [--byte-offset <byte-offset>] [--byte-length <byte-length>]\n";
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
