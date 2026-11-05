//! `project.export`: exports a project's committed Branch snapshot from its
//! Store to a local folder on the caller's host (#53).
//!
//! Covers:
//! 1. Tree fidelity across nested directories, empty directories, `.git/`,
//!    ignored files, binary files, Unicode filenames, multi-chunk (> 8 MiB)
//!    regular files, symlinks, executable modes (`0o755`), read-only
//!    directories (`0o555`) and nanosecond mtimes;
//! 2. Branch selection (`main` default vs explicit Branch) and pinned snapshot
//!    isolation during concurrent `file.write` / `branch.publish`;
//! 3. Worker-backed SQLite streaming (`Executor::Worker`), offline worker
//!    `Unavailable` refusal without creating local directories, and mid-stream
//!    failure cleanup of `.layerfs-export-*` staging folders (including
//!    `0o555` subdirectories);
//! 4. Pre-effect destination checks (`InvalidInput` for relative, `/`, `..`,
//!    or inside the Store home; `NotFound` for missing parent; `Exists` for
//!    existing file/symlink; `NotEmpty` for non-empty directory; allowed
//!    export into an existing empty directory) and live `prepare` / `export` /
//!    `finalize` progress.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker;
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lfs-exp-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn private_home(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    home
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call_with_events(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
    events: &mut dyn FnMut(SdkValue),
) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, events).map_err(|r| (r.code, r.message))
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    call_with_events(home, name, args, &mut |_| {})
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn big_bytes() -> Vec<u8> {
    (0..(9u32 << 20) + 321)
        .map(|i| (i.wrapping_mul(37) % 251) as u8)
        .collect()
}

/// Builds a rich source folder and sets deterministic permissions and
/// nanosecond modification times on every entry, root last.
fn populate_sample(folder: &Path) -> Vec<u8> {
    std::fs::create_dir_all(folder.join("src/nested")).unwrap();
    std::fs::create_dir_all(folder.join("empty_dir")).unwrap();
    std::fs::create_dir_all(folder.join(".git/objects")).unwrap();
    std::fs::write(folder.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(folder.join(".gitignore"), "*.log\nbuild/\n").unwrap();
    std::fs::write(folder.join("debug.log"), "ignored but in LayerFS\n").unwrap();
    std::fs::write(folder.join("README.md"), "# Sample\n").unwrap();
    std::fs::write(folder.join("zero.txt"), b"").unwrap();
    std::fs::write(
        folder.join("src/nested/数据-🚀.bin"),
        [0x00, 0xff, 0x7f, 0x80],
    )
    .unwrap();
    std::os::unix::fs::symlink("src/nested/数据-🚀.bin", folder.join("link_to_bin")).unwrap();
    std::os::unix::fs::symlink("../nonexistent_target", folder.join("src/dangling_link")).unwrap();
    std::fs::write(folder.join("run.sh"), "#!/bin/sh\necho ok\n").unwrap();
    std::fs::set_permissions(
        folder.join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let big = big_bytes();
    std::fs::write(folder.join("src/big.bin"), &big).unwrap();

    std::fs::create_dir_all(folder.join("readonly/sub")).unwrap();
    std::fs::write(folder.join("readonly/sub/locked.txt"), "locked content\n").unwrap();

    // Set explicit nanosecond mtimes before closing readonly/sub
    let set_mtime = |p: &Path, sec: i64, nsec: i64| {
        use nix::sys::stat::{utimensat, UtimensatFlags};
        use nix::sys::time::TimeSpec;
        let ts = TimeSpec::new(sec, nsec as _);
        utimensat(
            nix::fcntl::AT_FDCWD,
            p,
            &TimeSpec::UTIME_OMIT,
            &ts,
            UtimensatFlags::NoFollowSymlink,
        )
        .unwrap();
    };
    set_mtime(&folder.join("README.md"), 1_700_000_100, 111_222_333);
    set_mtime(&folder.join("run.sh"), 1_700_000_200, 444_555_666);
    set_mtime(&folder.join("link_to_bin"), 1_700_000_300, 777_888_999);
    set_mtime(
        &folder.join("readonly/sub/locked.txt"),
        1_700_000_400,
        123_000_456,
    );
    std::fs::set_permissions(
        folder.join("readonly/sub"),
        std::fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    set_mtime(&folder.join("readonly/sub"), 1_700_000_500, 654_321_000);
    set_mtime(folder, 1_700_000_600, 987_654_321);
    big
}

fn unlock_readonly(root: &Path) {
    let sub = root.join("readonly/sub");
    if sub.exists() {
        let _ = std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755));
    }
}

/// Verifies that `source` and `exported` have identical entry types,
/// permissions (except symlinks), nanosecond mtimes, symlink targets and
/// regular file bytes.
fn assert_same_tree(source: &Path, exported: &Path) {
    let (a, b) = (
        std::fs::symlink_metadata(source).unwrap(),
        std::fs::symlink_metadata(exported).unwrap(),
    );
    let shown = source.display();
    assert_eq!(a.file_type(), b.file_type(), "{shown}");
    assert_eq!(
        (a.mtime(), a.mtime_nsec()),
        (b.mtime(), b.mtime_nsec()),
        "mtime mismatch at {shown}"
    );
    if a.file_type().is_symlink() {
        assert_eq!(
            std::fs::read_link(source).unwrap(),
            std::fs::read_link(exported).unwrap(),
            "symlink target mismatch at {shown}"
        );
        return;
    }
    assert_eq!(
        a.mode() & 0o7777,
        b.mode() & 0o7777,
        "mode mismatch at {shown}"
    );
    if a.is_file() {
        assert_eq!(a.len(), b.len(), "size mismatch at {shown}");
        assert!(
            std::fs::read(source).unwrap() == std::fs::read(exported).unwrap(),
            "content mismatch at {shown}"
        );
        return;
    }
    let children = |dir: &Path| {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        v.sort();
        v
    };
    let names = children(source);
    assert_eq!(names, children(exported), "entries mismatch at {shown}");
    for name in names {
        assert_same_tree(&source.join(&name), &exported.join(&name));
    }
}

#[test]
fn export_fidelity_progress_and_empty_destination_on_local_sqlite() {
    let home = private_home("local-home");
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("source");
    let big = populate_sample(&source);

    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );

    let out_parent = tempdir("out-parent");
    let dest = out_parent.join("exported");
    let mut events = Vec::new();
    let res = call_with_events(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
        &mut |ev| events.push(ev),
    )
    .unwrap();

    assert_eq!(res.get_text("project"), Some("shop/app"));
    assert_eq!(res.get_text("store"), Some("shop"));
    assert_eq!(res.get_text("name"), Some("app"));
    assert_eq!(res.get_text("kind"), Some("sqlite"));
    assert_eq!(res.get_text("machine"), Some("native"));
    assert_eq!(res.get_text("branch"), Some("main"));
    assert!(res.get_text("snapshot").unwrap_or("").starts_with("main@"));
    assert_eq!(res.get_text("node").unwrap_or("").len(), 66);
    assert_eq!(res.get_text("root").unwrap_or("").len(), 64);
    assert_eq!(
        res.get_text("destination"),
        Some(dest.display().to_string().as_str())
    );
    assert_eq!(res.get_int("files"), Some(9));
    assert_eq!(res.get_int("directories"), Some(7));
    assert_eq!(res.get_int("symlinks"), Some(2));
    assert!(res.get_int("bytes").unwrap() > big.len() as i64);

    assert_same_tree(&source, &dest);

    // Check progress events: prepare -> export (monotonic, >= 2 events for > 9 MiB) -> finalize
    let phases: Vec<String> = events
        .iter()
        .map(|e| {
            let p = e.get("progress").expect("progress event");
            assert_eq!(p.get("total"), Some(&SdkValue::Null));
            p.get_text("phase").unwrap().to_owned()
        })
        .collect();
    assert_eq!(phases.first().map(String::as_str), Some("prepare"));
    assert_eq!(phases.last().map(String::as_str), Some("finalize"));
    let prepare_prog = events.first().and_then(|e| e.get("progress")).unwrap();
    assert_eq!(prepare_prog.get_int("files"), Some(0));
    assert_eq!(prepare_prog.get_int("directories"), Some(0));
    assert_eq!(prepare_prog.get_int("symlinks"), Some(0));
    assert_eq!(prepare_prog.get_int("bytes"), Some(0));
    assert_eq!(prepare_prog.get("done"), Some(&SdkValue::Null));

    let export_events: Vec<&SdkValue> = events
        .iter()
        .filter_map(|e| e.get("progress"))
        .filter(|p| p.get_text("phase") == Some("export"))
        .collect();
    assert!(
        export_events.len() >= 2,
        "expected multiple export progress updates for >9 MiB tree: {export_events:?}"
    );
    let mut prev = (0i64, 0i64, 0i64, 0i64);
    for p in &export_events {
        let cur = (
            p.get_int("files").unwrap(),
            p.get_int("directories").unwrap(),
            p.get_int("symlinks").unwrap(),
            p.get_int("bytes").unwrap(),
        );
        assert_eq!(p.get_int("done"), Some(cur.3));
        assert!(
            cur.0 >= prev.0 && cur.1 >= prev.1 && cur.2 >= prev.2 && cur.3 >= prev.3,
            "non-monotonic progress: {prev:?} -> {cur:?}"
        );
        prev = cur;
    }
    assert_eq!(prev.0, 9);
    assert_eq!(prev.1, 7);
    assert_eq!(prev.2, 2);
    assert_eq!(prev.3, res.get_int("bytes").unwrap());

    let finalize_prog = events.last().and_then(|e| e.get("progress")).unwrap();
    assert_eq!(finalize_prog.get_int("files"), Some(9));
    assert_eq!(finalize_prog.get_int("directories"), Some(7));
    assert_eq!(finalize_prog.get_int("symlinks"), Some(2));
    assert_eq!(finalize_prog.get_int("bytes"), res.get_int("bytes"));
    assert_eq!(finalize_prog.get_int("done"), res.get_int("bytes"));

    // Exporting into an already existing empty directory succeeds too
    let empty_dest = out_parent.join("preexisting-empty");
    std::fs::create_dir(&empty_dest).unwrap();
    let res2 = ok(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&empty_dest.display().to_string())),
        ],
    );
    assert_eq!(res2.get_int("files"), Some(9));
    assert_same_tree(&source, &empty_dest);

    unlock_readonly(&source);
    unlock_readonly(&dest);
    unlock_readonly(&empty_dest);
    for d in [home, source, out_parent] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn destination_and_argument_refusals_have_no_side_effects() {
    let home = private_home("neg-home");
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("neg-src");
    std::fs::write(source.join("a.txt"), "hello\n").unwrap();
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );

    let area = tempdir("neg-area");

    // 1. Relative destination
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t("relative/out")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput");

    // 2. Root path `/` or path with `..`
    for bad in ["/", "/tmp/a/../b"] {
        let (code, _) = call(
            &home,
            "project.export",
            &[("project", t("shop/app")), ("destination", t(bad))],
        )
        .unwrap_err();
        assert_eq!(code, "InvalidInput", "{bad}");
    }

    // 3. Inside the Store home (neutral wording, no LAYERFS_HOME)
    let in_home = home.join("exported");
    let (code, msg) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&in_home.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput");
    assert!(msg.contains("inside the Store home"), "{msg}");
    assert!(!msg.contains("LAYERFS_HOME"), "{msg}");
    assert!(!in_home.exists());

    // 4. Missing parent directory
    let missing_parent = area.join("no-such-parent/out");
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&missing_parent.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "NotFound");

    // 5. Existing regular file at destination
    let existing_file = area.join("file.txt");
    std::fs::write(&existing_file, "keep me\n").unwrap();
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&existing_file.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Exists");
    assert_eq!(
        std::fs::read_to_string(&existing_file).unwrap(),
        "keep me\n"
    );

    // 6. Existing symlink to an empty directory at destination
    let real_empty = area.join("real-empty");
    std::fs::create_dir(&real_empty).unwrap();
    let link_dest = area.join("link-dest");
    std::os::unix::fs::symlink(&real_empty, &link_dest).unwrap();
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&link_dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Exists");
    assert_eq!(std::fs::read_dir(&real_empty).unwrap().count(), 0);

    // 7. Existing non-empty directory at destination
    let non_empty = area.join("non-empty");
    std::fs::create_dir(&non_empty).unwrap();
    std::fs::write(non_empty.join("existing.txt"), "untouched\n").unwrap();
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&non_empty.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "NotEmpty");
    assert_eq!(
        std::fs::read_to_string(non_empty.join("existing.txt")).unwrap(),
        "untouched\n"
    );
    assert!(!non_empty.join("a.txt").exists());

    // 8. 66-hex Commit/Layer identity passed as `branch` is rejected as InvalidInput
    let fake_id = "01".repeat(33);
    let valid_dest = area.join("out");
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&valid_dest.display().to_string())),
            ("branch", t(&fake_id)),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput");
    assert!(!valid_dest.exists());

    // 9. Non-existent branch is rejected as NoSuchBranch and creates no directory
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&valid_dest.display().to_string())),
            ("branch", t("missing")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "NoSuchBranch");
    assert!(!valid_dest.exists());

    // 10. Folder scope refuses `project.export` as out of scope
    let scoped = layerfs_api::Scope::parse(&SdkValue::map([
        ("project", t("shop/app")),
        ("folder", t("/sub")),
    ]))
    .unwrap();
    let scoped_err = layerfs_api::call_scoped(
        Some(&home),
        Some(&scoped),
        "project.export",
        SdkValue::map([
            ("project", t("shop/app")),
            ("destination", t(&valid_dest.display().to_string())),
        ]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(scoped_err.code, "InvalidInput");
    assert!(
        scoped_err.message.contains("out of scope: project.export"),
        "{scoped_err:?}"
    );
    assert!(!valid_dest.exists());

    for d in [home, source, area] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn branch_selection_and_pinned_snapshot_under_concurrent_writes() {
    let home = private_home("pin-home");
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("pin-src");
    std::fs::write(source.join("base.txt"), "v1\n").unwrap();
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/app")),
            ("name", t("feat")),
            ("from_branch", t("main")),
        ],
    );
    ok(
        &home,
        "file.write",
        &[
            ("project", t("shop/app")),
            ("branch", t("feat")),
            ("path", t("/base.txt")),
            ("content", t("v2-on-feat\n")),
        ],
    );
    // Write a large file on `feat` so export takes multiple streaming steps
    let big = vec![b'x'; 5 << 20];
    ok(
        &home,
        "file.write",
        &[
            ("project", t("shop/app")),
            ("branch", t("feat")),
            ("path", t("/large.bin")),
            ("content", SdkValue::Bytes(big)),
        ],
    );

    let area = tempdir("pin-out");
    let main_out = area.join("main");
    let feat_out = area.join("feat");

    // Export default (`main`) vs explicit (`feat`)
    let main_res = ok(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&main_out.display().to_string())),
        ],
    );
    assert_eq!(main_res.get_text("branch"), Some("main"));
    assert_eq!(
        std::fs::read_to_string(main_out.join("base.txt")).unwrap(),
        "v1\n"
    );
    assert!(!main_out.join("large.bin").exists());

    // During `feat` export's first progress callback, mutate `feat` and publish
    // it to `main`; the in-flight export must still produce the pinned snapshot!
    let mut mutated_mid_export = false;
    let feat_res = call_with_events(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&feat_out.display().to_string())),
            ("branch", t("feat")),
        ],
        &mut |ev| {
            if !mutated_mid_export
                && ev.get("progress").and_then(|p| p.get_text("phase")) == Some("export")
            {
                mutated_mid_export = true;
                ok(
                    &home,
                    "file.write",
                    &[
                        ("project", t("shop/app")),
                        ("branch", t("feat")),
                        ("path", t("/base.txt")),
                        ("content", t("v3-mutated-during-export\n")),
                    ],
                );
                ok(
                    &home,
                    "file.write",
                    &[
                        ("project", t("shop/app")),
                        ("branch", t("feat")),
                        ("path", t("/intruder.txt")),
                        ("content", t("should not appear\n")),
                    ],
                );
            }
        },
    )
    .unwrap();
    assert!(mutated_mid_export);
    assert_eq!(feat_res.get_text("branch"), Some("feat"));
    assert_eq!(
        std::fs::read_to_string(feat_out.join("base.txt")).unwrap(),
        "v2-on-feat\n"
    );
    assert!(!feat_out.join("intruder.txt").exists());

    for d in [home, source, area] {
        let _ = std::fs::remove_dir_all(d);
    }
}

fn move_store_file(from: &Path, to: &Path, store: &str) {
    let source = paths::store_dir(from, store);
    let target = paths::store_dir(to, store);
    std::fs::create_dir_all(&target).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("store.sqlite") {
            std::fs::rename(entry.path(), target.join(&name)).unwrap();
        }
    }
}

fn place_store(home: &Path, store: &str, machine: &str) {
    let record_path = paths::store_record(home, store);
    let record = read_record(&record_path).unwrap().unwrap();
    let record = home::with_field(record, "machine", t(machine));
    write_record(&record_path, &record).unwrap();
}

/// Stand-in proxy that drops the connection after forwarding `drop_after_events`
/// event frames for `project.export`.
fn drop_mid_export_proxy(listener: TcpListener, real: SocketAddr, drop_after_events: usize) {
    let seen = AtomicUsize::new(0);
    std::thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let mut client = client;
            let Ok(Some(hello)) = worker::read_frame_within(&mut client, worker::MAX_HELLO) else {
                continue;
            };
            let mut upstream = TcpStream::connect(real).unwrap();
            worker::write_frame(&mut upstream, &hello).unwrap();
            let ready = worker::read_frame(&mut upstream).unwrap().unwrap();
            worker::write_frame(&mut client, &ready).unwrap();
            let Some(request) = worker::read_frame(&mut client).unwrap() else {
                continue;
            };
            let is_export = request.get_text("op") == Some("project.export");
            worker::write_frame(&mut upstream, &request).unwrap();
            while let Some(frame) = worker::read_frame(&mut upstream).unwrap() {
                let is_event = frame.get("event").is_some();
                worker::write_frame(&mut client, &frame).unwrap();
                if is_export
                    && is_event
                    && seen.fetch_add(1, Ordering::SeqCst) + 1 >= drop_after_events
                {
                    break;
                }
                if !is_event {
                    break;
                }
            }
        }
    });
}

#[test]
fn worker_sqlite_export_and_midstream_disconnect_cleanup() {
    let home = private_home("wk-home");
    let root = tempdir("wk-guest");
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();

    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("wk-src");
    let _big = populate_sample(&source);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );
    // Ensure local Store handle is closed before moving file to worker root
    layerfs_api::close_idle(std::time::Duration::ZERO);
    move_store_file(&home, &root, "shop");
    place_store(&home, "shop", "box");

    let out_parent = tempdir("wk-out");
    let dest = out_parent.join("exported");

    // 1. Worker not running yet -> Unavailable, and no destination or staging directory left
    let (code, msg) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{msg}");
    assert!(!dest.exists());
    assert_eq!(std::fs::read_dir(&out_parent).unwrap().count(), 0);

    // 2. Start real worker and a proxy that drops mid-export after 2 event frames
    let real_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let real_addr = real_listener.local_addr().unwrap();
    let guest_root = root.clone();
    std::thread::spawn(move || worker::serve(real_listener, "k".to_owned(), guest_root));

    let proxy_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap().to_string();
    drop_mid_export_proxy(proxy_listener, real_addr, 2);
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&proxy_addr)), ("secret", t("k"))]),
    )
    .unwrap();

    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unknown");
    assert!(
        !dest.exists(),
        "destination must not be published on midstream failure"
    );
    assert_eq!(
        std::fs::read_dir(&out_parent).unwrap().count(),
        0,
        "staging directory (including 0o555 subdir) must be cleaned up on failure"
    );

    // 3. Point worker.json at the real worker and verify full worker export
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&real_addr.to_string())), ("secret", t("k"))]),
    )
    .unwrap();
    let mut events = Vec::new();
    let exported = call_with_events(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
        &mut |e| events.push(e),
    )
    .unwrap();
    assert_eq!(exported.get_text("machine"), Some("box"));
    assert_eq!(exported.get_int("files"), Some(9));
    assert_eq!(exported.get_int("directories"), Some(7));
    assert_eq!(exported.get_int("symlinks"), Some(2));
    assert_same_tree(&source, &dest);
    assert!(events.len() >= 4, "{events:?}");
    assert_eq!(
        std::fs::read_dir(&out_parent).unwrap().count(),
        1,
        "only the published destination directory remains"
    );

    unlock_readonly(&source);
    unlock_readonly(&dest);
    for d in [home, root, source, out_parent] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn synthetic_case_or_duplicate_collision_maps_already_exists_to_conflict_and_cleans_staging() {
    let home = private_home("conf-home");
    ok(&home, "store.create", &[("name", t("shop"))]);
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    place_store(&home, "shop", "box");
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();

    // Stand-in worker that emits a `prepare` event and a `part` containing two
    // offset-0 `Record::Write` entries that collide on the host filesystem
    // (`AlreadyExists` -> mapped to `Conflict`), inside a `0o555` directory so
    // `StagingGuard` Drop cleanup must also unlock and remove the staging tree.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let extra_parts_sent = std::sync::Arc::new(AtomicUsize::new(0));
    let worker_aborted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sent_clone = extra_parts_sent.clone();
    let aborted_clone = worker_aborted.clone();

    let worker_handle = std::thread::spawn(move || {
        // Connection 1: Part 1 collides locally -> client immediately closes connection
        // so the worker stops before sending all remaining parts.
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            let _ = worker::read_frame_within(&mut stream, worker::MAX_HELLO).unwrap();
            worker::write_frame(&mut stream, &worker::Hello::own().to_frame()).unwrap();
            let _ = worker::read_frame(&mut stream).unwrap();
            worker::write_frame(
                &mut stream,
                &SdkValue::map([("event", SdkValue::map([("phase", t("prepare"))]))]),
            )
            .unwrap();
            let mut part = Vec::new();
            let push_str = |buf: &mut Vec<u8>, s: &[u8]| {
                buf.extend_from_slice(&(s.len() as u32).to_be_bytes());
                buf.extend_from_slice(s);
            };
            // D "sub"
            part.push(b'D');
            push_str(&mut part, b"sub");
            // W "sub/readme.txt" offset 0 "first\n"
            part.push(b'W');
            push_str(&mut part, b"sub/readme.txt");
            part.extend_from_slice(&0u64.to_be_bytes());
            push_str(&mut part, b"first\n");
            // M "sub" mode 0o555 (so StagingGuard::drop exercises permission recovery)
            part.push(b'M');
            push_str(&mut part, b"sub");
            part.extend_from_slice(&0o555u32.to_be_bytes());
            part.extend_from_slice(&1_700_000_000i64.to_be_bytes());
            part.extend_from_slice(&0u32.to_be_bytes());
            // Collide on top-level "Dup.txt" + "Dup.txt"
            part.push(b'W');
            push_str(&mut part, b"Dup.txt");
            part.extend_from_slice(&0u64.to_be_bytes());
            push_str(&mut part, b"one\n");
            part.push(b'W');
            push_str(&mut part, b"Dup.txt");
            part.extend_from_slice(&0u64.to_be_bytes());
            push_str(&mut part, b"two\n");

            worker::write_frame(
                &mut stream,
                &SdkValue::map([("event", SdkValue::map([("part", SdkValue::Bytes(part))]))]),
            )
            .unwrap();

            // Attempt to stream 32 more 1-MiB parts; client abort must break the stream early.
            let chunk = vec![b'x'; 1 << 20];
            for i in 0..32 {
                let mut follow = Vec::new();
                follow.push(b'W');
                push_str(&mut follow, format!("extra-{i}.bin").as_bytes());
                follow.extend_from_slice(&0u64.to_be_bytes());
                push_str(&mut follow, &chunk);
                if worker::write_frame(
                    &mut stream,
                    &SdkValue::map([("event", SdkValue::map([("part", SdkValue::Bytes(follow))]))]),
                )
                .is_err()
                {
                    aborted_clone.store(true, Ordering::SeqCst);
                    break;
                }
                sent_clone.fetch_add(1, Ordering::SeqCst);
            }
        }

        // Connection 2: Worker sends valid part (1 file, 6 bytes) but reports
        // mismatched counts in its final reply -> release check fails before publish
        // and cleans up staging.
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            let _ = worker::read_frame_within(&mut stream, worker::MAX_HELLO).unwrap();
            worker::write_frame(&mut stream, &worker::Hello::own().to_frame()).unwrap();
            let _ = worker::read_frame(&mut stream).unwrap();
            worker::write_frame(
                &mut stream,
                &SdkValue::map([("event", SdkValue::map([("phase", t("prepare"))]))]),
            )
            .unwrap();
            let mut part = Vec::new();
            let push_str = |buf: &mut Vec<u8>, s: &[u8]| {
                buf.extend_from_slice(&(s.len() as u32).to_be_bytes());
                buf.extend_from_slice(s);
            };
            part.push(b'W');
            push_str(&mut part, b"ok.txt");
            part.extend_from_slice(&0u64.to_be_bytes());
            push_str(&mut part, b"hello\n");
            worker::write_frame(
                &mut stream,
                &SdkValue::map([("event", SdkValue::map([("part", SdkValue::Bytes(part))]))]),
            )
            .unwrap();
            worker::write_frame(
                &mut stream,
                &SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("snapshot", t("main@000000000000")),
                        ("node", t(&"01".repeat(33))),
                        ("root", t(&"02".repeat(32))),
                        ("files", SdkValue::Int(2)), // mismatch: LocalWriter wrote 1 file
                        ("directories", SdkValue::Int(0)),
                        ("symlinks", SdkValue::Int(0)),
                        ("bytes", SdkValue::Int(6)),
                    ]),
                )]),
            )
            .unwrap();
        }
    });
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&addr)), ("secret", t("k"))]),
    )
    .unwrap();

    let out_parent = tempdir("conf-out");
    let dest = out_parent.join("exported");
    let (code, message) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Conflict", "{message}");
    assert!(
        !dest.exists(),
        "destination must not exist after Conflict refusal"
    );
    assert_eq!(
        std::fs::read_dir(&out_parent).unwrap().count(),
        0,
        "staging directory (including 0o555 subdir) must be cleaned up after Conflict"
    );

    // 2. Verify mismatched worker counts fail before publish and clean staging
    let (code2, message2) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code2, "Failed", "{message2}");
    assert!(
        message2.contains("do not match snapshot"),
        "expected count mismatch failure, got: {message2}"
    );
    assert!(
        !dest.exists(),
        "destination must not be published on count mismatch"
    );
    assert_eq!(
        std::fs::read_dir(&out_parent).unwrap().count(),
        0,
        "staging directory must be cleaned up on count mismatch"
    );

    worker_handle.join().unwrap();
    assert!(
        worker_aborted.load(Ordering::SeqCst),
        "worker stream must be aborted when local write fails"
    );
    assert!(
        extra_parts_sent.load(Ordering::SeqCst) < 32,
        "worker must stop before sending all remaining parts (sent {})",
        extra_parts_sent.load(Ordering::SeqCst)
    );

    for d in [home, out_parent] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn shared_store_lease_and_concurrent_remove_admission() {
    let home = private_home("adm-home");
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir("adm-src");
    std::fs::write(source.join("base.txt"), "pinned-content\n").unwrap();
    std::fs::write(source.join("large.bin"), vec![b'a'; 5 << 20]).unwrap();
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );

    let area = tempdir("adm-out");

    // 1. Another live process holding a shared Store lease (`open/1` for PID 1)
    // does NOT block `project.export` (Store leases are shared across readers/exports).
    let lease_dir = paths::lease_dir(&home, "shop");
    std::fs::create_dir_all(&lease_dir).unwrap();
    let other_lease = lease_dir.join("1");
    std::fs::write(&other_lease, "1\n").unwrap();

    // 2. During an in-flight `project.export`:
    //    - same-Store `file.read` and a second concurrent `project.export` succeed;
    //    - exclusive Store operations (`store.remove`, `store.push`) are refused `InUse`;
    //    - `project.remove` drops the catalog stack while packs stay (`packs_kept: true`),
    //      leaving the in-flight pinned export intact while refusing later exports.
    let first_dest = area.join("first");
    let second_dest = area.join("second");
    let mut checked_mid_export = false;
    let first_res = call_with_events(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&first_dest.display().to_string())),
        ],
        &mut |ev| {
            if !checked_mid_export
                && ev.get("progress").and_then(|p| p.get_text("phase")) == Some("export")
            {
                checked_mid_export = true;

                // Same-Store read does not block on export's lease
                let read = ok(
                    &home,
                    "file.read",
                    &[
                        ("project", t("shop/app")),
                        ("branch", t("main")),
                        ("path", t("/base.txt")),
                    ],
                );
                assert_eq!(read.get_text("text"), Some("pinned-content\n"));

                // Same-Store concurrent export does not block on export's lease
                let second_res = ok(
                    &home,
                    "project.export",
                    &[
                        ("project", t("shop/app")),
                        ("destination", t(&second_dest.display().to_string())),
                    ],
                );
                assert_eq!(second_res.get_int("files"), Some(2));

                // Exclusive Store operations (`store.remove`, `store.push`) are refused `InUse`
                let (rm_code, rm_msg) =
                    call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
                assert_eq!(rm_code, "InUse", "{rm_msg}");
                let (push_code, push_msg) = call(
                    &home,
                    "store.push",
                    &[("name", t("shop")), ("machine", t("box"))],
                )
                .unwrap_err();
                assert_eq!(push_code, "InUse", "{push_msg}");

                // `project.remove` drops the LayerStack while keeping packs/objects;
                // the already-pinned export continues without corruption.
                let removed = ok(&home, "project.remove", &[("project", t("shop/app"))]);
                assert_eq!(removed.get("packs_kept"), Some(&SdkValue::Bool(true)));
            }
        },
    )
    .unwrap();
    assert!(checked_mid_export);
    assert_eq!(first_res.get_int("files"), Some(2));
    assert_same_tree(&source, &first_dest);
    assert_same_tree(&source, &second_dest);
    let _ = std::fs::remove_file(&other_lease);

    // After `project.remove`, a new export is refused `NoSuchProject`
    let after_rm_dest = area.join("after-rm");
    let (code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&after_rm_dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "NoSuchProject");
    assert!(!after_rm_dest.exists());

    // Re-init `shop/app` and verify Store-level `Busy` and `Unresolved` admission
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );
    let store_rec_path = paths::store_record(&home, "shop");
    let base_rec = read_record(&store_rec_path).unwrap().unwrap();

    // Active `op` by live PID 1 -> `Busy`
    let busy_rec = home::with_field(
        base_rec.clone(),
        "op",
        SdkValue::map([
            ("id", t("op-00000000000000ab")),
            ("name", t("store.push")),
            ("phase", t("record")),
            ("owner", SdkValue::map([("pid", SdkValue::Int(1))])),
            ("uncertain", SdkValue::Bool(false)),
        ]),
    );
    write_record(&store_rec_path, &busy_rec).unwrap();
    let busy_dest = area.join("busy-dest");
    let (busy_code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&busy_dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(busy_code, "Busy");
    assert!(!busy_dest.exists());

    // Uncertain `op` left by dead process -> `Unresolved`
    let unresolved_rec = home::with_field(
        base_rec,
        "op",
        SdkValue::map([
            ("id", t("op-00000000000000ab")),
            ("name", t("store.push")),
            ("phase", t("record")),
            ("owner", SdkValue::map([("pid", SdkValue::Int(99_999_999))])),
            ("uncertain", SdkValue::Bool(true)),
        ]),
    );
    write_record(&store_rec_path, &unresolved_rec).unwrap();
    let (unres_code, _) = call(
        &home,
        "project.export",
        &[
            ("project", t("shop/app")),
            ("destination", t(&busy_dest.display().to_string())),
        ],
    )
    .unwrap_err();
    assert_eq!(unres_code, "Unresolved");
    assert!(!busy_dest.exists());

    for d in [home, source, area] {
        let _ = std::fs::remove_dir_all(d);
    }
}
