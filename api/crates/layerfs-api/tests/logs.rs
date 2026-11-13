//! The call log (#659): one line per call in `trace/calls.jsonl`, its
//! `argv` rendered from the table before the scope rewrites the arguments,
//! free text and credentials as byte counts, the saved version a result
//! names, and `logs.list` / `logs.reset`. The environment is shared by the
//! test threads, so every test here takes one lock.
use layerfs_api::Scope;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static ENV: Mutex<()> = Mutex::new(());
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The test's lock, with the log settings cleared.
fn serial() -> MutexGuard<'static, ()> {
    let guard = ENV.lock().unwrap_or_else(|poison| poison.into_inner());
    std::env::remove_var("LAYERFS_LOGS");
    std::env::remove_var("LAYERFS_LOGS_TAG");
    guard
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-logs-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(args: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    )
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, String> {
    layerfs_api::call(Some(home), name, map(args), &mut |_| {}).map_err(|r| r.code)
}

fn scoped(home: &Path, scope: &Scope, name: &str, args: &[(&str, SdkValue)]) {
    let _ = layerfs_api::call_scoped(
        Some(home),
        Some(scope),
        name,
        map(args),
        &mut |_| {},
        layerfs_api::Client::Rust,
    );
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|code| panic!("{name}: {code}"))
}

/// A new home, opened.
fn home() -> PathBuf {
    let home = tempdir("home");
    layerfs_api::home::open(&home).unwrap();
    home
}

/// A home with project `shop/api` (one file, `README.md`).
fn project() -> (PathBuf, PathBuf) {
    let home = home();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[p(), ("folder", t(&folder.display().to_string()))],
    );
    (home, folder)
}

fn p() -> (&'static str, SdkValue) {
    ("project", t("shop/api"))
}

/// Every line of the call log, oldest first.
fn lines(home: &Path) -> Vec<SdkValue> {
    match std::fs::read_to_string(paths::calls_file(home)) {
        Ok(text) => text
            .lines()
            .map(|line| layerfs_api::json::parse(line).unwrap())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn last(home: &Path) -> SdkValue {
    lines(home).pop().expect("a line")
}

fn argv(line: &SdkValue) -> Vec<String> {
    match line.get("argv") {
        Some(SdkValue::List(words)) => words
            .iter()
            .map(|w| match w {
                SdkValue::Text(text) => text.clone(),
                other => panic!("argv word {other:?}"),
            })
            .collect(),
        other => panic!("no argv: {other:?}"),
    }
}

fn bytes(n: usize) -> String {
    format!("<{n} bytes>")
}

#[test]
fn one_line_per_call_for_a_success_and_each_refusal() {
    let _env = serial();
    let home = home();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let line = last(&home);
    assert_eq!(argv(&line), ["store", "create", "shop"]);
    assert_eq!(line.get_int("v"), Some(1));
    assert_eq!(line.get_text("outcome"), Some("ok"));
    assert_eq!(line.get_text("client"), Some("rust"));
    assert!(line.get_int("at").unwrap() > 1_700_000_000_000);
    assert!(line.get_int("ms").unwrap() >= 0);
    assert!(line.get("tag").is_none() && line.get("commit").is_none());
    // refused before its arguments are checked: [family, call] only
    assert_eq!(
        call(&home, "fs.read_all", &[("path", t("/x"))]),
        Err("InvalidInput".into())
    );
    let line = last(&home);
    assert_eq!(argv(&line), ["fs", "read-all"]);
    assert_eq!(line.get_text("outcome"), Some("InvalidInput"));
    assert!(call(&home, "store.get", &[("bogus", t("x"))]).is_err());
    assert_eq!(argv(&last(&home)), ["store", "get"]);
    // refused after the check: the whole argv
    let code = call(&home, "store.get", &[("name", t("nope"))]).unwrap_err();
    let line = last(&home);
    assert_eq!(argv(&line), ["store", "get", "nope"]);
    assert_eq!(line.get_text("outcome"), Some(code.as_str()));
    assert_eq!(lines(&home).len(), 4);
    let mode = std::fs::metadata(paths::calls_file(&home))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let mode = std::fs::metadata(paths::trace_dir(&home))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn argv_has_positionals_options_flags_and_the_scope_before_it_rewrites() {
    let _env = serial();
    let (home, folder) = project();
    let _ = call(
        &home,
        "file.diff",
        &[
            p(),
            ("left_branch", t("main")),
            ("left_path", t("/README.md")),
            ("right_branch", t("main")),
            ("right_path", t("/README.md")),
            ("allow_missing", SdkValue::Bool(true)),
            ("include_attribution", SdkValue::Bool(false)),
            ("page_size", SdkValue::Int(5)),
        ],
    );
    assert_eq!(
        argv(&last(&home)),
        [
            "file",
            "diff",
            "shop/api",
            "main",
            "/README.md",
            "main",
            "/README.md",
            "--allow-missing",
            "--page-size",
            "5"
        ]
    );
    // optional positionals stop at the first absent one
    let _ = call(&home, "backup.list", &[]);
    assert_eq!(argv(&last(&home)), ["backup", "list"]);
    // the scope composes Branch names and prefixes exec commands; the line
    // shows what the caller asked for
    let scope = Scope::parse(&map(&[
        ("project", t("shop/api")),
        ("folder", t("/web")),
        ("branch", t("agent")),
    ]))
    .unwrap();
    scoped(
        &home,
        &scope,
        "branch.fork",
        &[p(), ("name", t("x")), ("from_branch", t("main"))],
    );
    let scope_words = [
        "--scope",
        "/web",
        "--scope-project",
        "shop/api",
        "--scope-branch",
        "agent",
    ];
    let mut want = vec!["branch", "fork", "shop/api", "x", "main"];
    want.extend(scope_words);
    assert_eq!(argv(&last(&home)), want);
    scoped(
        &home,
        &scope,
        "workspace.exec",
        &[("ws", t("none")), ("command", t("make test"))],
    );
    let mut want = vec!["workspace".to_owned(), "exec".into(), "none".into()];
    want.push(bytes("make test".len()));
    want.extend(scope_words.map(str::to_owned));
    assert_eq!(argv(&last(&home)), want);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn free_text_credentials_content_and_json_are_byte_counts() {
    let _env = serial();
    let (home, folder) = project();
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("dev")), ("from_branch", t("main"))],
    );
    let body = "x".repeat(5321);
    ok(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("dev")),
            ("path", t("/a.ts")),
            ("content", t(&body)),
            ("message", t("add a")),
        ],
    );
    assert_eq!(
        argv(&last(&home)),
        [
            "file".to_owned(),
            "write".into(),
            "shop/api".into(),
            "dev".into(),
            "/a.ts".into(),
            bytes(5321),
            "--message".into(),
            bytes(5)
        ]
    );
    let _ = call(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("main")),
            ("path", t("/b")),
            ("content", SdkValue::Bytes(vec![0xff; 7])),
        ],
    );
    assert_eq!(argv(&last(&home))[5], bytes(7));
    // text is cut at 200 characters
    let long = format!("/{}", "é".repeat(300));
    let _ = call(
        &home,
        "file.stat",
        &[p(), ("branch", t("main")), ("path", t(&long))],
    );
    let path = &argv(&last(&home))[4];
    assert_eq!(path.chars().count(), 201);
    assert!(path.ends_with('…'));
    // each named text argument
    let _ = call(
        &home,
        "file.edit",
        &[
            p(),
            ("branch", t("main")),
            ("path", t("/missing")),
            ("old", t("ab")),
            ("new", t("cde")),
        ],
    );
    let words = argv(&last(&home));
    assert_eq!(&words[5..], ["--old", &bytes(2), "--new", &bytes(3)]);
    let _ = call(
        &home,
        "store.connect",
        &[
            ("name", t("pg")),
            ("url", t("postgres://u:secret@db/x")),
            ("password", t("hunter2")),
        ],
    );
    let words = argv(&last(&home));
    assert_eq!(
        words,
        [
            "store".to_owned(),
            "connect".into(),
            "pg".into(),
            bytes(24),
            "--password".into(),
            bytes(7)
        ]
    );
    let _ = call(
        &home,
        "workspace.exec",
        &[("ws", t("none")), ("command", t("echo secret"))],
    );
    assert_eq!(argv(&last(&home))[3], bytes(11));
    // json, encoded length
    let changes = SdkValue::List(vec![map(&[("op", t("rm")), ("path", t("/b"))])]);
    let encoded = layerfs_api::json::print(&changes).len();
    let _ = call(
        &home,
        "file.apply",
        &[p(), ("branch", t("main")), ("changes", changes)],
    );
    assert_eq!(argv(&last(&home))[4], bytes(encoded));
    let text = std::fs::read_to_string(paths::calls_file(&home)).unwrap();
    for secret in ["secret", "hunter2", "make", "add a"] {
        assert!(!text.contains(secret), "{secret} logged");
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

fn hex66(line: &SdkValue, field: &str) -> String {
    let id = line
        .get_text(field)
        .unwrap_or_else(|| panic!("{field}: {line:?}"));
    assert_eq!(id.len(), 66, "{id}");
    id.to_owned()
}

#[test]
fn saved_versions_are_logged_for_saving_calls_only() {
    let _env = serial();
    let (home, folder) = project();
    let write = |content: &str| {
        ok(
            &home,
            "file.write",
            &[
                p(),
                ("branch", t("work")),
                ("path", t("/f")),
                ("content", t(content)),
            ],
        )
    };
    // Forked: a new Branch has no head, so its base Layer
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("work")), ("from_branch", t("main"))],
    );
    let line = last(&home);
    assert_eq!(line.get_text("kind"), Some("Forked"));
    assert!(line.get("commit").is_none());
    hex66(&line, "layer");
    // Committed: commit.id
    let answer = write("one\n");
    let line = last(&home);
    assert_eq!(line.get_text("kind"), Some("Committed"));
    assert_eq!(hex66(&line, "commit"), answer.get_text("head").unwrap());
    assert!(line.get("layer").is_none());
    // UpToDate: the head
    let answer = write("one\n");
    let line = last(&home);
    assert_eq!(line.get_text("kind"), Some("UpToDate"));
    assert_eq!(hex66(&line, "commit"), answer.get_text("head").unwrap());
    // Forked from a Branch with a head: the head
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("side")), ("from_branch", t("work"))],
    );
    assert_eq!(last(&home).get_text("kind"), Some("Forked"));
    hex66(&last(&home), "commit");
    // Merged: the head
    ok(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("side")),
            ("path", t("/g")),
            ("content", t("g\n")),
        ],
    );
    let answer = ok(
        &home,
        "branch.merge",
        &[p(), ("name", t("both")), ("a", t("work")), ("b", t("side"))],
    );
    let line = last(&home);
    assert_eq!(line.get_text("kind"), Some("Merged"));
    assert_eq!(hex66(&line, "commit"), answer.get_text("head").unwrap());
    // Added: published_layer.id; again: UpToDate with its layer
    let answer = ok(&home, "branch.publish", &[p(), ("branch", t("work"))]);
    let line = last(&home);
    assert_eq!(line.get_text("kind"), Some("Added"));
    let published = answer.get("published_layer").unwrap().get_text("id");
    assert_eq!(Some(hex66(&line, "layer").as_str()), published);
    assert!(line.get("commit").is_none());
    let answer = ok(&home, "branch.publish", &[p(), ("branch", t("work"))]);
    let line = last(&home);
    match answer.get_text("kind") {
        Some("UpToDate") => assert_eq!(
            Some(hex66(&line, "layer").as_str()),
            answer.get_text("layer")
        ),
        Some("NoChanges") => assert_eq!(
            Some(hex66(&line, "layer").as_str()),
            answer.get_text("head_layer")
        ),
        other => panic!("publish again: {other:?}"),
    }
    // a read names no version
    ok(&home, "branch.get", &[p(), ("branch", t("work"))]);
    let line = last(&home);
    assert!(line.get("commit").is_none() && line.get("layer").is_none());
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_tag_comes_from_the_environment_or_the_call() {
    let _env = serial();
    let home = home();
    std::env::set_var("LAYERFS_LOGS_TAG", "dsh:s_1:r1");
    ok(&home, "store.list", &[]);
    assert_eq!(last(&home).get_text("tag"), Some("dsh:s_1:r1"));
    // log_tag overrides it and never reaches the table check
    ok(&home, "store.list", &[("log_tag", t("dsh:s_1:r2"))]);
    assert_eq!(last(&home).get_text("tag"), Some("dsh:s_1:r2"));
    // an oversized tag or one with control characters is dropped
    ok(&home, "store.list", &[("log_tag", t(&"x".repeat(129)))]);
    assert!(last(&home).get("tag").is_none());
    std::env::set_var("LAYERFS_LOGS_TAG", "bad\ntag");
    ok(&home, "store.list", &[]);
    assert!(last(&home).get("tag").is_none());
    std::env::set_var("LAYERFS_LOGS_TAG", "x".repeat(128));
    ok(&home, "store.list", &[]);
    assert_eq!(last(&home).get_text("tag").map(str::len), Some(128));
    // the JSON form takes it too, and names its client
    std::env::remove_var("LAYERFS_LOGS_TAG");
    let answer = layerfs_api::call_json(
        Some(&home),
        None,
        "store.list",
        r#"{"log_tag":"py:1"}"#,
        &mut |_| {},
        layerfs_api::Client::Python,
    );
    assert!(answer.starts_with(r#"{"value""#), "{answer}");
    let line = last(&home);
    assert_eq!(line.get_text("tag"), Some("py:1"));
    assert_eq!(line.get_text("client"), Some("python"));
    // bad JSON is refused before any check and still logged
    layerfs_api::call_json(
        Some(&home),
        None,
        "store.list",
        "{",
        &mut |_| {},
        layerfs_api::Client::Node,
    );
    let line = last(&home);
    assert_eq!(argv(&line), ["store", "list"]);
    assert_eq!(line.get_text("client"), Some("node"));
    assert_eq!(line.get_text("outcome"), Some("InvalidInput"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_logs_calls_and_logs_off_write_nothing() {
    let _env = serial();
    let home = home();
    ok(&home, "logs.list", &[]);
    ok(&home, "logs.reset", &[]);
    assert!(lines(&home).is_empty());
    std::env::set_var("LAYERFS_LOGS", "0");
    ok(&home, "store.list", &[]);
    assert!(!paths::calls_file(&home).exists());
    std::env::remove_var("LAYERFS_LOGS");
    ok(&home, "store.list", &[]);
    assert_eq!(lines(&home).len(), 1);
    // a folder that is not a home gets no log
    let bare = tempdir("bare");
    let _ = call(&bare, "fs.read", &[]);
    assert!(!paths::trace_dir(&bare).exists());
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(bare).unwrap();
}

#[test]
fn eight_threads_appending_leave_whole_lines() {
    let _env = serial();
    let home = home();
    std::thread::scope(|threads| {
        for thread in 0..8 {
            let home = &home;
            threads.spawn(move || {
                for i in 0..200 {
                    let line = map(&[
                        ("v", SdkValue::Int(1)),
                        ("argv", SdkValue::List(vec![t(&"w".repeat(300))])),
                        ("n", SdkValue::Int(thread * 1000 + i)),
                    ]);
                    layerfs_home::logs::append(home, &line);
                }
                ok(home, "store.list", &[]);
            });
        }
    });
    let all = lines(&home);
    assert_eq!(all.len(), 8 * 201);
    let page = ok(&home, "logs.list", &[("limit", SdkValue::Int(1000))]);
    assert_eq!(page.get_int("skipped"), Some(0));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn the_log_rotates_past_16_mib_and_reset_clears_both_files() {
    let _env = serial();
    let home = home();
    std::fs::create_dir_all(paths::trace_dir(&home)).unwrap();
    let filler = format!("{}\n", r#"{"v":1,"argv":["old"],"outcome":"ok"}"#);
    let count = (16 << 20) / filler.len() + 1;
    std::fs::write(paths::calls_file(&home), filler.repeat(count)).unwrap();
    ok(&home, "store.list", &[]);
    let rotated = std::fs::read_to_string(paths::calls_rotated(&home)).unwrap();
    assert_eq!(rotated.lines().count(), count + 1);
    assert!(!paths::calls_file(&home).exists());
    ok(&home, "store.list", &[]);
    assert_eq!(lines(&home).len(), 1);
    // newest first across both files
    let page = ok(&home, "logs.list", &[("limit", SdkValue::Int(3))]);
    let SdkValue::List(items) = page.get("lines").unwrap() else {
        panic!("no lines")
    };
    let words: Vec<_> = items.iter().map(|l| argv(l)[0].clone()).collect();
    assert_eq!(words, ["store", "store", "old"]);
    let reset = ok(&home, "logs.reset", &[]);
    assert_eq!(reset.get_text("kind"), Some("Reset"));
    assert_eq!(reset.get_int("removed"), Some(count as i64 + 2));
    assert!(!paths::calls_rotated(&home).exists());
    assert_eq!(
        std::fs::metadata(paths::calls_file(&home)).unwrap().len(),
        0
    );
    let reset = ok(&home, "logs.reset", &[]);
    assert_eq!(reset.get_int("removed"), Some(0));
    std::fs::remove_dir_all(home).unwrap();
}

fn page(home: &Path, args: &[(&str, SdkValue)]) -> (Vec<SdkValue>, Option<String>, bool, i64) {
    let page = ok(home, "logs.list", args);
    let SdkValue::List(items) = page.get("lines").unwrap().clone() else {
        panic!("no lines")
    };
    (
        items,
        page.get_text("next").map(str::to_owned),
        page.get("has_more") == Some(&SdkValue::Bool(true)),
        page.get_int("skipped").unwrap(),
    )
}

#[test]
fn logs_list_filters_pages_and_refuses_a_stale_cursor() {
    let _env = serial();
    let (home, folder) = project();
    let since = layerfs_home::home::now_ms();
    for i in 0..5 {
        std::env::set_var("LAYERFS_LOGS_TAG", format!("dsh:s{}:r{i}", i % 2));
        ok(&home, "store.list", &[]);
    }
    std::env::remove_var("LAYERFS_LOGS_TAG");
    let _ = call(&home, "store.get", &[("name", t("nope"))]);
    ok(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/README.md"))],
    );
    // op, project, tag prefix and exact tag, outcome, client, since
    let (items, ..) = page(&home, &[("op", t("store.list"))]);
    assert_eq!(items.len(), 5);
    let (items, ..) = page(&home, &[("op", t("file"))]);
    assert_eq!(items.len(), 1);
    let (items, ..) = page(&home, &[("project", t("shop/api"))]);
    assert_eq!(items.len(), 2, "project.init and file.read");
    let (items, ..) = page(&home, &[("tag", t("dsh:s0:"))]);
    assert_eq!(items.len(), 3);
    let (items, ..) = page(&home, &[("tag", t("dsh:s1:r3"))]);
    assert_eq!(items.len(), 1);
    let (items, ..) = page(&home, &[("tag", t("dsh:s1"))]);
    assert!(items.is_empty(), "a tag without `:` is exact");
    let (items, ..) = page(&home, &[("outcome", t("NotFound"))]);
    assert_eq!(items.len(), 1);
    let (items, ..) = page(&home, &[("client", t("cli"))]);
    assert!(items.is_empty());
    let (items, ..) = page(&home, &[("since", SdkValue::Int(since))]);
    assert_eq!(items.len(), 7);
    // newest first, in pages
    let (first, next, more, _) = page(&home, &[("limit", SdkValue::Int(3))]);
    assert!(more);
    assert_eq!(argv(&first[0])[1], "read");
    let next = next.unwrap();
    let (second, _, more, _) = page(&home, &[("limit", SdkValue::Int(3)), ("cursor", t(&next))]);
    assert!(more);
    assert_eq!(second[0].get_text("tag"), Some("dsh:s1:r3"));
    let (all, next_all, more, _) = page(&home, &[]);
    assert_eq!(all.len(), 9);
    assert!(!more && next_all.is_none());
    assert_eq!(all[3], second[0]);
    for (args, why) in [
        (vec![("limit", SdkValue::Int(0))], "limit 0"),
        (vec![("limit", SdkValue::Int(1001))], "limit 1001"),
        (vec![("cursor", t("bogus"))], "a malformed cursor"),
    ] {
        assert_eq!(
            call(&home, "logs.list", &args),
            Err("InvalidInput".into()),
            "{why}"
        );
    }
    // a torn last line is skipped and counted
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(paths::calls_file(&home))
        .unwrap();
    std::io::Write::write_all(&mut file, br#"{"v":1,"argv":["sto"#).unwrap();
    drop(file);
    let (items, _, _, skipped) = page(&home, &[]);
    assert_eq!((items.len(), skipped), (9, 1));
    // after a reset the cursor is stale
    ok(&home, "logs.reset", &[]);
    ok(&home, "store.list", &[]);
    ok(&home, "store.list", &[]);
    ok(&home, "store.list", &[]);
    ok(&home, "store.list", &[]);
    assert_eq!(
        call(&home, "logs.list", &[("cursor", t(&next))]),
        Err("InvalidInput".into())
    );
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
