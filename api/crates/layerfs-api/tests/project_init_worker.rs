//! `project.init` into a sqlite Store on another machine (#536 H11, H11a):
//! the caller reads the folder on its own host and copies it to that
//! machine's worker, which adds the copy and removes it; the caller
//! records the project. The folder here is under the caller's temp
//! directory, outside the worker's root.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker::{self, Endpoint};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-iw-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(
    home: &Path,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Result<SdkValue, (String, String)> {
    calls::call(Some(home), name, args, events).map_err(|r| (r.code, r.message))
}

fn init_args(project: &str, folder: &Path) -> SdkValue {
    SdkValue::map([
        ("project", t(project)),
        ("folder", t(&folder.display().to_string())),
    ])
}

/// Moves Store `store`'s file from `from`'s Store folder to `to`'s.
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

/// A home with machine `box`, whose worker record names `listener`.
fn home_with_box(tag: &str, listener: &TcpListener) -> PathBuf {
    let home = tempdir(tag);
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    let address = listener.local_addr().unwrap().to_string();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    home
}

fn serve(listener: TcpListener, root: &Path) {
    let root = root.to_owned();
    std::thread::spawn(move || worker::serve(listener, "k".to_owned(), root));
}

fn store_call(home: &Path, name: &str, project: &str, path: &str) -> SdkValue {
    let args = SdkValue::map([
        ("project", t(project)),
        ("branch", t("main")),
        ("path", t(path)),
    ]);
    call(home, name, args, &mut |_| {}).unwrap_or_else(|e| panic!("{name} {path}: {e:?}"))
}

/// Bytes past one 8 MiB part, in a pattern that is not one repeated byte.
fn big() -> Vec<u8> {
    (0..(9u32 << 20) + 123)
        .map(|i| (i * 31 % 251) as u8)
        .collect()
}

/// The folder on the caller's host: nested, an empty directory, a link,
/// an executable and a file larger than one part; answers that file.
fn sample(folder: &Path) -> Vec<u8> {
    std::fs::create_dir_all(folder.join("src/lib")).unwrap();
    std::fs::create_dir_all(folder.join("empty")).unwrap();
    std::fs::write(folder.join("README.md"), "on the caller\n").unwrap();
    std::fs::write(folder.join("src/lib/a.txt"), "nested\n").unwrap();
    std::os::unix::fs::symlink("src/lib/a.txt", folder.join("link")).unwrap();
    std::fs::write(folder.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    let executable = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(folder.join("run.sh"), executable).unwrap();
    // a read-only directory is copied with its mode
    std::fs::create_dir_all(folder.join("shut/in")).unwrap();
    std::fs::write(folder.join("shut/in/f"), "x").unwrap();
    let closed = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(folder.join("shut/in"), closed).unwrap();
    let big = big();
    std::fs::write(folder.join("src/big.bin"), &big).unwrap();
    big
}

#[test]
fn init_into_a_worker_s_store_copies_the_caller_s_folder() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = home_with_box("home", &listener);
    let root = tempdir("guest");

    // a Store on box: made on native, its file placed under the worker's root
    let made = call(
        &home,
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    );
    made.unwrap_or_else(|e| panic!("store.create: {e:?}"));
    move_store_file(&home, &root, "shop");
    place_store(&home, "shop", "box");

    // refused on this host before the machine is reached: box's worker is
    // not serving yet, so a call that reached it would wait, then fail
    let missing = tempdir("missing").join("nothing");
    let refusal = call(
        &home,
        "project.init",
        init_args("shop/web", &missing),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.0, "InvalidInput", "{}", refusal.1);
    assert!(refusal.1.contains("does not exist"), "{}", refusal.1);
    let plain = tempdir("plain").join("file");
    std::fs::write(&plain, "x").unwrap();
    let refusal = call(
        &home,
        "project.init",
        init_args("shop/web", &plain),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.0, "InvalidInput", "{}", refusal.1);
    assert!(refusal.1.contains("not a directory"), "{}", refusal.1);
    // a socket is refused before anything is sent, as Init refuses one
    let odd = tempdir("odd");
    let _socket = std::os::unix::net::UnixListener::bind(odd.join("s")).unwrap();
    let refusal = call(
        &home,
        "project.init",
        init_args("shop/web", &odd),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.0, "InvalidInput", "{}", refusal.1);
    assert!(refusal.1.contains("socket"), "{}", refusal.1);
    serve(listener, &root);

    let folder = tempdir("folder");
    let big = sample(&folder);

    let mut sent = Vec::new();
    let added = call(
        &home,
        "project.init",
        init_args("shop/web", &folder),
        &mut |event| {
            if let Some(progress) = event.get("progress") {
                if progress.get_text("phase") == Some("send") {
                    sent.push((progress.get_int("done"), progress.get_int("total")));
                }
            }
        },
    )
    .unwrap_or_else(|e| panic!("project.init: {e:?}"));
    assert_eq!(added.get_text("project"), Some("shop/web"));
    assert_eq!(added.get_text("machine"), Some("box"));
    assert!(added.get_text("root").is_some(), "{added:?}");
    // the caller sent the folder's file bytes, in more than one part
    let total = (big.len() + 14 + 7 + 18 + 1) as i64;
    assert_eq!(sent.first(), Some(&(Some(0), Some(total))), "{sent:?}");
    assert_eq!(sent.last(), Some(&(Some(total), Some(total))), "{sent:?}");
    assert!(sent.len() >= 3, "{sent:?}");
    // the caller records it; the worker's root holds the Store's project
    // and no copy
    let record = read_record(&paths::project_record(&home, "shop", "web")).unwrap();
    assert!(record.unwrap().get_text("stack").is_some());
    assert!(paths::project_dir(&root, "shop", "web").is_dir());
    assert!(!paths::store_file(&home, "shop").exists());
    let copies = std::fs::read_dir(root.join("incoming")).unwrap().count();
    assert_eq!(copies, 0);

    // read back through the worker
    let read = store_call(&home, "file.read", "shop/web", "/src/lib/a.txt");
    assert_eq!(read.get_text("text"), Some("nested\n"));
    let stat = store_call(&home, "file.stat", "shop/web", "/run.sh");
    assert_eq!(stat.get_int("mode"), Some(0o755), "{stat:?}");
    let stat = store_call(&home, "file.stat", "shop/web", "/src/big.bin");
    assert_eq!(stat.get_int("size"), Some(big.len() as i64), "{stat:?}");
    let stat = store_call(&home, "file.stat", "shop/web", "/empty");
    assert_eq!(stat.get_text("type"), Some("dir"), "{stat:?}");
    let stat = store_call(&home, "file.stat", "shop/web", "/link");
    assert_eq!(stat.get_text("type"), Some("symlink"), "{stat:?}");

    let again = call(
        &home,
        "project.init",
        init_args("shop/web", &folder),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(again.0, "Exists", "{}", again.1);

    open_up(&folder);
    let (missing, plain) = (missing.parent().unwrap(), plain.parent().unwrap());
    for dir in [&home, &root, &folder, &odd, missing, plain] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Lets the test remove what [`sample`] closed.
fn open_up(folder: &Path) {
    let open = std::fs::Permissions::from_mode(0o755);
    let _ = std::fs::set_permissions(folder.join("shut/in"), open);
}

/// A stand-in for box's worker that passes each request to the worker at
/// `real`, except those `refuse` picks, which it refuses `Failed`.
fn stand_in(
    listener: TcpListener,
    real: SocketAddr,
    refuse: impl Fn(&SdkValue) -> bool + Send + 'static,
) {
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let mut client = client.unwrap();
            let hello = worker::read_frame_within(&mut client, worker::MAX_HELLO);
            let mut upstream = TcpStream::connect(real).unwrap();
            worker::write_frame(&mut upstream, &hello.unwrap().unwrap()).unwrap();
            let ready = worker::read_frame(&mut upstream).unwrap().unwrap();
            worker::write_frame(&mut client, &ready).unwrap();
            let Some(request) = worker::read_frame(&mut client).unwrap() else {
                continue;
            };
            if refuse(&request) {
                let refusal = SdkValue::map([
                    ("code", t("Failed")),
                    ("message", t("kept")),
                    ("detail", SdkValue::Null),
                ]);
                let frame = SdkValue::map([("refused", refusal)]);
                worker::write_frame(&mut client, &frame).unwrap();
                continue;
            }
            worker::write_frame(&mut upstream, &request).unwrap();
            while let Some(frame) = worker::read_frame(&mut upstream).unwrap() {
                let answer = frame.get("event").is_none();
                worker::write_frame(&mut client, &frame).unwrap();
                if answer {
                    break;
                }
            }
        }
    });
}

/// `copy` holds what Init reads of `source`: kinds, permission bits (not a
/// link's), modification times to the nanosecond, link targets and bytes.
fn same_tree(source: &Path, copy: &Path) {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (
        std::fs::symlink_metadata(source).unwrap(),
        std::fs::symlink_metadata(copy).unwrap(),
    );
    let shown = source.display();
    assert_eq!(a.file_type(), b.file_type(), "{shown}");
    assert_eq!(
        (a.mtime(), a.mtime_nsec()),
        (b.mtime(), b.mtime_nsec()),
        "{shown}"
    );
    if a.file_type().is_symlink() {
        let target = std::fs::read_link(source).unwrap();
        assert_eq!(target, std::fs::read_link(copy).unwrap(), "{shown}");
        return;
    }
    assert_eq!(a.mode() & 0o7777, b.mode() & 0o7777, "{shown}");
    if a.is_file() {
        let bytes = std::fs::read(source).unwrap();
        assert!(bytes == std::fs::read(copy).unwrap(), "{shown}");
        return;
    }
    let names = |dir: &Path| {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let children = names(source);
    assert_eq!(children, names(copy), "{shown}");
    for name in children {
        same_tree(&source.join(&name), &copy.join(&name));
    }
}

#[test]
fn the_copy_holds_what_init_reads_of_the_folder() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = home_with_box("home2", &listener);
    let root = tempdir("kept");
    let real = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = real.local_addr().unwrap();
    serve(real, &root);
    // the Init is refused: the copy stays
    stand_in(listener, address, |request| {
        request.get_text("op") == Some("project.init")
    });
    let made = call(
        &home,
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    );
    made.unwrap_or_else(|e| panic!("store.create: {e:?}"));
    move_store_file(&home, &root, "shop");
    place_store(&home, "shop", "box");
    let folder = tempdir("source");
    sample(&folder);
    // a time with nanoseconds on the folder itself, set last
    let time = std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_789);
    let handle = std::fs::File::open(&folder).unwrap();
    handle.set_modified(time).unwrap();

    let refusal = call(
        &home,
        "project.init",
        init_args("shop/web", &folder),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal, ("Failed".to_owned(), "kept".to_owned()));
    let copies: Vec<_> = std::fs::read_dir(root.join("incoming"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    same_tree(&folder, &copies[0]);
    assert!(!paths::project_record(&home, "shop", "web").exists());

    open_up(&folder);
    open_up(&copies[0]);
    for dir in [home, root, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// One `project.stage` write of `records` to the worker at `endpoint`.
fn stage(endpoint: &Endpoint, records: Vec<u8>) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::map([
        ("staged", t("00112233445566ff")),
        ("step", t("write")),
        ("part", SdkValue::Bytes(records)),
    ]);
    worker::call(endpoint, "project.stage", args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// One record: `tag`, `path`, then each of `more` with its length.
fn record(tag: u8, path: &str, more: &[&[u8]]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(path.len() as u32).to_be_bytes());
    out.extend_from_slice(path.as_bytes());
    for field in more {
        out.extend_from_slice(&(field.len() as u32).to_be_bytes());
        out.extend_from_slice(field);
    }
    out
}

#[test]
fn a_folder_copy_stays_inside_its_staging_folder() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let root = tempdir("stage");
    serve(listener, &root);
    let staged = root.join("incoming/init-00112233445566ff");

    let refusal = stage(&endpoint, record(b'D', "../out", &[])).unwrap_err();
    assert_eq!(refusal.0, "Failed", "{}", refusal.1);
    assert!(!root.join("incoming/out").exists());
    // nothing is written through a link the copy made
    stage(&endpoint, record(b'L', "l", &[b".."])).unwrap();
    let refusal = stage(&endpoint, record(b'D', "l/out", &[])).unwrap_err();
    assert!(refusal.1.contains("not a directory"), "{}", refusal.1);
    assert!(!root.join("incoming/out").exists());
    // a directory a copied mode closed is still discarded
    stage(&endpoint, record(b'D', "shut", &[])).unwrap();
    let mut write = record(b'W', "shut/f", &[]);
    write.extend_from_slice(&0u64.to_be_bytes());
    write.extend_from_slice(&1u32.to_be_bytes());
    write.push(b'x');
    stage(&endpoint, write).unwrap();
    assert!(staged.join("shut/f").is_file());
    let closed = std::fs::Permissions::from_mode(0o500);
    std::fs::set_permissions(staged.join("shut"), closed).unwrap();
    let args = SdkValue::map([("staged", t("00112233445566ff")), ("step", t("discard"))]);
    worker::call(&endpoint, "project.stage", args, &mut |_| {}).unwrap();
    assert!(!staged.exists());
    // the id names one folder under incoming/, nothing else
    let args = SdkValue::map([("staged", t("../../x")), ("step", t("discard"))]);
    let refusal = worker::call(&endpoint, "project.stage", args, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_copy_that_fails_midway_is_discarded() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = home_with_box("home3", &listener);
    let root = tempdir("midway");
    let real = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = real.local_addr().unwrap();
    serve(real, &root);
    // the second part is refused
    let writes = std::sync::atomic::AtomicUsize::new(0);
    stand_in(listener, address, move |request| {
        let args = request.get("args").cloned().unwrap_or_default();
        request.get_text("op") == Some("project.stage")
            && args.get_text("step") == Some("write")
            && writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1
    });
    let made = call(
        &home,
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    );
    made.unwrap_or_else(|e| panic!("store.create: {e:?}"));
    move_store_file(&home, &root, "shop");
    place_store(&home, "shop", "box");
    let folder = tempdir("half");
    sample(&folder);

    let refusal = call(
        &home,
        "project.init",
        init_args("shop/web", &folder),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal, ("Failed".to_owned(), "kept".to_owned()));
    // the first part was written, then discarded
    let copies = std::fs::read_dir(root.join("incoming")).unwrap().count();
    assert_eq!(copies, 0);
    assert!(!paths::project_record(&home, "shop", "web").exists());

    open_up(&folder);
    for dir in [home, root, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
