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
    // nothing was overwritten: the short form is the Commit alone, and the
    // long form's count is null, set only beside a cut list (R2 F1)
    assert_eq!(made, SdkValue::map([("commit", t(&at))]), "{made:?}");
    let long = ok(
        &home,
        "file.write",
        &[
            p(),
            dev(),
            ("path", t("/d.txt")),
            ("content", t("d")),
            detail(),
        ],
    );
    assert_eq!(long.get("overwritten"), Some(&SdkValue::List(Vec::new())));
    assert_eq!(long.get("overwritten_count"), Some(&SdkValue::Null));
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
    // a hit's text is its line without the line end (R2 N3)
    let found = ok(&home, "file.grep", &[p(), dev(), ("pattern", t("login"))]);
    for row in items(&found) {
        let text = row.get_text("text").unwrap_or_default();
        assert!(text.contains("login") && !text.ends_with('\n'), "{row:?}");
    }
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

/// A list's `_count` companion is set only when the list was cut at 20
/// (conformance R1 F2): a merge that overwrote nothing answers its Commit
/// alone, and its long form and `commit.get`'s carry null counts.
#[test]
fn a_merge_answers_counts_only_beside_cut_lists() {
    let (home, folder) = project();
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("agent")), ("from", t("dev"))],
    );
    write(&home, "/x.txt", t("x\n"));
    ok(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("agent")),
            ("path", t("/y.txt")),
            ("content", t("y\n")),
        ],
    );
    let merged = ok(&home, "branch.merge", &[p(), dev(), ("from", t("agent"))]);
    let commit = head(&merged);
    assert_eq!(
        merged,
        SdkValue::map([("commit", t(&commit))]),
        "{merged:?}"
    );
    let again = ok(
        &home,
        "branch.merge",
        &[p(), dev(), ("from", t("agent")), detail()],
    );
    assert_eq!(again.get_text("outcome"), Some("unchanged"), "{again:?}");
    for key in ["overwritten_count", "units_count"] {
        assert_eq!(again.get(key), Some(&SdkValue::Null), "{key}: {again:?}");
    }
    let got = ok(
        &home,
        "commit.get",
        &[p(), ("commit", t(&commit)), detail()],
    );
    assert_eq!(
        got.get("overwritten_count"),
        Some(&SdkValue::Null),
        "{got:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// A taken Branch name (conformance R1 N5) is refused `Exists` in a plain
/// sentence, with the call again under another name as its `fix`.
#[test]
fn a_taken_branch_name_says_so_and_offers_another() {
    let (home, folder) = project();
    let taken = refused(&home, "branch.fork", &[p(), ("name", t("dev"))]);
    assert_eq!(taken.code, "Exists", "{taken:?}");
    assert_eq!(
        taken.message, "the name dev is taken by a Branch; choose another name",
        "{taken:?}"
    );
    assert_eq!(fix(&taken), "layerfs branch fork shop/api dev-2");
    let into = [p(), dev(), ("from", t("main")), ("into", t("dev"))];
    let taken = refused(&home, "branch.merge", &into);
    assert_eq!(taken.code, "Exists", "{taken:?}");
    assert_eq!(
        fix(&taken),
        "layerfs branch merge shop/api dev main --into dev-2"
    );
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// `commit.diff --with patch` (conformance R1 F7): every hunk row keeps
/// `op` and `text` in the short form, a blank line's empty `text` too, and
/// an added text file answers its lines as `+` hunks.
#[test]
fn a_patch_keeps_blank_lines_and_covers_added_files() {
    let (home, folder) = project();
    write(&home, "/README.md", t("hello\n\nworld\n"));
    write(&home, "/n.txt", t("x\ny\n"));
    let diff = ok(
        &home,
        "commit.diff",
        &[p(), ("ref", t("dev")), ("with", t("patch"))],
    );
    let rows = items(&diff);
    let hunks = |path: &str| {
        let row = (rows.iter())
            .find(|row| row.get_text("path") == Some(path))
            .unwrap_or_else(|| panic!("{path}: {diff:?}"));
        row.get("hunks").cloned().unwrap_or_default()
    };
    let line = |op: &str, text: &str| SdkValue::map([("op", t(op)), ("text", t(text))]);
    assert_eq!(
        hunks("/README.md"),
        // the Store's lines: every `\n` ends one, the rest is the last
        SdkValue::List(vec![
            line(" ", "hello"),
            line(" ", ""),
            line("+", "world"),
            line("+", ""),
        ]),
        "{diff:?}"
    );
    assert_eq!(
        hunks("/n.txt"),
        SdkValue::List(vec![line("+", "x"), line("+", "y"), line("+", "")]),
        "{diff:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// `file.read`'s short form (conformance R2 F3, F4, F6): `size` only for a
/// file that is not text; `next_line` only while a line is left, so a cut
/// last line has none; `file.edit`'s `replaced` always in the long form,
/// in the short form only past 1.
#[test]
fn reads_answer_size_and_next_line_only_when_they_are_news() {
    let (home, folder) = project();
    let read = |path: &str, extra: &[(&str, SdkValue)]| {
        let mut args = vec![p(), dev(), ("path", t(path))];
        args.extend(extra.iter().cloned());
        ok(&home, "file.read", &args)
    };
    write(&home, "/a.txt", t("one\ntwo\n"));
    let text = read("/a.txt", &[]);
    assert_eq!(text.get("size"), None, "{text:?}");
    assert_eq!(read("/a.txt", &[detail()]).get_int("size"), Some(8));
    write(&home, "/bin", SdkValue::Bytes(vec![0, 255, 1]));
    let binary = read("/bin", &[]);
    assert_eq!(binary.get_int("size"), Some(3), "{binary:?}");
    // one line longer than a reply: cut, and no line after it
    let long = "x".repeat(70_000);
    write(&home, "/long.txt", t(&long));
    let cut = read("/long.txt", &[]);
    assert_eq!(
        cut.get("line_truncated"),
        Some(&SdkValue::Bool(true)),
        "{:?}",
        cut.get("next_line")
    );
    assert_eq!(cut.get("next_line"), None);
    // a cut first line with a second after it: the second is next
    write(&home, "/two.txt", t(&format!("{long}\nsecond\n")));
    let first = read("/two.txt", &[]);
    assert_eq!(
        first.get_int("next_line"),
        Some(2),
        "{:?}",
        first.get("next_line")
    );
    // file.edit: a plain edit replaced one, said only in the long form
    let edit = |path: &str, extra: &[(&str, SdkValue)]| {
        let mut args = vec![
            p(),
            dev(),
            ("path", t(path)),
            ("old_string", t("o")),
            ("new_string", t("0")),
        ];
        args.extend(extra.iter().cloned());
        ok(&home, "file.edit", &args)
    };
    write(&home, "/e.txt", t("one\n"));
    let plain = edit("/e.txt", &[detail()]);
    assert_eq!(plain.get_int("replaced"), Some(1), "{plain:?}");
    write(&home, "/f.txt", t("one\n"));
    let single = edit("/f.txt", &[("replace_all", SdkValue::Bool(true))]);
    assert_eq!(single.get("replaced"), None, "{single:?}");
    write(&home, "/g.txt", t("oo\n"));
    let both = edit("/g.txt", &[("replace_all", SdkValue::Bool(true))]);
    assert_eq!(both.get_int("replaced"), Some(2), "{both:?}");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

/// `--at` names a Commit only (conformance R2 F5): a Branch name or a
/// prefix shorter than 8 is refused `InvalidInput`, on every read.
#[test]
fn at_names_a_commit_never_a_branch() {
    let (home, folder) = project();
    let made = write(&home, "/a.txt", t("a\n"));
    let at = head(&made);
    for name in ["file.read", "file.stat", "file.list", "file.blame"] {
        let path = if name == "file.list" { "/" } else { "/a.txt" };
        let args = |value: &str| vec![p(), dev(), ("path", t(path)), ("at", t(value))];
        for bad in ["main", "dev", &at[..7]] {
            let refusal = refused(&home, name, &args(bad));
            assert_eq!(
                layerfs_home::outcome::public_code(&refusal.code),
                "InvalidInput",
                "{name} --at {bad}: {refusal:?}"
            );
        }
        ok(&home, name, &args(&at[..8]));
        ok(&home, name, &args(&at));
    }
    for name in ["file.glob", "file.grep"] {
        let pattern = if name == "file.glob" { "*.txt" } else { "a" };
        let refusal = refused(
            &home,
            name,
            &[p(), dev(), ("pattern", t(pattern)), ("at", t("main"))],
        );
        assert_eq!(
            layerfs_home::outcome::public_code(&refusal.code),
            "InvalidInput",
            "{name}: {refusal:?}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
