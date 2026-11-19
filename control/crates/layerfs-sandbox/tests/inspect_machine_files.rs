//! `machine.files` on the side that holds a machine's data
//! (layerfs-project#178 step 4, the inspect kind `machine_files`): the
//! roots ranked by role, a LayerFS folder read beneath itself with what
//! LayerFS keeps private, a run folder's Workspace mounts answered from
//! their records, and a machine's `/`: system folders, the world-readable
//! rule, links resolved inside the root, mounts that are not entered.
//! Pages by name cursor and the soft deadline are the walker's, checked
//! here in this view.
//!
//! Nothing runs: a machine is a folder laid out as a worker's root, with
//! `state` and `bundle` beside it. Scratch folders are under the build's
//! own temporary folder, or under `/tmp` where a test reads them from `/`.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::names::MachineName;
use layerfs_sandbox::inspect::{self, FilesRequest, Side, HARD, SOFT};
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
use std::cell::Cell;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// The inspect permits are the process's: the tests that take one take
/// this turn.
static PERMITS: Mutex<()> = Mutex::new(());

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, PermissionsExt::from_mode(bits)).unwrap();
}

/// A machine as its worker sees it: `layerfs/data` (the worker's root)
/// with a Store, Sandbox `s1` and its run folder, and `state` and `bundle`
/// beside it.
struct Machine {
    scratch: PathBuf,
    layerfs: PathBuf,
    data: PathBuf,
    run: PathBuf,
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = mode_all(&self.scratch);
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// Lets a scratch tree with closed folders be removed.
fn mode_all(folder: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if std::fs::symlink_metadata(&path)?.is_dir() {
            mode(&path, 0o700);
            mode_all(&path)?;
        }
    }
    Ok(())
}

fn machine_in(parent: &Path, tag: &str) -> Machine {
    let scratch = parent.join(format!(
        "mfiles-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let layerfs = scratch.join("layerfs");
    let data = layerfs.join("data");
    let run = paths::run_root(&data).join("layerfs-0a1b2c3d-s1");
    for folder in ["tmp", "workspaces", "config", "overlay"] {
        std::fs::create_dir_all(run.join(folder)).unwrap();
    }
    for folder in ["store/shop/open", "incoming", "log"] {
        std::fs::create_dir_all(data.join(folder)).unwrap();
    }
    std::fs::create_dir_all(paths::run_root(&data).join("images")).unwrap();
    for folder in [&scratch, &layerfs, &data] {
        mode(folder, 0o755);
    }
    let write = |path: PathBuf, text: &str, bits: u32| {
        std::fs::write(&path, text).unwrap();
        mode(&path, bits);
    };
    write(run.join("daemon.log"), "listening\n", 0o600);
    write(run.join("config/daemon.setup"), "private key", 0o600);
    write(run.join("overlay/1.sqlite"), "overlay", 0o644);
    write(run.join("tmp/notes.txt"), "by a command\n", 0o644);
    write(data.join("store/shop/store.sqlite"), "db", 0o644);
    write(data.join("store/shop/store.sqlite-wal"), "wal", 0o644);
    write(data.join("store/shop/record.json"), "{}", 0o644);
    write(data.join("store/shop/lock"), "", 0o644);
    write(data.join("store/shop/open/4242"), "lease", 0o644);
    write(data.join("log/worker.log"), "launched\n", 0o600);
    write(data.join("incoming/private"), "staged\n", 0o600);
    sandbox(&data, "s1", &run);
    write(paths::sandbox_control(&data, "s1"), "keys", 0o600);
    std::fs::create_dir_all(layerfs.join("state")).unwrap();
    write(layerfs.join("state/secret"), "s3cret", 0o600);
    write(layerfs.join("state/worker.pid"), "77\n", 0o644);
    mode(&layerfs.join("state"), 0o700);
    std::fs::create_dir_all(layerfs.join("bundle")).unwrap();
    std::fs::write(layerfs.join("bundle/layerfs"), b"\x7fELF\0\0\0\x01").unwrap();
    Machine {
        scratch,
        layerfs,
        data,
        run,
    }
}

fn machine(tag: &str) -> Machine {
    machine_in(Path::new(env!("CARGO_TARGET_TMPDIR")), tag)
}

/// Sandbox `name` of `root` as the side that runs it records it; its
/// daemon is no process: a read never looks for it.
fn sandbox(root: &Path, name: &str, directory: &Path) {
    let daemon = DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: (directory.file_name().unwrap().to_string_lossy()).into_owned(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: directory.display().to_string(),
        process: (4_000_000, 1, format!("{:032x}", 7)),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(root, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t(name)),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

fn args(fields: &[(&str, SdkValue)]) -> SdkValue {
    let mut all = vec![("machine".to_owned(), t("box"))];
    all.extend(fields.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    SdkValue::Map(all)
}

/// One request on a worker's side, never out of time.
fn read(root: &Path, fields: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    inspect::machine_page(&Side::Worker(root), &args(fields), &|| false)
}

fn at(root: &Path, path: &Path, more: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![("path", t(path.to_str().unwrap()))];
    fields.extend(more.iter().cloned());
    read(root, &fields)
}

fn entries(answer: &SdkValue) -> &[SdkValue] {
    assert_eq!(answer.get_text("kind"), Some("folder"), "{answer:?}");
    match answer.get("entries") {
        Some(SdkValue::List(entries)) => entries,
        _ => panic!("{answer:?}"),
    }
}

/// Each entry as `<path below base> <kind>[ <role>]`.
fn shown(answer: &SdkValue, base: &Path) -> Vec<String> {
    entries(answer)
        .iter()
        .map(|entry| {
            let path = Path::new(entry.get_text("path").unwrap());
            let below = path.strip_prefix(base).unwrap_or(path).display();
            let kind = entry.get_text("kind").unwrap();
            match entry.get_text("role") {
                Some(role) => format!("{below} {kind} {role}"),
                None => format!("{below} {kind}"),
            }
        })
        .collect()
}

fn entry<'a>(answer: &'a SdkValue, path: &Path) -> &'a SdkValue {
    let found = (entries(answer).iter()).find(|entry| entry.get_text("path") == path.to_str());
    found.unwrap_or_else(|| panic!("no entry {} in {answer:?}", path.display()))
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

fn roles(answer: &SdkValue) -> Vec<&str> {
    (entries(answer).iter())
        .map(|entry| entry.get_text("role").unwrap())
        .collect()
}

#[test]
fn the_roots_are_the_layerfs_folders_ranked_by_role_then_the_machine() {
    let fx = machine("roots");
    let roots = read(&fx.data, &[]).unwrap();
    println!("machine.files roots: {}", json(&roots));
    assert_eq!(
        roles(&roots),
        [
            "data",
            "stores",
            "sandbox_records",
            "run_folders",
            "images",
            "staging",
            "logs",
            "state",
            "bundle",
            "machine"
        ]
    );
    let paths_listed: Vec<_> = (entries(&roots).iter())
        .map(|entry| entry.get_text("path").unwrap().to_owned())
        .collect();
    let data = |below: &str| fx.data.join(below).display().to_string();
    let beside = |name: &str| fx.layerfs.join(name).display().to_string();
    assert_eq!(
        paths_listed,
        [
            fx.data.display().to_string(),
            data("store"),
            data("sandbox"),
            data("machine/native/run"),
            data("machine/native/run/images"),
            data("incoming"),
            data("log"),
            beside("state"),
            beside("bundle"),
            "/".to_owned(),
        ]
    );
    // every root is a folder with its own `stat`; the data root says what
    // its filesystem has left, and no other root does
    for root in entries(&roots) {
        assert_eq!(root.get_text("kind"), Some("folder"));
        assert_eq!(root.get_int("depth"), Some(1));
        assert!(root.get_int("mode").is_some() && root.get_int("mtime").is_some());
        let free = root.get_int("free_bytes");
        match root.get_text("role") {
            Some("data") => assert!(free.unwrap() > 0),
            _ => assert!(root.get("free_bytes").is_none()),
        }
    }
    assert_eq!(entries(&roots)[7].get_int("mode"), Some(0o700));
    assert_eq!(roots.get("next"), Some(&SdkValue::Null));
    assert!(!roots.get_bool("has_more"));

    // only what is there is listed: a custom image's folder once it is
    // copied in, and no folder that is a link or was never made
    std::fs::create_dir(fx.layerfs.join("image")).unwrap();
    std::fs::remove_dir_all(fx.data.join("incoming")).unwrap();
    std::fs::remove_dir_all(fx.layerfs.join("bundle")).unwrap();
    symlink(&fx.data, fx.layerfs.join("bundle")).unwrap();
    let fewer = read(&fx.data, &[]).unwrap();
    assert_eq!(
        roles(&fewer),
        [
            "data",
            "stores",
            "sandbox_records",
            "run_folders",
            "images",
            "logs",
            "state",
            "image",
            "machine"
        ]
    );

    // a home is its owner's own computer: its LayerFS folders, and no `/`
    let home = fx.scratch.join("home");
    home::private_dir(&home).unwrap();
    home::ensure(&home).unwrap();
    for folder in ["store", "sandbox", "machine/native/run", "log", "secret"] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    let native = inspect::machine_page(&Side::Home(&home), &args(&[]), &|| false).unwrap();
    println!("machine.files native roots: {}", json(&native));
    assert_eq!(
        roles(&native),
        ["stores", "sandbox_records", "run_folders", "logs"]
    );
    assert!(entries(&native)
        .iter()
        .all(|root| root.get("free_bytes").is_none()));
    // a cursor continues a folder, and the roots are none
    invalid(read(&fx.data, &[("cursor", t("/x"))]), "name its `path`");
    invalid(read(&fx.data, &[("limit", SdkValue::Int(0))]), "`limit`");
}

#[test]
fn a_layerfs_folder_is_read_beneath_itself_and_keeps_what_layerfs_keeps() {
    let fx = machine("folders");
    let data = &fx.data;
    // the data root: its folders, each with its role where it has one
    let top = at(data, data, &[]).unwrap();
    assert_eq!(
        shown(&top, data),
        [
            "incoming folder staging",
            "log folder logs",
            "machine folder",
            "sandbox folder sandbox_records",
            "store folder stores"
        ]
    );
    let deep = at(data, &data.join("machine"), &[("depth", SdkValue::Int(3))]).unwrap();
    assert_eq!(
        shown(&deep, &data.join("machine")),
        [
            "native folder",
            "native/run folder run_folders",
            "native/run/images folder images",
            "native/run/layerfs-0a1b2c3d-s1 folder"
        ]
    );

    // a Store: its database, its companions, its lock and its leases are
    // listed with their size and never read
    let store = data.join("store/shop");
    let listed = at(data, &store, &[("depth", SdkValue::Int(2))]).unwrap();
    assert_eq!(
        shown(&listed, &store),
        [
            "lock restricted",
            "open restricted",
            "record.json file",
            "store.sqlite restricted",
            "store.sqlite-wal restricted"
        ]
    );
    assert_eq!(
        entry(&listed, &store.join("store.sqlite")).get_int("size"),
        Some(2)
    );
    for kept in ["store.sqlite", "store.sqlite-wal", "lock", "open"] {
        invalid(at(data, &store.join(kept), &[]), "kept private by LayerFS");
    }
    invalid(
        at(data, &store.join("open/4242"), &[]),
        "is in a folder that is listed, not read",
    );
    let record = at(data, &store.join("record.json"), &[]).unwrap();
    assert_eq!(record.get_text("content"), Some("{}"));
    invalid(
        at(data, &paths::sandbox_control(data, "s1"), &[]),
        "kept private by LayerFS",
    );

    // a run folder: its key and its overlay databases restricted, its log
    // a log, and its Workspace mounts from their records. Where they
    // would be mounted there is a FIFO and a folder nobody may open:
    // neither is touched
    let made = std::process::Command::new("mkfifo")
        .arg(fx.run.join("workspaces/1"))
        .status();
    assert!(made.unwrap().success());
    std::fs::create_dir(fx.run.join("workspaces/2")).unwrap();
    mode(&fx.run.join("workspaces/2"), 0o000);
    let id = format!("{:064x}", 1);
    let mounted = fx.run.join("workspaces/1");
    let record = WorkspaceRecord::mounted(id.clone(), None, "main", mounted.to_str().unwrap(), 0);
    let folder = paths::workspace_dir(data, "s1");
    record.write(&folder.join(format!("{id}.json"))).unwrap();
    let started = Instant::now();
    let run = at(data, &fx.run, &[("depth", SdkValue::Int(2))]).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        shown(&run, &fx.run),
        [
            "config restricted",
            "daemon.log file log",
            "overlay restricted",
            "tmp folder",
            "tmp/notes.txt file",
            "workspaces folder",
            "workspaces/1 mount",
            "workspaces/2 mount"
        ]
    );
    let run_shown = fx.run.display();
    assert_eq!(
        json(entry(&run, &mounted)),
        format!(
            r#"{{"path":"{run_shown}/workspaces/1","depth":2,"kind":"mount","size":null,"mode":null,"mtime":null,"workspace":"s1:{run_shown}/workspaces/1"}}"#
        )
    );
    // one no record names is a mount all the same, with no handle
    assert_eq!(
        json(entry(&run, &fx.run.join("workspaces/2"))),
        format!(
            r#"{{"path":"{run_shown}/workspaces/2","depth":2,"kind":"mount","size":null,"mode":null,"mtime":null}}"#
        )
    );
    // the records a request carries name it: what the home sends for a
    // Sandbox whose daemon it reaches directly
    let sent = SdkValue::List(vec![SdkValue::map([
        ("id", t("7f")),
        ("path", t(fx.run.join("workspaces/2").to_str().unwrap())),
    ])]);
    let carried = at(data, &fx.run.join("workspaces"), &[("workspaces", sent)]).unwrap();
    let ids: Vec<_> = (entries(&carried).iter())
        .map(|entry| entry.get_text("workspace").unwrap().to_owned())
        .collect();
    // the recorded one by its handle (`s1:<path>`, call surface 2.0), the
    // carried one, which names no handle, by its id
    let handle = format!("s1:{run_shown}/workspaces/1");
    assert_eq!(ids, [handle.as_str(), "7f"]);
    for inside in ["workspaces/1", "workspaces/2"] {
        invalid(at(data, &fx.run.join(inside), &[]), "is a mount point");
    }
    invalid(
        at(data, &fx.run.join("workspaces/2/src"), &[]),
        "is in a folder that is listed, not read",
    );
    invalid(
        at(data, &fx.run.join("config/daemon.setup"), &[]),
        "is in a folder that is listed, not read",
    );
    invalid(
        at(data, &fx.run.join("overlay/1.sqlite"), &[]),
        "is in a folder that is listed, not read",
    );
    invalid(at(data, &fx.run.join("daemon.log"), &[]), "is a log");
    invalid(at(data, &data.join("log/worker.log"), &[]), "is a log");
    let notes = at(data, &fx.run.join("tmp/notes.txt"), &[]).unwrap();
    assert_eq!(notes.get_text("content"), Some("by a command\n"));
    mode(&fx.run.join("workspaces/2"), 0o700);

    // a file only its owner may read is LayerFS's own here, and is read:
    // the world-readable rule is for what is outside the LayerFS folders
    let private = at(data, &data.join("incoming/private"), &[]).unwrap();
    assert_eq!(private.get_text("content"), Some("staged\n"));
    // no link is followed, in or out
    symlink(data.join("log"), data.join("incoming/in")).unwrap();
    let staged = at(data, &data.join("incoming"), &[]).unwrap();
    assert_eq!(
        shown(&staged, data),
        ["incoming/in symlink", "incoming/private file"]
    );
    assert_eq!(
        entry(&staged, &data.join("incoming/in")).get_text("target"),
        data.join("log").to_str()
    );
    invalid(at(data, &data.join("incoming/in"), &[]), "symbolic link");
    let (code, _) = refused(at(data, &data.join("incoming/none"), &[]));
    assert_eq!(code, "NotFound");

    // beside the data root: the worker's secret is restricted, its pid a
    // file; the bundle's binary is told binary
    let state = fx.layerfs.join("state");
    let listed = at(data, &state, &[]).unwrap();
    assert_eq!(
        shown(&listed, &state),
        ["secret restricted", "worker.pid file"]
    );
    assert_eq!(
        entry(&listed, &state.join("secret")).get_int("size"),
        Some(6)
    );
    invalid(
        at(data, &state.join("secret"), &[]),
        "kept private by LayerFS",
    );
    let pid = at(data, &state.join("worker.pid"), &[]).unwrap();
    assert_eq!(pid.get_text("content"), Some("77\n"));
    let binary = at(data, &fx.layerfs.join("bundle/layerfs"), &[]).unwrap();
    assert_eq!(
        json(&binary),
        r#"{"kind":"file","size":8,"offset":0,"content":"","binary":true,"next":null}"#
    );
}

#[test]
fn the_image_cache_keeps_its_locks_and_a_home_keeps_its_own_lock_files() {
    let fx = machine("locks");
    let data = &fx.data;
    // the image cache: one lock and one sidecar per digest, beside the
    // unpacked image
    let images = fx.run.parent().unwrap().join("images");
    let digest = format!("{:064x}", 7);
    std::fs::write(images.join(format!("{digest}.2.lock")), "").unwrap();
    std::fs::write(images.join(format!("{digest}.2.json")), "{}").unwrap();
    let listed = at(data, &images, &[]).unwrap();
    assert_eq!(
        shown(&listed, &images),
        [
            format!("{digest}.2.json file"),
            format!("{digest}.2.lock restricted")
        ]
    );
    invalid(
        at(data, &images.join(format!("{digest}.2.lock")), &[]),
        "kept private by LayerFS",
    );
    let sidecar = at(data, &images.join(format!("{digest}.2.json")), &[]).unwrap();
    assert_eq!(sidecar.get_text("content"), Some("{}"));
    // a lock file a command wrote in its home is its own file: it is read
    std::fs::write(fx.run.join("tmp/Cargo.lock"), "version = 4").unwrap();
    let own = at(data, &fx.run.join("tmp/Cargo.lock"), &[]).unwrap();
    assert_eq!(own.get_text("content"), Some("version = 4"));
}

#[test]
fn a_home_reads_only_its_layerfs_folders() {
    let _turn = PERMITS.lock().unwrap_or_else(|poison| poison.into_inner());
    let fx = machine("native");
    let home = fx.scratch.join("home");
    home::private_dir(&home).unwrap();
    home::ensure(&home).unwrap();
    for folder in [
        "store/shop",
        "sandbox",
        "machine/native/run",
        "machine/box",
        "log",
        "secret",
    ] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    std::fs::write(home.join("store/shop/record.json"), "{}").unwrap();
    std::fs::write(home.join("store/shop/store.sqlite"), "db").unwrap();
    std::fs::write(home.join("machine/box/worker.json"), "secret").unwrap();
    std::fs::write(home.join("secret/key"), "secret").unwrap();
    let native = MachineName::parse("native").unwrap();
    let request = |path: &Path| FilesRequest {
        path: Some(path.to_str().unwrap().to_owned()),
        ..FilesRequest::default()
    };
    // the call itself, in this process
    let roots = inspect::machine_files(&home, &native, &FilesRequest::default()).unwrap();
    assert_eq!(
        roles(&roots),
        ["stores", "sandbox_records", "run_folders", "logs"]
    );
    let store = home.join("store/shop");
    let listed = inspect::machine_files(&home, &native, &request(&store)).unwrap();
    assert_eq!(
        shown(&listed, &store),
        ["record.json file", "store.sqlite restricted"]
    );
    let file = inspect::machine_files(&home, &native, &request(&store.join("record.json")));
    assert_eq!(file.unwrap().get_text("content"), Some("{}"));
    assert_eq!(inspect::running(), 0);
    // anything else of the computer is refused, the home's own secrets
    // included: nothing of it is opened
    let outside = [
        Path::new("/").to_owned(),
        Path::new("/etc/passwd").to_owned(),
        home.clone(),
        home.join("secret/key"),
        home.join("machine/box/worker.json"),
        home.join("machine/native"),
    ];
    for path in outside {
        let refusal = inspect::machine_files(&home, &native, &request(&path)).unwrap_err();
        println!(
            "machine.files refusal: {}: {}",
            refusal.code, refusal.message
        );
        assert_eq!(refusal.code, "InvalidInput", "{}", path.display());
        assert!(
            refusal
                .message
                .contains("is outside this machine's LayerFS folders"),
            "{}",
            refusal.message
        );
    }
    // options are checked before the machine is looked up, and a machine
    // nobody recorded is not read
    let wide = FilesRequest {
        depth: Some(33),
        ..request(&store)
    };
    let nobody = MachineName::parse("nobody").unwrap();
    let refusal = inspect::machine_files(&home, &nobody, &wide).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    let lost = inspect::machine_files(&home, &nobody, &FilesRequest::default()).unwrap_err();
    assert_ne!(lost.code, "InvalidInput", "{lost:?}");
    println!(
        "machine.files unknown machine: {}: {}",
        lost.code, lost.message
    );
}

#[test]
fn a_walk_pages_by_name_and_answers_what_it_has_at_its_soft_budget() {
    let fx = machine("pages");
    let folder = fx.data.join("incoming/big");
    std::fs::create_dir_all(folder.join("m")).unwrap();
    let mut expected = Vec::new();
    for n in 0..1200 {
        std::fs::write(folder.join(format!("e{n:04}")), "").unwrap();
        expected.push(format!("e{n:04}"));
    }
    for n in 0..20 {
        std::fs::write(folder.join(format!("m/in{n:02}")), "").unwrap();
    }
    expected.push("m".to_owned());
    expected.extend((0..20).map(|n| format!("m/in{n:02}")));
    // every page from the `next` of the one before: 1221 entries in tree
    // order, 500 a page
    let (mut listed, mut pages, mut cursor) = (Vec::new(), 0, None::<String>);
    loop {
        let mut fields = vec![("depth", SdkValue::Int(2))];
        fields.extend(cursor.iter().map(|cursor| ("cursor", t(cursor))));
        let page = at(&fx.data, &folder, &fields).unwrap();
        pages += 1;
        assert!(pages < 10, "the walk does not end");
        for entry in entries(&page) {
            let path = Path::new(entry.get_text("path").unwrap());
            listed.push(path.strip_prefix(&folder).unwrap().display().to_string());
        }
        if !page.get_bool("has_more") {
            break;
        }
        assert_eq!(page.get_text("reason"), Some("limit"));
        assert_eq!(entries(&page).len(), 500);
        cursor = Some(page.get_text("next").unwrap().to_owned());
    }
    assert_eq!(pages, 3);
    assert_eq!(listed, expected);

    // a clock that runs out at its eighth look: the walker looks at it
    // once while it reads the folder's 1201 names, then before each entry
    let looks = Cell::new(0);
    let spent = || {
        looks.set(looks.get() + 1);
        looks.get() > 7
    };
    let asked = args(&[("path", t(folder.to_str().unwrap()))]);
    let page = inspect::machine_page(&Side::Worker(&fx.data), &asked, &spent).unwrap();
    assert_eq!(entries(&page).len(), 6);
    assert!(page.get_bool("has_more"));
    assert_eq!(page.get_text("reason"), Some("deadline"));
    assert_eq!(page.get_text("next"), folder.join("e0005").to_str());

    // through the request's own budget, as the worker serves the kind: a
    // request that has used its four seconds before the walk starts
    // answers no entry, in time, and says why
    let _turn = PERMITS.lock().unwrap_or_else(|poison| poison.into_inner());
    let asked = home::with_field(asked, "kind", t("machine_files"));
    let (root, request) = (fx.data.clone(), asked.clone());
    let started = Instant::now();
    let late = inspect::admit(move |budget| {
        std::thread::sleep(SOFT + Duration::from_millis(100));
        inspect::serve(&Side::Worker(&root), &request, budget)
    })
    .unwrap();
    assert!(started.elapsed() < HARD, "{:?}", started.elapsed());
    assert_eq!(
        json(&late),
        r#"{"kind":"folder","entries":[],"next":null,"has_more":true,"reason":"deadline"}"#
    );
    assert_eq!(inspect::running(), 0);
    let root = fx.data.clone();
    let whole = inspect::admit(move |budget| inspect::serve(&Side::Worker(&root), &asked, budget));
    assert_eq!(entries(&whole.unwrap()).len(), 500);
}

#[test]
fn from_the_root_system_folders_are_listed_and_never_entered() {
    let fx = machine("slash");
    let data = &fx.data;
    let slash = Path::new("/");
    let top = at(data, slash, &[("limit", SdkValue::Int(1000))]).unwrap();
    println!("machine.files / : {}", json(&top));
    let dev = entry(&top, Path::new("/dev"));
    assert_eq!(
        json(dev),
        r#"{"path":"/dev","depth":1,"kind":"system","size":null,"mode":null,"mtime":null}"#
    );
    #[cfg(target_os = "linux")]
    for system in ["/proc", "/sys"] {
        assert_eq!(
            entry(&top, Path::new(system)).get_text("kind"),
            Some("system")
        );
    }
    // deeper, nothing under a system folder is listed
    let deep = at(
        data,
        slash,
        &[("depth", SdkValue::Int(2)), ("limit", SdkValue::Int(1000))],
    )
    .unwrap();
    assert!(entries(&deep).iter().all(|entry| {
        let path = entry.get_text("path").unwrap();
        !path.starts_with("/dev/") && !path.starts_with("/proc/") && !path.starts_with("/sys/")
    }));
    invalid(at(data, Path::new("/dev"), &[]), "is a system folder");
    invalid(
        at(data, Path::new("/dev/null"), &[]),
        "is in a system folder",
    );
    #[cfg(target_os = "linux")]
    {
        invalid(
            at(data, Path::new("/proc/self/environ"), &[]),
            "is in a system folder",
        );
        invalid(
            at(data, Path::new("/sys/kernel"), &[]),
            "is in a system folder",
        );
    }

    // a program every system has, read as any user may: binary. On an
    // image with a merged `/usr`, `/bin` is a link and the path leads
    // through it
    let ls = Path::new("/bin/ls");
    match std::fs::metadata(ls) {
        Ok(found) if found.is_file() && !std::fs::symlink_metadata(ls).unwrap().is_symlink() => {
            let through = std::fs::symlink_metadata("/bin").unwrap().is_symlink();
            let read = at(data, ls, &[("length", SdkValue::Int(64))]);
            match (through, cfg!(target_os = "macos")) {
                // macOS resolves no link in this view
                (true, true) => invalid(read, "symbolic link"),
                _ => {
                    let file = read.unwrap();
                    assert_eq!(file.get_text("kind"), Some("file"));
                    assert!(file.get_bool("binary"));
                    assert_eq!(file.get_int("size"), i64::try_from(found.len()).ok());
                    println!("/bin/ls: binary, read through a link: {through}");
                }
            }
        }
        _ => println!("NOT_RUN /bin/ls: no such regular file here"),
    }

    // a file not every user may read is listed and never opened
    let closed = [
        "/etc/shadow",
        "/etc/gshadow",
        "/usr/bin/sudo",
        "/etc/sudoers",
    ];
    let found = closed.iter().map(Path::new).find(|path| {
        let real = !std::fs::symlink_metadata(path.parent().unwrap()).is_ok_and(|m| m.is_symlink());
        let stat = std::fs::symlink_metadata(path);
        real && stat.is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o004 == 0)
    });
    match found {
        Some(path) => {
            let folder = path.parent().unwrap();
            let listed = at(data, folder, &[("limit", SdkValue::Int(1000))]).unwrap();
            match (entries(&listed).iter()).find(|e| e.get_text("path") == path.to_str()) {
                Some(shown) => {
                    println!("machine.files unreadable: {}", json(shown));
                    assert_eq!(shown.get_text("kind"), Some("unreadable"));
                    assert!(shown.get_int("size").is_some());
                }
                None => assert!(
                    listed.get_bool("has_more"),
                    "{} is not listed",
                    path.display()
                ),
            }
            let refusal = at(data, path, &[]).unwrap_err();
            println!(
                "machine.files refusal: {}: {}",
                refusal.code, refusal.message
            );
            assert_eq!(refusal.code, "InvalidInput");
            assert!(
                refusal.message.contains("is not world-readable"),
                "{}",
                refusal.message
            );
        }
        None => println!("NOT_RUN unreadable file: none of {closed:?} is a closed file here"),
    }
    let (code, _) = refused(at(data, Path::new("/no-such-entry-of-root"), &[]));
    assert_eq!(code, "NotFound");
}

/// A machine laid out under `/tmp`, where a read from `/` reaches it: the
/// world-readable rule, links resolved inside the root, and LayerFS
/// folders listed apart. Linux only: macOS resolves no link in this view
/// and keeps its temporary folders on another volume than `/`.
#[cfg(target_os = "linux")]
#[test]
fn from_the_root_only_what_every_user_may_read_is_opened() {
    let tmp = Path::new("/tmp");
    let fx = machine_in(tmp, "world");
    let data = &fx.data;
    let open = fx.scratch.join("open");
    std::fs::create_dir_all(open.join("closed")).unwrap();
    std::fs::create_dir_all(open.join("nox")).unwrap();
    std::fs::create_dir_all(open.join("dir")).unwrap();
    let write = |name: &str, bits: u32| {
        std::fs::write(open.join(name), "text\n").unwrap();
        mode(&open.join(name), bits);
    };
    write("a.txt", 0o644);
    write("b.txt", 0o600);
    write("closed/in.txt", 0o644);
    write("dir/in.txt", 0o644);
    write("store.sqlite", 0o644);
    symlink("dir", open.join("link")).unwrap();
    symlink("/proc", open.join("proc")).unwrap();
    symlink(fx.layerfs.join("state"), open.join("state")).unwrap();
    symlink(data, open.join("data")).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(open.join("pipe"))
        .status();
    assert!(made.unwrap().success());
    mode(&open, 0o755);
    mode(&open.join("dir"), 0o755);
    mode(&open.join("closed"), 0o700);
    mode(&open.join("nox"), 0o744);

    let listed = at(data, &open, &[("depth", SdkValue::Int(2))]).unwrap();
    println!("machine.files world: {}", json(&listed));
    assert_eq!(
        shown(&listed, &open),
        [
            "a.txt file",
            "b.txt unreadable",
            "closed unreadable",
            "data symlink",
            "dir folder",
            "dir/in.txt file",
            "link symlink",
            "nox unreadable",
            "pipe other",
            "proc symlink",
            "state symlink",
            "store.sqlite restricted"
        ]
    );
    assert_eq!(entry(&listed, &open.join("b.txt")).get_int("size"), Some(5));
    assert_eq!(
        entry(&listed, &open.join("link")).get_text("target"),
        Some("dir")
    );
    let text = |path: &Path| {
        let file = at(data, path, &[]).unwrap();
        file.get_text("content").map(str::to_owned)
    };
    assert_eq!(text(&open.join("a.txt")).as_deref(), Some("text\n"));
    // a path leads through a link, which resolves inside the machine's root
    assert_eq!(text(&open.join("link/in.txt")).as_deref(), Some("text\n"));
    invalid(at(data, &open.join("link"), &[]), "symbolic link");
    invalid(at(data, &open.join("b.txt"), &[]), "is not world-readable");
    invalid(at(data, &open.join("closed"), &[]), "is not world-readable");
    invalid(
        at(data, &open.join("closed/in.txt"), &[]),
        "is in a folder that is not world-readable",
    );
    invalid(at(data, &open.join("nox"), &[]), "is not world-readable");
    invalid(at(data, &open.join("pipe"), &[]), "a FIFO");
    invalid(
        at(data, &open.join("store.sqlite"), &[]),
        "kept private by LayerFS",
    );
    // a link into a mount the view does not enter ends nowhere: what was
    // opened is told by its mount id
    invalid(
        at(data, &open.join("proc/version"), &[]),
        "is in a mount this view does not enter",
    );
    // nor does a link into a LayerFS folder give what that folder keeps
    invalid(
        at(data, &open.join("state/secret"), &[]),
        "is in a folder that is not world-readable",
    );
    invalid(
        at(data, &open.join("data/store/shop/store.sqlite"), &[]),
        "kept private by LayerFS",
    );
    invalid(
        at(data, &open.join("data/sandbox/s1/control.json"), &[]),
        "is not world-readable",
    );

    // the LayerFS folders are listed with their roles and read apart: a
    // walk from `/` enters none, and each answers by its own path
    let folders = at(data, &fx.layerfs, &[("depth", SdkValue::Int(3))]).unwrap();
    assert_eq!(
        shown(&folders, &fx.layerfs),
        [
            "bundle folder bundle",
            "data folder data",
            "state folder state"
        ]
    );
    let state = at(data, &fx.layerfs.join("state"), &[]).unwrap();
    assert_eq!(
        shown(&state, &fx.layerfs),
        ["state/secret restricted", "state/worker.pid file"]
    );

    // a mount of a type that can wait on another host or process, where
    // this host has one at the top of `/`: listed with its type, never
    // entered, by its path or through a link
    let crossed = ["ext4", "xfs", "btrfs", "zfs", "f2fs", "overlay", "tmpfs"];
    let table = layerfs_sandbox_backend::local::confined::mounts().unwrap();
    let mut tops = std::collections::BTreeMap::new();
    for mount in &table {
        tops.insert(mount.point.clone(), mount.fs.clone());
    }
    let found = tops.iter().find(|(point, fs)| {
        let system = ["/proc", "/sys", "/dev", "/run"]
            .iter()
            .any(|s| point.starts_with(s));
        point.parent() == Some(Path::new("/"))
            && !system
            && !crossed.contains(&fs.as_str())
            && std::fs::metadata(point).is_ok_and(|found| found.is_dir())
    });
    match found {
        Some((point, fs)) => {
            let top = at(data, Path::new("/"), &[("limit", SdkValue::Int(1000))]).unwrap();
            let shown = entry(&top, point);
            println!("machine.files mount: {}", json(shown));
            assert_eq!(shown.get_text("kind"), Some("mount"));
            assert_eq!(shown.get_text("fs"), Some(fs.as_str()));
            assert_eq!(shown.get("mode"), Some(&SdkValue::Null));
            invalid(at(data, point, &[]), "is a mount point");
            invalid(at(data, &point.join("x"), &[]), "is in a mount");
            symlink(point, open.join("m")).unwrap();
            invalid(
                at(data, &open.join("m/x"), &[]),
                "is in a mount this view does not enter",
            );
            println!("a {fs} mount at {}: listed, never entered", point.display());
        }
        None => println!(
            "NOT_RUN a mount that is not entered: no mount of another type than {crossed:?} is at the top of `/` here"
        ),
    }
}
