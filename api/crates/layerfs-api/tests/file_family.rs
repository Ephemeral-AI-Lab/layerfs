//! The `file` family on the call surface 2.0 (package F-file): reads at a
//! Branch's head or at `--at` (an id or a prefix), a file that is not text
//! answered as `binary` and its `size`, octal modes, the head guard
//! `--expected`, `edit --replace-all`, `rm --recursive`, `apply` with the
//! single calls' field names, and cursor pages that carry their filters
//! and the Commit their first page read. Store calls: Linux only.
use layerfs_api::home;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-file-family-{tag}-{}-{}",
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

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    layerfs_api::call(Some(home), name, map(args), &mut |_| {})
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn refused(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkRefusal {
    match call(home, name, args) {
        Ok(value) => panic!("{name} answered {value:?}"),
        Err(refusal) => refusal,
    }
}

fn items(value: &SdkValue) -> Vec<SdkValue> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("no items: {other:?}"),
    }
}

/// A field the refusal answers flat for the caller's next call.
fn next(refusal: &SdkRefusal, name: &str) -> SdkValue {
    (refusal.detail.get("next"))
        .and_then(|next| next.get(name).cloned())
        .unwrap_or_default()
}

fn fix(refusal: &SdkRefusal) -> String {
    refusal
        .detail
        .get_text("fix")
        .unwrap_or_else(|| panic!("no fix: {refusal:?}"))
        .to_owned()
}

fn p() -> (&'static str, SdkValue) {
    ("project", t("shop/api"))
}

fn dev() -> (&'static str, SdkValue) {
    ("branch", t("dev"))
}

fn detail() -> (&'static str, SdkValue) {
    ("detail", SdkValue::Bool(true))
}

/// A home with project `shop/api` Inited from a folder, and Branch `dev`.
fn project() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("dev")), ("from", t("main"))],
    );
    (home, folder)
}

fn write(home: &Path, path: &str, content: SdkValue) -> SdkValue {
    ok(
        home,
        "file.write",
        &[p(), dev(), ("path", t(path)), ("content", content)],
    )
}

fn head(answer: &SdkValue) -> String {
    let commit = answer
        .get_text("commit")
        .unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(commit.len(), 66, "a full id: {commit}");
    commit.to_owned()
}

#[test]
fn reads_answer_text_binary_windows_and_octal_modes_at_a_head_or_a_prefix() {
    let (home, folder) = project();
    let first = write(&home, "/a.txt", t("one\ntwo\nthree\n"));
    let first = head(&first);
    write(&home, "/a.txt", t("ONE\ntwo\nthree\n"));
    // a window: the second line, then the next one to read
    let window = ok(
        &home,
        "file.read",
        &[
            p(),
            dev(),
            ("path", t("/a.txt")),
            ("line", SdkValue::Int(2)),
            ("limit", SdkValue::Int(1)),
        ],
    );
    assert_eq!(window.get_text("text"), Some("two\n"), "{window:?}");
    assert_eq!(window.get_int("next_line"), Some(3));
    // `--at` by a prefix reads the first write
    let old = ok(
        &home,
        "file.read",
        &[
            p(),
            dev(),
            ("path", t("/a.txt")),
            ("at", t(&first[..10])),
            detail(),
        ],
    );
    assert_eq!(old.get_text("text"), Some("one\ntwo\nthree\n"));
    assert_eq!(old.get_text("commit"), Some(first.as_str()));
    assert_eq!(old.get("next_line"), Some(&SdkValue::Null));
    // a file that is not text: `binary` and `size`, no `text`
    write(&home, "/b.bin", SdkValue::Bytes(vec![0, 159, 146, 150]));
    let binary = ok(&home, "file.read", &[p(), dev(), ("path", t("/b.bin"))]);
    assert_eq!(binary.get("binary"), Some(&SdkValue::Bool(true)));
    assert_eq!(binary.get_int("size"), Some(4));
    assert!(binary.get("text").is_none(), "{binary:?}");
    // stat: octal mode text and a modification time
    let stat = ok(
        &home,
        "file.stat",
        &[p(), dev(), ("path", t("/a.txt")), detail()],
    );
    assert_eq!(stat.get_text("type"), Some("file"));
    assert_eq!(stat.get_text("mode"), Some("0644"));
    assert!(
        stat.get_text("modified_at")
            .is_some_and(|at| at.ends_with('Z')),
        "{stat:?}"
    );
    // a window of more than 2000 lines is refused
    let wide = refused(
        &home,
        "file.read",
        &[
            p(),
            dev(),
            ("path", t("/a.txt")),
            ("limit", SdkValue::Int(2001)),
        ],
    );
    assert_eq!(wide.code, "InvalidInput");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn writes_guard_the_head_and_edit_rm_mkdir_and_apply_take_their_new_options() {
    let (home, folder) = project();
    let made = write(&home, "/a.txt", t("x x x\n"));
    let at = head(&made);
    // a stale `--expected` is refused `Conflict` naming the head
    write(&home, "/b.txt", t("b\n"));
    let stale = refused(
        &home,
        "file.write",
        &[
            p(),
            dev(),
            ("path", t("/c")),
            ("content", t("c")),
            ("expected", t(&at)),
        ],
    );
    assert_eq!(layerfs_home::outcome::public_code(&stale.code), "Conflict");
    let SdkValue::Text(current) = next(&stale, "commit") else {
        panic!("{stale:?}")
    };
    assert_eq!(current.len(), 66, "{stale:?}");
    assert_ne!(current, at);
    // the head it names, by a prefix, lets the write through
    ok(
        &home,
        "file.write",
        &[
            p(),
            dev(),
            ("path", t("/c")),
            ("content", t("c")),
            ("expected", t(&current[..12])),
        ],
    );
    // edit: more than one occurrence needs --replace-all, with a fix
    let edit = |extra: &[(&str, SdkValue)]| {
        let mut args = vec![
            p(),
            dev(),
            ("path", t("/a.txt")),
            ("old_string", t("x")),
            ("new_string", t("yy")),
        ];
        args.extend(extra.iter().cloned());
        call(&home, "file.edit", &args)
    };
    let ambiguous = edit(&[]).unwrap_err();
    assert_eq!(
        layerfs_home::outcome::public_code(&ambiguous.code),
        "InvalidInput"
    );
    assert_eq!(next(&ambiguous, "matches"), SdkValue::Int(3));
    assert!(fix(&ambiguous).ends_with("--replace-all"), "{ambiguous:?}");
    let all = edit(&[("replace_all", SdkValue::Bool(true))]).unwrap();
    assert_eq!(all.get_int("replaced"), Some(3), "{all:?}");
    let read = ok(&home, "file.read", &[p(), dev(), ("path", t("/a.txt"))]);
    assert_eq!(read.get_text("text"), Some("yy yy yy\n"));
    let none = edit(&[]).unwrap_err();
    assert_eq!(layerfs_home::outcome::public_code(&none.code), "NotFound");
    // mkdir with an octal mode; rm of a folder that is not empty
    ok(
        &home,
        "file.mkdir",
        &[p(), dev(), ("path", t("/d/e")), ("mode", t("0750"))],
    );
    let stat = ok(&home, "file.stat", &[p(), dev(), ("path", t("/d/e"))]);
    assert_eq!(stat.get_text("mode"), Some("0750"));
    let full = refused(&home, "file.rm", &[p(), dev(), ("path", t("/d"))]);
    assert_eq!(
        layerfs_home::outcome::public_code(&full.code),
        "InvalidInput"
    );
    assert!(fix(&full).ends_with("--recursive"), "{full:?}");
    ok(
        &home,
        "file.rm",
        &[
            p(),
            dev(),
            ("path", t("/d")),
            ("recursive", SdkValue::Bool(true)),
        ],
    );
    // apply: the single calls' field names, a mode in octal text
    let changes = SdkValue::List(vec![
        map(&[("op", t("mv")), ("from", t("/b.txt")), ("to", t("/m.txt"))]),
        map(&[
            ("op", t("edit")),
            ("path", t("/m.txt")),
            ("old_string", t("b")),
            ("new_string", t("B")),
        ]),
        map(&[
            ("op", t("chmod")),
            ("path", t("/m.txt")),
            ("mode", t("0600")),
        ]),
    ]);
    let applied = ok(&home, "file.apply", &[p(), dev(), ("changes", changes)]);
    head(&applied);
    let stat = ok(&home, "file.stat", &[p(), dev(), ("path", t("/m.txt"))]);
    assert_eq!(stat.get_text("mode"), Some("0600"));
    // nothing to change: unchanged, the head as `commit`
    let same = write(&home, "/c", t("c"));
    assert_eq!(
        same.get("unchanged"),
        Some(&SdkValue::Bool(true)),
        "{same:?}"
    );
    // main is written only through publish: a fix names a fork
    let main = refused(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("main")),
            ("path", t("/z")),
            ("content", t("z")),
        ],
    );
    assert!(
        fix(&main).starts_with("layerfs branch fork shop/api"),
        "{main:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn cursors_carry_their_filters_and_the_commit_their_first_page_read() {
    let (home, folder) = project();
    // a write makes no missing parent folders: `file.mkdir` does
    ok(&home, "file.mkdir", &[p(), dev(), ("path", t("/src"))]);
    ok(
        &home,
        "file.mkdir",
        &[p(), dev(), ("path", t("/node_modules/x"))],
    );
    for (path, body) in [
        ("/src/a.ts", "Login here\n"),
        ("/src/b.js", "login there\n"),
        ("/node_modules/x/c.ts", "login\n"),
    ] {
        write(&home, path, t(body));
    }
    let list = |extra: &[(&str, SdkValue)]| {
        let mut args = vec![p(), dev(), ("path", t("/src")), ("limit", SdkValue::Int(1))];
        args.extend(extra.iter().cloned());
        call(&home, "file.list", &args)
    };
    let first = list(&[]).unwrap();
    let cursor = first.get_text("next_cursor").unwrap().to_owned();
    // a write after the first page: the next page still reads its Commit
    write(&home, "/src/0.ts", t("new\n"));
    let second = list(&[("cursor", t(&cursor))]).unwrap();
    let names: Vec<_> = (items(&first).into_iter().chain(items(&second)))
        .map(|row| row.get_text("path").unwrap().to_owned())
        .collect();
    assert_eq!(names, ["/src/a.ts", "/src/b.js"]);
    assert!(second.get("next_cursor").is_none(), "{second:?}");
    // another path, or another call's cursor, is refused
    let other = refused(
        &home,
        "file.list",
        &[p(), dev(), ("path", t("/")), ("cursor", t(&cursor))],
    );
    assert_eq!(other.code, "InvalidInput");
    let glob = refused(
        &home,
        "file.glob",
        &[p(), dev(), ("pattern", t("*.ts")), ("cursor", t(&cursor))],
    );
    assert_eq!(glob.code, "InvalidInput");
    // glob --exclude leaves a folder out
    let ts = ok(
        &home,
        "file.glob",
        &[
            p(),
            dev(),
            ("pattern", t("*.ts")),
            ("exclude", t("node_modules")),
        ],
    );
    assert_eq!(items(&ts), vec![t("/src/0.ts"), t("/src/a.ts")]);
    // grep: literal, --ignore-case, --include, --exclude
    let grep = |extra: &[(&str, SdkValue)]| {
        let mut args = vec![p(), dev(), ("pattern", t("login"))];
        args.extend(extra.iter().cloned());
        let found = ok(&home, "file.grep", &args);
        let mut paths: Vec<_> = (items(&found).iter())
            .map(|row| row.get_text("path").unwrap().to_owned())
            .collect();
        paths.sort();
        paths
    };
    assert_eq!(grep(&[]), ["/node_modules/x/c.ts", "/src/b.js"]);
    let folded = grep(&[
        ("ignore_case", SdkValue::Bool(true)),
        ("exclude", t("node_modules")),
    ]);
    assert_eq!(folded, ["/src/a.ts", "/src/b.js"]);
    assert_eq!(
        grep(&[
            ("ignore_case", SdkValue::Bool(true)),
            ("include", t("*.ts"))
        ]),
        ["/node_modules/x/c.ts", "/src/a.ts"]
    );
    // blame: runs of lines, each Commit once
    write(&home, "/l.txt", t("1\n2\n3\n"));
    let edited = ok(
        &home,
        "file.edit",
        &[
            p(),
            dev(),
            ("path", t("/l.txt")),
            ("old_string", t("3")),
            ("new_string", t("three")),
        ],
    );
    let blame = ok(&home, "file.blame", &[p(), dev(), ("path", t("/l.txt"))]);
    let ranges = match blame.get("ranges") {
        Some(SdkValue::List(ranges)) => ranges.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(ranges.len(), 2, "{blame:?}");
    assert_eq!(ranges[1].get_int("start"), Some(3));
    assert_eq!(ranges[1].get_text("commit"), edited.get_text("commit"));
    assert!(matches!(blame.get("commits"), Some(SdkValue::List(c)) if c.len() == 2));
    // history: the Commits that changed a path, newest first, by cursor
    let history = ok(
        &home,
        "file.history",
        &[
            p(),
            dev(),
            ("path", t("/l.txt")),
            ("limit", SdkValue::Int(1)),
        ],
    );
    let rows = items(&history);
    assert_eq!(rows[0].get_text("change"), Some("modified"));
    assert_eq!(rows[0].get_text("commit"), edited.get_text("commit"));
    let rest = ok(
        &home,
        "file.history",
        &[
            p(),
            dev(),
            ("path", t("/l.txt")),
            ("cursor", t(history.get_text("next_cursor").unwrap())),
        ],
    );
    assert_eq!(items(&rest)[0].get_text("change"), Some("added"));
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
