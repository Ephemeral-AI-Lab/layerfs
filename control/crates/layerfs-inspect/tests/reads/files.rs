//! The Files walker and its first view, a Sandbox's home
//! (layerfs-project#178 step 3, the inspect kind `sandbox_files`), on the
//! side that holds the Sandbox's run folder. Pages: byte order of name,
//! tree order to `depth`, a name cursor that goes on whatever was added or
//! removed, `limit` and the soft deadline. File windows: bounds, text and
//! binary, a sparse file read by window. The roots: Workspaces from their
//! records and never `stat`ed, `home` and `log`. Every refusal: a path
//! outside the home, a `..`, a link (out or in), a FIFO, a socket, a
//! device, a hard link the commands do not own, what LayerFS keeps
//! private.
//!
//! Nothing runs: a Sandbox is its record and its run folder, which is all
//! a read uses. Scratch folders are under the build's own temporary folder.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_home::disk::{self, paths, write_record};
use layerfs_inspect::{FilesRequest, Side, HARD, SOFT};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
use std::cell::Cell;
use std::os::unix::fs::{symlink, FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn text_of(path: &Path) -> SdkValue {
    t(path.to_str().unwrap())
}

/// A home with Sandbox `s1`, whose commands run as `uid`, and its run
/// folder as a start leaves it.
struct Fixture {
    scratch: PathBuf,
    home: PathBuf,
    run: PathBuf,
    /// The commands' home, `<run folder>/tmp`.
    tmp: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

fn fixture(tag: &str, uid: u32) -> Fixture {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "ifiles-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    std::fs::create_dir_all(&scratch).unwrap();
    disk::private_dir(&home).unwrap();
    disk::ensure(&home).unwrap();
    let run = paths::run_root(&home).join("layerfs-0a1b2c3d-s1");
    for folder in ["tmp", "workspaces", "config", "overlay"] {
        std::fs::create_dir_all(run.join(folder)).unwrap();
    }
    std::fs::write(run.join("daemon.log"), "listening\n").unwrap();
    std::fs::write(run.join("config/daemon.setup"), "private key").unwrap();
    sandbox(&home, "s1", &run, uid);
    Fixture {
        tmp: run.join("tmp"),
        scratch,
        home,
        run,
    }
}

/// Sandbox `name` of `root` as the side that runs it records it. Its
/// daemon is not this or any process: a read never looks for it.
fn sandbox(root: &Path, name: &str, directory: &Path, uid: u32) {
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: (directory.file_name().unwrap().to_string_lossy()).into_owned(),
        container: "ab".repeat(32),
        runtime: "process",
        uid,
        gid: layerfs_home::lock::flock::egid(),
        directory: directory.display().to_string(),
        process: (4_000_000, 1, format!("{:032x}", 7)),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(root, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("id", t(name)),
            ("runtime", t("process")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

fn me() -> u32 {
    layerfs_home::lock::flock::euid()
}

fn args(fields: &[(&str, SdkValue)]) -> SdkValue {
    let mut all = vec![("sandbox".to_owned(), t("s1"))];
    all.extend(fields.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    SdkValue::Map(all)
}

/// One request on the side that holds the run folder, never out of time.
fn read(fx: &Fixture, fields: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    layerfs_inspect::sandbox_page(&Side::Home(&fx.home), &args(fields), &|| false)
}

fn at(fx: &Fixture, path: &Path, more: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![("path", text_of(path))];
    fields.extend(more.iter().cloned());
    read(fx, &fields)
}

fn entries(answer: &SdkValue) -> &[SdkValue] {
    assert_eq!(answer.get_text("kind"), Some("folder"), "{answer:?}");
    match answer.get("entries") {
        Some(SdkValue::List(entries)) => entries,
        _ => panic!("{answer:?}"),
    }
}

/// Each entry's path below `base`.
fn below(answer: &SdkValue, base: &Path) -> Vec<String> {
    let prefix = format!("{}/", base.display());
    entries(answer)
        .iter()
        .map(|entry| {
            let path = entry.get_text("path").unwrap();
            path.strip_prefix(&prefix).unwrap_or(path).to_owned()
        })
        .collect()
}

fn more(answer: &SdkValue) -> bool {
    answer.get_bool("has_more")
}

fn refused(outcome: Result<SdkValue, SdkRefusal>) -> (String, String) {
    match outcome {
        Ok(value) => panic!("answered {value:?}"),
        Err(refusal) => (refusal.code, refusal.message),
    }
}

fn invalid(outcome: Result<SdkValue, SdkRefusal>, says: &str) {
    let (code, message) = refused(outcome);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains(says), "{message} lacks {says:?}");
}

fn json(value: &SdkValue) -> String {
    layerfs_home::json::print(value)
}

/// Every page of `folder` with `fields`, each asked from the `next` of the
/// one before: the entries' paths below it, and how many pages there were.
fn all_pages(fx: &Fixture, folder: &Path, fields: &[(&str, SdkValue)]) -> (Vec<String>, usize) {
    let (mut listed, mut pages, mut cursor) = (Vec::new(), 0, None::<String>);
    loop {
        let mut asked = fields.to_vec();
        asked.extend(cursor.iter().map(|cursor| ("cursor", t(cursor))));
        let page = at(fx, folder, &asked).unwrap();
        let names = below(&page, folder);
        pages += 1;
        assert!(pages < 10_000, "the walk does not end");
        if let Some(last) = entries(&page).last() {
            assert_eq!(page.get_text("next"), last.get_text("path"));
        }
        listed.extend(names);
        if !more(&page) {
            assert!(page.get("reason").is_none(), "{page:?}");
            return (listed, pages);
        }
        assert_eq!(page.get_text("reason"), Some("limit"));
        cursor = Some(page.get_text("next").unwrap().to_owned());
    }
}

/// The tree under `folder` to `depth` levels as the walker orders it:
/// names in byte order, a folder's entries after it. Links are not entered.
fn tree(folder: &Path, prefix: &str, depth: usize, into: &mut Vec<(String, usize)>, level: usize) {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    for name in names {
        let path = folder.join(&name);
        let shown = format!("{prefix}{name}");
        into.push((shown.clone(), level));
        let entered = std::fs::symlink_metadata(&path).unwrap().is_dir() && level < depth;
        if entered {
            tree(&path, &format!("{shown}/"), depth, into, level + 1);
        }
    }
}

#[test]
fn a_big_folder_pages_by_name_and_goes_on_through_changes() {
    let fx = fixture("big", me());
    let big = fx.tmp.join("big");
    std::fs::create_dir(&big).unwrap();
    let name = |n: usize| format!("f{n:05}");
    for n in 0..3000 {
        std::fs::write(big.join(name(n)), "").unwrap();
    }
    // three full pages of 1000, and the last knows nothing follows
    let (listed, pages) = all_pages(&fx, &big, &[("limit", SdkValue::Int(1000))]);
    assert_eq!(pages, 3);
    assert_eq!(listed, (0..3000).map(name).collect::<Vec<_>>());
    // the default page is 500 entries
    let first = at(&fx, &big, &[]).unwrap();
    assert_eq!(below(&first, &big), (0..500).map(name).collect::<Vec<_>>());
    assert!(more(&first));
    assert_eq!(first.get_text("reason"), Some("limit"));
    let cursor = first.get_text("next").unwrap().to_owned();
    assert_eq!(cursor, big.join(name(499)).to_str().unwrap());

    // between pages: entries listed, the cursor's own, the next one and
    // later ones go; new ones come before and after the cursor
    for gone in [100, 499, 500, 1234, 2999] {
        std::fs::remove_file(big.join(name(gone))).unwrap();
    }
    for new in ["a-early", "f00250x", "f00499a", "f01000a", "zz-late"] {
        std::fs::write(big.join(new), "").unwrap();
    }
    let mut rest = Vec::new();
    let mut cursor = Some(cursor);
    while let Some(after) = cursor {
        let fields = [("cursor", t(&after)), ("limit", SdkValue::Int(500))];
        let page = at(&fx, &big, &fields).unwrap();
        rest.extend(below(&page, &big));
        cursor = more(&page).then(|| page.get_text("next").unwrap().to_owned());
    }
    // every name after the cursor that is there, once, in byte order: no
    // surviving name skipped, none listed twice, none from before it
    let mut expected: Vec<String> = (501..2999).filter(|n| *n != 1234).map(name).collect();
    expected.extend(["f00499a", "f01000a", "zz-late"].map(str::to_owned));
    expected.sort();
    assert_eq!(rest, expected);
    assert_eq!(rest.len(), 2500);

    // `limit` is 1 to 1000, `depth` 1 to 32, before any read
    for (option, value) in [("limit", 0), ("limit", 1001), ("depth", 0), ("depth", 33)] {
        let (code, message) = refused(at(&fx, &big, &[(option, SdkValue::Int(value))]));
        assert_eq!(code, "InvalidInput", "{message}");
        assert!(message.starts_with(&format!("sandbox.files: `{option}` is 1 to ")));
    }
}

#[test]
fn a_page_walks_folders_in_tree_order_to_its_depth() {
    let fx = fixture("depth", me());
    let top = fx.tmp.join("tree");
    for folder in ["a/x", "c", "d/e/f", "Z.d"] {
        std::fs::create_dir_all(top.join(folder)).unwrap();
    }
    for file in [
        "a/x/deep1",
        "a/x/deep2",
        "a/y",
        "b",
        "d/e/f/g",
        "d/h",
        "Z",
        "a.txt",
    ] {
        std::fs::write(top.join(file), "x").unwrap();
    }
    // a link to a folder is an entry, never a folder to walk
    symlink(top.join("d"), top.join("a/link")).unwrap();
    for depth in 1..=4 {
        let mut expected = Vec::new();
        tree(&top, "", depth, &mut expected, 1);
        for limit in [1, 2, 3, 5, 1000] {
            let fields = [
                ("depth", SdkValue::Int(depth as i64)),
                ("limit", SdkValue::Int(limit)),
            ];
            let (listed, pages) = all_pages(&fx, &top, &fields);
            let names: Vec<&str> = expected.iter().map(|(name, _)| name.as_str()).collect();
            assert_eq!(listed, names, "depth {depth} limit {limit}");
            // a full last page knows nothing follows
            assert_eq!(pages, expected.len().div_ceil(limit as usize));
        }
        // each entry says how far below the folder it is
        let page = at(&fx, &top, &[("depth", SdkValue::Int(depth as i64))]).unwrap();
        let depths: Vec<_> = entries(&page)
            .iter()
            .map(|entry| entry.get_int("depth").unwrap() as usize)
            .collect();
        let levels: Vec<usize> = expected.iter().map(|(_, level)| *level).collect();
        assert_eq!(depths, levels, "depth {depth}");
    }
    let mut whole = Vec::new();
    tree(&top, "", 3, &mut whole, 1);
    assert_eq!(
        whole.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        [
            "Z",
            "Z.d",
            "a",
            "a/link",
            "a/x",
            "a/x/deep1",
            "a/x/deep2",
            "a/y",
            "a.txt",
            "b",
            "c",
            "d",
            "d/e",
            "d/e/f",
            "d/h"
        ]
    );

    // the cursor's folder goes between two pages: the walk goes on after it
    let deep = [("depth", SdkValue::Int(3)), ("limit", SdkValue::Int(6))];
    let first = at(&fx, &top, &deep).unwrap();
    assert_eq!(
        below(&first, &top),
        ["Z", "Z.d", "a", "a/link", "a/x", "a/x/deep1"]
    );
    std::fs::remove_dir_all(top.join("a/x")).unwrap();
    std::fs::write(top.join("a/w"), "new, before the cursor").unwrap();
    let cursor = ("cursor", first.get("next").unwrap().clone());
    let second = at(&fx, &top, &[deep[0].clone(), cursor]).unwrap();
    assert_eq!(
        below(&second, &top),
        ["a/y", "a.txt", "b", "c", "d", "d/e", "d/e/f", "d/h"]
    );
    assert!(!more(&second));
    // a cursor on a folder goes on inside it
    let inside = [
        ("depth", SdkValue::Int(3)),
        ("cursor", text_of(&top.join("d"))),
    ];
    assert_eq!(
        below(&at(&fx, &top, &inside).unwrap(), &top),
        ["d/e", "d/e/f", "d/h"]
    );
    // and one whose entry is gone goes on after where it was
    let gone = [("cursor", text_of(&top.join("a/x/deep1"))), deep[0].clone()];
    assert_eq!(below(&at(&fx, &top, &gone).unwrap(), &top)[0], "a/y");

    // a cursor is an entry of the folder, no deeper than `depth`
    for cursor in [fx.tmp.join("other"), top.clone(), top.join("d/e/f")] {
        let fields = [("cursor", text_of(&cursor)), ("depth", SdkValue::Int(2))];
        invalid(
            at(&fx, &top, &fields),
            "`cursor` is the path of an entry of",
        );
    }
    invalid(read(&fx, &[("cursor", text_of(&top))]), "name its `path`");
}

#[test]
fn names_are_in_byte_order_and_each_kind_is_told_from_its_own_stat() {
    let fx = fixture("kinds", me());
    // no two differ only in case: macOS folds case in names
    let names = [
        "B", "a", "Z", "é", "y", "ä", "_", "10", "9", "a b", "C", "ab", "Ac",
    ];
    let folder = fx.tmp.join("order");
    std::fs::create_dir(&folder).unwrap();
    for name in names {
        std::fs::write(folder.join(name), "").unwrap();
    }
    let mut sorted = names.map(str::to_owned).to_vec();
    sorted.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    assert_eq!(sorted[..6], ["10", "9", "Ac", "B", "C", "Z"]);
    assert_eq!(sorted[6..], ["_", "a", "a b", "ab", "y", "ä", "é"]);
    assert_eq!(below(&at(&fx, &folder, &[]).unwrap(), &folder), sorted);
    let (paged, _) = all_pages(&fx, &folder, &[("limit", SdkValue::Int(4))]);
    assert_eq!(paged, sorted);

    let kinds = fx.tmp.join("kinds");
    std::fs::create_dir_all(kinds.join("folder/inner")).unwrap();
    std::fs::write(kinds.join("file"), "12345").unwrap();
    std::fs::set_permissions(kinds.join("file"), PermissionsExt::from_mode(0o640)).unwrap();
    symlink("/etc", kinds.join("link-out")).unwrap();
    symlink("folder", kinds.join("link-in")).unwrap();
    symlink("nowhere", kinds.join("link-dangling")).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(kinds.join("pipe"))
        .status();
    assert!(made.unwrap().success());
    // what LayerFS keeps private, wherever it is: a Store's database
    std::fs::write(kinds.join("store.sqlite"), vec![0u8; 4096]).unwrap();
    std::fs::write(kinds.join("store.sqlite-wal"), vec![0u8; 100]).unwrap();
    // a lock file in a command's own project is the command's
    std::fs::write(kinds.join("Cargo.lock"), "[[package]]").unwrap();
    let page = at(&fx, &kinds, &[("depth", SdkValue::Int(2))]).unwrap();
    let shown: Vec<(String, &str, Option<i64>)> = entries(&page)
        .iter()
        .map(|entry| {
            let name = entry.get_text("path").unwrap();
            let name = name.rsplit_once("/kinds/").unwrap().1.to_owned();
            (name, entry.get_text("kind").unwrap(), entry.get_int("size"))
        })
        .collect();
    let expected = [
        ("Cargo.lock", "file", Some(11)),
        ("file", "file", Some(5)),
        ("folder", "folder", None),
        ("folder/inner", "folder", None),
        ("link-dangling", "symlink", None),
        ("link-in", "symlink", None),
        ("link-out", "symlink", None),
        ("pipe", "other", None),
        ("store.sqlite", "restricted", Some(4096)),
        ("store.sqlite-wal", "restricted", Some(100)),
    ];
    assert_eq!(
        shown,
        expected.map(|(name, kind, size)| (name.to_owned(), kind, size))
    );
    let entry = |name: &str| {
        let path = kinds.join(name);
        (entries(&page).iter())
            .find(|entry| entry.get_text("path") == path.to_str())
            .unwrap()
    };
    // a link shows what it names and is never followed or entered
    assert_eq!(entry("link-out").get_text("target"), Some("/etc"));
    assert_eq!(entry("link-in").get_text("target"), Some("folder"));
    assert_eq!(entry("link-dangling").get_text("target"), Some("nowhere"));
    assert!(entry("file").get("target").is_none());
    assert_eq!(entry("file").get_int("mode"), Some(0o640));
    let mtime = entry("file").get_int("mtime").unwrap();
    assert!(mtime > 1_600_000_000_000, "milliseconds: {mtime}");
    assert_eq!(entry("folder/inner").get_int("depth"), Some(2));
    assert!(entry("folder").get_int("mode").is_some());
    // exactly these fields, in this order
    assert_eq!(
        json(entry("file")),
        format!(
            r#"{{"path":"{}","depth":1,"kind":"file","size":5,"mode":416,"mtime":{mtime}}}"#,
            kinds.join("file").display()
        )
    );
    // an empty folder is an empty page
    let empty = at(&fx, &kinds.join("folder/inner"), &[]).unwrap();
    assert_eq!(
        json(&empty),
        r#"{"kind":"folder","entries":[],"next":null,"has_more":false}"#
    );
}

#[test]
fn a_page_past_its_soft_budget_answers_what_it_has_with_reason_deadline() {
    let fx = fixture("deadline", me());
    let folder = fx.tmp.join("slow");
    std::fs::create_dir_all(folder.join("m")).unwrap();
    for n in 0..20 {
        std::fs::write(folder.join(format!("e{n:02}")), "").unwrap();
        std::fs::write(folder.join(format!("m/in{n:02}")), "").unwrap();
    }
    // a clock that runs out after seven entries: the walker looks at it
    // before each one
    let looks = Cell::new(0);
    let spent = || {
        looks.set(looks.get() + 1);
        looks.get() > 7
    };
    let asked = args(&[("path", text_of(&folder)), ("depth", SdkValue::Int(2))]);
    let page = layerfs_inspect::sandbox_page(&Side::Home(&fx.home), &asked, &spent).unwrap();
    let first = below(&page, &folder);
    assert_eq!(first, ["e00", "e01", "e02", "e03", "e04", "e05", "e06"]);
    assert!(more(&page));
    assert_eq!(page.get_text("reason"), Some("deadline"));
    assert_eq!(page.get_text("next"), folder.join("e06").to_str());
    // the walk goes on from there, and nothing is listed twice or skipped
    let cursor = ("cursor", page.get("next").unwrap().clone());
    let fields = [("depth", SdkValue::Int(2)), cursor];
    let rest = at(&fx, &folder, &fields).unwrap();
    let mut whole = Vec::new();
    tree(&folder, "", 2, &mut whole, 1);
    let mut listed = first;
    listed.extend(below(&rest, &folder));
    assert_eq!(listed.len(), 41);
    assert_eq!(
        listed,
        whole.into_iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    // through the request's own budget: a request that has used its four
    // seconds before the walk starts answers no entry, in time, and says
    // why; its helper ends and frees its permit
    let _turn = crate::permits();
    let asked = disk::with_field(asked, "kind", t("sandbox_files"));
    let (home, request) = (fx.home.clone(), asked.clone());
    let started = Instant::now();
    let late = layerfs_inspect::admit(move |budget| {
        std::thread::sleep(SOFT + Duration::from_millis(100));
        layerfs_inspect::serve(&Side::Home(&home), &request, budget)
    })
    .unwrap();
    assert!(started.elapsed() < HARD, "{:?}", started.elapsed());
    assert_eq!(
        json(&late),
        r#"{"kind":"folder","entries":[],"next":null,"has_more":true,"reason":"deadline"}"#
    );
    assert_eq!(layerfs_inspect::running(), 0);
    // a request in time is not cut
    let home = fx.home.clone();
    let whole = layerfs_inspect::admit(move |budget| {
        layerfs_inspect::serve(&Side::Home(&home), &asked, budget)
    });
    assert_eq!(entries(&whole.unwrap()).len(), 41);
}

#[test]
fn a_file_is_read_by_window_as_text_or_told_binary() {
    let fx = fixture("window", me());
    let notes = fx.tmp.join("notes.txt");
    std::fs::write(&notes, "héllo\nwörld\tend\r\n\x1b[1mbold\n").unwrap();
    let whole = at(&fx, &notes, &[]).unwrap();
    // line breaks and tabs as they are; every other control character
    // escaped, as a log line's are
    assert_eq!(
        json(&whole),
        r#"{"kind":"file","size":28,"offset":0,"content":"héllo\nwörld\tend\\r\n\\u{1b}[1mbold\n","binary":false,"next":null}"#
    );
    // a window never ends inside a character: `é` is two bytes
    let window = |offset: i64, length: i64| {
        let fields = [
            ("offset", SdkValue::Int(offset)),
            ("length", SdkValue::Int(length)),
        ];
        let got = at(&fx, &notes, &fields).unwrap();
        assert_eq!(got.get_int("offset"), Some(offset));
        assert_eq!(got.get_int("size"), Some(28));
        (
            got.get_text("content").unwrap().to_owned(),
            got.get_int("next"),
        )
    };
    assert_eq!(window(0, 2), ("h".to_owned(), Some(1)));
    assert_eq!(window(1, 2), ("é".to_owned(), Some(3)));
    assert_eq!(window(3, 4), ("llo\n".to_owned(), Some(7)));
    // a window of one byte inside a character still moves on
    assert_eq!(window(1, 1), ("\u{fffd}".to_owned(), Some(2)));
    // the windows of a file, each from the `next` before it, are the file
    let (mut joined, mut offset) = (String::new(), Some(0));
    while let Some(from) = offset {
        let (content, next) = window(from, 5);
        joined.push_str(&content);
        offset = next;
    }
    assert_eq!(joined, whole.get_text("content").unwrap());
    // at and past the end: nothing, and no next
    assert_eq!(window(28, 10), (String::new(), None));
    assert_eq!(window(5000, 10), (String::new(), None));
    assert_eq!(window(20, 1 << 20).1, None);
    for (option, value) in [("offset", -1), ("length", 0), ("length", (1 << 20) + 1)] {
        let (code, message) = refused(at(&fx, &notes, &[(option, SdkValue::Int(value))]));
        assert_eq!(code, "InvalidInput", "{message}");
        assert!(message.starts_with(&format!("sandbox.files: `{option}` is ")));
    }

    let empty = fx.tmp.join("empty");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(
        json(&at(&fx, &empty, &[]).unwrap()),
        r#"{"kind":"file","size":0,"offset":0,"content":"","binary":false,"next":null}"#
    );
    // a NUL in the window: binary, and its bytes are not sent as text
    let elf = fx.tmp.join("a.out");
    std::fs::write(&elf, b"\x7fELF\x02\x01\x01\x00\x00\x00rest").unwrap();
    assert_eq!(
        json(&at(&fx, &elf, &[]).unwrap()),
        r#"{"kind":"file","size":14,"offset":0,"content":"","binary":true,"next":null}"#
    );
    // bytes that are not UTF-8 and hold no NUL are text with U+FFFD
    let latin = fx.tmp.join("latin1");
    std::fs::write(&latin, b"caf\xe9\n").unwrap();
    let got = at(&fx, &latin, &[]).unwrap();
    assert_eq!(got.get_text("content"), Some("caf\u{fffd}\n"));
    assert!(!got.get_bool("binary"));
    // the first window is 256 KiB; a window is at most 1 MiB
    let long = fx.tmp.join("long");
    std::fs::write(&long, vec![b'a'; 300 << 10]).unwrap();
    let first = at(&fx, &long, &[]).unwrap();
    assert_eq!(first.get_text("content").unwrap().len(), 256 << 10);
    assert_eq!(first.get_int("next"), Some(256 << 10));
    let widest = at(&fx, &long, &[("length", SdkValue::Int(1 << 20))]).unwrap();
    assert_eq!(widest.get_text("content").unwrap().len(), 300 << 10);
    assert_eq!(widest.get_int("next"), None);

    // 10 GiB that hold eight bytes: a window near its end reads that
    // window and nothing else
    let sparse = fx.tmp.join("sparse");
    let size: u64 = 10 << 30;
    let file = std::fs::File::create(&sparse).unwrap();
    file.set_len(size).unwrap();
    file.write_all_at(b"the end\n", size - 8).unwrap();
    drop(file);
    let started = Instant::now();
    let tail = at(&fx, &sparse, &[("offset", SdkValue::Int(size as i64 - 8))]).unwrap();
    assert_eq!(
        json(&tail),
        r#"{"kind":"file","size":10737418240,"offset":10737418232,"content":"the end\n","binary":false,"next":null}"#
    );
    let middle = [
        ("offset", SdkValue::Int(5 << 30)),
        ("length", SdkValue::Int(1 << 20)),
    ];
    let hole = at(&fx, &sparse, &middle).unwrap();
    assert!(hole.get_bool("binary"));
    assert_eq!(hole.get_int("next"), Some((5 << 30) + (1 << 20)));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    // and its folder lists it with its size, without reading it
    let listed = at(&fx, &fx.tmp, &[]).unwrap();
    let entry = (entries(&listed).iter())
        .find(|entry| entry.get_text("path") == sparse.to_str())
        .unwrap();
    assert_eq!(entry.get_int("size"), Some(size as i64));
}

/// A socket at `path`: bound there, or bound under a shorter name beside
/// the scratch folders and moved (a socket's path holds about 100 bytes).
fn socket(path: &Path) -> bool {
    if std::os::unix::net::UnixListener::bind(path).is_ok() {
        return true;
    }
    let short = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .ancestors()
        .find_map(|folder| {
            let short = folder.join(format!(".s{}", std::process::id()));
            (short.as_os_str().len() < 90).then_some(short)
        });
    let Some(short) = short else { return false };
    let _ = std::fs::remove_file(&short);
    let bound = std::os::unix::net::UnixListener::bind(&short).is_ok();
    let moved = bound && std::fs::rename(&short, path).is_ok();
    let _ = std::fs::remove_file(&short);
    moved
}

#[test]
fn what_is_not_a_file_or_folder_of_the_home_is_refused() {
    let fx = fixture("refusals", me());
    std::fs::create_dir_all(fx.tmp.join("real/sub")).unwrap();
    std::fs::write(fx.tmp.join("real/file"), "inside").unwrap();
    std::fs::write(fx.run.join("overlay/1.sqlite"), vec![0u8; 4096]).unwrap();
    assert_eq!(
        at(&fx, &fx.tmp.join("real/file"), &[])
            .unwrap()
            .get_text("content"),
        Some("inside")
    );

    // outside the home: the rest of the run folder, the computer
    let outside = "is outside Sandbox s1's home";
    let (code, message) = refused(at(&fx, Path::new("/etc/hosts"), &[]));
    assert_eq!(code, "InvalidInput");
    assert!(message.contains(outside) && message.ends_with("only paths under the home are read"));
    for (path, why) in [
        ("config/daemon.setup", "LayerFS keeps it private"),
        ("config", "LayerFS keeps it private"),
        ("overlay/1.sqlite", "LayerFS keeps it private"),
        ("daemon.log", "a log is read with `sandbox.log`"),
        (
            "workspaces/1",
            "a Workspace's files are read through its mount",
        ),
        ("workspaces", "only paths under the home are read"),
        ("", "only paths under the home are read"),
    ] {
        let (code, message) = refused(at(&fx, &fx.run.join(path), &[]));
        assert_eq!(code, "InvalidInput", "{path}: {message}");
        assert!(
            message.contains(outside) && message.contains(why),
            "{path}: {message}"
        );
    }
    // a path is absolute and has no `..`, whatever it would lead to
    let escaping = format!("{}/real/../../config/daemon.setup", fx.tmp.display());
    let inside = format!("{}/real/../real/file", fx.tmp.display());
    for path in [
        escaping.as_str(),
        inside.as_str(),
        "tmp/real/file",
        "real",
        "",
    ] {
        invalid(
            read(&fx, &[("path", t(path))]),
            "`path` is an absolute path with no `..`",
        );
    }

    // a link, out of the home or within it, at the end or on the way
    symlink("/etc", fx.tmp.join("out")).unwrap();
    symlink(fx.run.join("config"), fx.tmp.join("config")).unwrap();
    symlink("real", fx.tmp.join("in")).unwrap();
    symlink("file", fx.tmp.join("real/alias")).unwrap();
    for path in [
        "out",
        "out/hosts",
        "config",
        "config/daemon.setup",
        "in",
        "in/file",
        "real/alias",
    ] {
        invalid(
            at(&fx, &fx.tmp.join(path), &[]),
            "symbolic link; links are not followed",
        );
    }

    // a FIFO nobody writes to: refused at once, not waited on
    let made = std::process::Command::new("mkfifo")
        .arg(fx.tmp.join("pipe"))
        .status();
    assert!(made.unwrap().success());
    let started = Instant::now();
    invalid(
        at(&fx, &fx.tmp.join("pipe"), &[]),
        "is a FIFO; only files and folders are read",
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    if socket(&fx.tmp.join("sock")) {
        invalid(
            at(&fx, &fx.tmp.join("sock"), &[]),
            "is a socket; only files and folders",
        );
        println!("socket: refused");
    } else {
        println!("NOT_RUN socket: no socket could be made in the scratch folder");
    }
    let node = std::process::Command::new("mknod")
        .arg(fx.tmp.join("null"))
        .args(["c", "1", "3"])
        .output();
    match node {
        Ok(out) if out.status.success() => {
            invalid(
                at(&fx, &fx.tmp.join("null"), &[]),
                "is a device; only files and folders",
            );
            println!("device: refused");
        }
        _ => println!("NOT_RUN device: this user cannot make a device node (mknod refused)"),
    }

    // what LayerFS keeps private is listed and never read, nor is
    // anything under it
    std::fs::create_dir_all(fx.tmp.join("nested/store.sqlite.d")).unwrap();
    std::fs::write(fx.tmp.join("nested/store.sqlite"), "db").unwrap();
    std::fs::write(fx.tmp.join("nested/store.sqlite.d/x"), "x").unwrap();
    invalid(
        at(&fx, &fx.tmp.join("nested/store.sqlite"), &[]),
        "is kept private by LayerFS",
    );
    invalid(
        at(&fx, &fx.tmp.join("nested/store.sqlite.d"), &[]),
        "is kept private by LayerFS",
    );
    invalid(
        at(&fx, &fx.tmp.join("nested/store.sqlite.d/x"), &[]),
        "is in a folder that is listed, not read",
    );
    let nested = at(&fx, &fx.tmp.join("nested"), &[("depth", SdkValue::Int(3))]).unwrap();
    assert_eq!(
        below(&nested, &fx.tmp),
        ["nested/store.sqlite", "nested/store.sqlite.d"]
    );

    let (code, message) = refused(at(&fx, &fx.tmp.join("nothing"), &[]));
    assert_eq!(code, "NotFound", "{message}");
    let (code, _) = refused(at(&fx, &fx.tmp.join("real/file/x"), &[]));
    assert_eq!(code, "NotFound");
    // every refusal left the home as it was: still listed
    assert!(at(&fx, &fx.tmp, &[]).is_ok());
}

#[test]
fn a_hard_link_is_read_only_when_the_commands_own_the_file() {
    // the Sandbox's commands are this user: their own links are theirs
    let own = fixture("links-own", me());
    std::fs::write(own.tmp.join("one"), "two names").unwrap();
    std::fs::hard_link(own.tmp.join("one"), own.tmp.join("twin")).unwrap();
    for name in ["one", "twin"] {
        let got = at(&own, &own.tmp.join(name), &[]).unwrap();
        assert_eq!(got.get_text("content"), Some("two names"));
    }
    // the commands run as another user (the record's numeric uid, never a
    // name looked up): a file with two names that user does not own was
    // linked in, and is listed but not read
    let other = fixture("links-other", me() + 1);
    std::fs::write(other.tmp.join("single"), "one name").unwrap();
    std::fs::write(other.tmp.join("one"), "two names").unwrap();
    std::fs::hard_link(other.tmp.join("one"), other.tmp.join("twin")).unwrap();
    let got = at(&other, &other.tmp.join("single"), &[]).unwrap();
    assert_eq!(got.get_text("content"), Some("one name"));
    for name in ["one", "twin"] {
        invalid(
            at(&other, &other.tmp.join(name), &[]),
            "is a hard link to a file the Sandbox's commands do not own",
        );
    }
    let listed = at(&other, &other.tmp, &[]).unwrap();
    assert_eq!(below(&listed, &other.tmp), ["one", "single", "twin"]);
}

#[test]
fn the_roots_are_the_workspaces_from_their_records_then_home_and_log() {
    let fx = fixture("roots", me());
    // three Workspaces recorded; where they would be mounted there is a
    // FIFO, a folder nobody may open, and nothing: none is touched
    let folder = paths::workspace_dir(&fx.home, "s1");
    for n in [2u8, 10, 1] {
        let id = format!("{n:064x}");
        let path = fx.run.join(format!("workspaces/{n}"));
        let record = WorkspaceRecord::mounted(id.clone(), None, "main", path.to_str().unwrap(), 0);
        record.write(&folder.join(format!("{id}.json"))).unwrap();
    }
    let made = std::process::Command::new("mkfifo")
        .arg(fx.run.join("workspaces/1"))
        .status();
    assert!(made.unwrap().success());
    std::fs::create_dir(fx.run.join("workspaces/2")).unwrap();
    let closed = PermissionsExt::from_mode(0o000);
    std::fs::set_permissions(fx.run.join("workspaces/2"), closed).unwrap();
    let before = std::fs::metadata(fx.run.join("workspaces/2")).unwrap();

    let started = Instant::now();
    let roots = read(&fx, &[]).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    let run = fx.run.display();
    let stat = |name: &str| std::fs::metadata(fx.run.join(name)).unwrap();
    let mtime = |name: &str| {
        let modified = stat(name).modified().unwrap();
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    };
    let mode = |name: &str| stat(name).permissions().mode() & 0o7777;
    let mount = |n: u8| {
        format!(
            r#"{{"path":"{run}/workspaces/{n}","depth":1,"kind":"mount","size":null,"mode":null,"mtime":null,"role":"workspace","workspace":"s1:{run}/workspaces/{n}"}}"#
        )
    };
    let expected = format!(
        r#"{{"kind":"folder","entries":[{},{},{},{{"path":"{run}/tmp","depth":1,"kind":"folder","size":null,"mode":{},"mtime":{},"role":"home"}},{{"path":"{run}/daemon.log","depth":1,"kind":"file","size":10,"mode":{},"mtime":{},"role":"log"}}],"next":null,"has_more":false}}"#,
        mount(1),
        mount(2),
        mount(10),
        mode("tmp"),
        mtime("tmp"),
        mode("daemon.log"),
        mtime("daemon.log"),
    );
    assert_eq!(json(&roots), expected);
    // a Workspace's folder was not opened or changed
    let after = std::fs::metadata(fx.run.join("workspaces/2")).unwrap();
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());

    // the records a request carries are used in place of this side's own:
    // what the home sends for a Sandbox whose daemon it reaches directly
    let sent = SdkValue::List(vec![SdkValue::map([
        ("id", t("7f")),
        (
            "path",
            t("/layerfs/data/machine/linux-guest/run/x/workspaces/7"),
        ),
    ])]);
    let carried = read(&fx, &[("workspaces", sent)]).unwrap();
    let shown: Vec<_> = entries(&carried)
        .iter()
        .map(|entry| (entry.get_text("role").unwrap(), entry.get_text("workspace")))
        .collect();
    assert_eq!(
        shown,
        [("workspace", Some("7f")), ("home", None), ("log", None)]
    );
    invalid(
        read(&fx, &[("workspaces", t("all"))]),
        "`workspaces` is a list",
    );

    // the home's path is what a listing of it uses
    let home = entries(&roots)[3].get_text("path").unwrap();
    assert_eq!(home, fx.tmp.to_str().unwrap());
    std::fs::write(fx.tmp.join("hello"), "hi").unwrap();
    assert_eq!(
        below(&at(&fx, Path::new(home), &[]).unwrap(), &fx.tmp),
        ["hello"]
    );
    // the log is listed, and read through `sandbox.log` only
    invalid(
        at(&fx, &fx.run.join("daemon.log"), &[]),
        "a log is read with `sandbox.log`",
    );

    // a run folder a start left without a log lists no log
    std::fs::remove_file(fx.run.join("daemon.log")).unwrap();
    let fewer = read(&fx, &[]).unwrap();
    assert_eq!(entries(&fewer).len(), 4);
    let _ = std::fs::set_permissions(
        fx.run.join("workspaces/2"),
        PermissionsExt::from_mode(0o700),
    );
}

#[test]
fn the_call_reads_a_stopped_sandbox_s_home_in_this_process() {
    let _turn = crate::permits();
    // the fixture's daemon is no process at all: a stopped Sandbox
    let fx = fixture("local", me());
    std::fs::write(fx.tmp.join("left"), "by a command").unwrap();
    let roots = layerfs_inspect::sandbox_files(&fx.home, "s1", &FilesRequest::default()).unwrap();
    let roles: Vec<_> = (entries(&roots).iter())
        .map(|entry| entry.get_text("role").unwrap())
        .collect();
    assert_eq!(roles, ["home", "log"]);
    let request = |path: &Path| FilesRequest {
        path: Some(path.to_str().unwrap().to_owned()),
        ..FilesRequest::default()
    };
    let listed = layerfs_inspect::sandbox_files(&fx.home, "s1", &request(&fx.tmp)).unwrap();
    assert_eq!(below(&listed, &fx.tmp), ["left"]);
    let file =
        layerfs_inspect::sandbox_files(&fx.home, "s1", &request(&fx.tmp.join("left"))).unwrap();
    assert_eq!(file.get_text("content"), Some("by a command"));
    assert_eq!(layerfs_inspect::running(), 0);

    // options are checked before the Sandbox is looked up
    let wide = FilesRequest {
        limit: Some(1001),
        ..request(&fx.tmp)
    };
    let refusal = layerfs_inspect::sandbox_files(&fx.home, "nobody", &wide).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    let lost = layerfs_inspect::sandbox_files(&fx.home, "nobody", &FilesRequest::default());
    assert_eq!(lost.unwrap_err().code, "NoSuchSandbox");
    // a record whose run folder is gone: no Sandbox runs here
    std::fs::remove_dir_all(&fx.run).unwrap();
    for request in [FilesRequest::default(), request(&fx.tmp)] {
        let gone = layerfs_inspect::sandbox_files(&fx.home, "s1", &request).unwrap_err();
        assert_eq!(gone.code, "NoSuchSandbox", "{gone:?}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_mount_under_the_home_is_listed_from_the_table_and_never_entered() {
    let fx = fixture("mount", me());
    let point = fx.tmp.join("mnt");
    std::fs::create_dir(&point).unwrap();
    let mounted = std::process::Command::new("mount")
        .args(["-t", "tmpfs", "tmpfs"])
        .arg(&point)
        .output();
    match mounted {
        Ok(out) if out.status.success() => {
            std::fs::write(point.join("file"), "x").unwrap();
            let listed = at(&fx, &fx.tmp, &[("depth", SdkValue::Int(2))]).unwrap();
            assert_eq!(
                json(&listed),
                format!(
                    r#"{{"kind":"folder","entries":[{{"path":"{}","depth":1,"kind":"mount","size":null,"mode":null,"mtime":null,"fs":"tmpfs"}}],"next":"{}","has_more":false}}"#,
                    point.display(),
                    point.display()
                )
            );
            invalid(
                at(&fx, &point, &[]),
                "is a mount point; a mount is listed, not read",
            );
            invalid(at(&fx, &point.join("file"), &[]), "is in a mount");
            let off = std::process::Command::new("umount").arg(&point).status();
            assert!(off.unwrap().success());
            println!("mount: listed and refused");
        }
        other => println!(
            "NOT_RUN mount under the home: no mount can be made here ({})",
            match other {
                Ok(out) => String::from_utf8_lossy(&out.stderr).trim().to_owned(),
                Err(error) => error.to_string(),
            }
        ),
    }
}
