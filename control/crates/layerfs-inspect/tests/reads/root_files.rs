//! `sandbox.files`, Root view (layerfs-project#178 step 5): what a
//! namespaces Sandbox's commands see as `/`, read through the root of the
//! Sandbox's init process on the side that runs it. The roots answer
//! offers `/` for such a Sandbox only; a stopped one keeps its home and
//! answers `Unavailable` for its root; a host-process Sandbox has no such
//! view.
//!
//! No Sandbox runs here. On Linux a child process of the test stands in
//! for a Sandbox's init: its root is this host's `/` and its mount table
//! this host's, which is every part of the read but a mount namespace of
//! its own (NOT_RUN here: making one needs a privilege the test host does
//! not give). The run folder is under `/tmp`, where a read from `/`
//! reaches it.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_os::process::LocalProcess;
use layerfs_home::home::{paths, write_record};
use {layerfs_inspect, layerfs_sandbox::deploy::Side};
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);
/// The uid the fixture's Sandbox runs its commands as; not this test's.
const COMMANDS: u32 = 501;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn mode(path: &Path, bits: u32) {
    std::fs::set_permissions(path, PermissionsExt::from_mode(bits)).unwrap();
}

/// A worker's root with Sandbox `s1`, its run folder as a start leaves it,
/// and a Store's folder beside.
struct Fixture {
    scratch: PathBuf,
    data: PathBuf,
    run: PathBuf,
    /// The process that stands in for the Sandbox's init.
    init: Option<Child>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

impl Fixture {
    /// Ends the stand-in init and reaps it: the Sandbox has stopped.
    fn stop(&mut self) {
        if let Some(mut init) = self.init.take() {
            let _ = init.kill();
            let _ = init.wait();
        }
    }
}

/// The identity a daemon record keeps of a process.
type Recorded = (u32, u64, String);

/// A process that is no process: an id past any this kernel gives.
fn nobody() -> Recorded {
    (4_000_000, 1, format!("{:032x}", 7))
}

fn fixture(tag: &str, runtime: &'static str, live: bool) -> Fixture {
    let scratch = std::env::temp_dir().join(format!(
        "rfiles-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let data = scratch.join("layerfs/data");
    // a worker's root: its run root is the guest's (#819 R)
    layerfs_home::home::guest::serve_root(&data);
    let run = paths::run_root(&data).join("layerfs-0a1b2c3d-s1");
    assert!(run.starts_with(data.join("machine/linux-guest/run")));
    for folder in ["tmp", "workspaces/1", "config", "overlay"] {
        std::fs::create_dir_all(run.join(folder)).unwrap();
    }
    std::fs::create_dir_all(data.join("store/shop")).unwrap();
    // every folder on the way is one any user may pass, as in a machine
    let mut above = run.clone();
    while above != scratch {
        mode(&above, 0o755);
        above.pop();
    }
    mode(&scratch, 0o755);
    std::fs::write(run.join("daemon.log"), "listening\n").unwrap();
    std::fs::write(run.join("config/daemon.setup"), "private key").unwrap();
    std::fs::write(run.join("overlay/1.sqlite"), "overlay").unwrap();
    std::fs::write(run.join("tmp/notes.txt"), "by a command\n").unwrap();
    std::fs::write(data.join("store/shop/store.sqlite"), "db").unwrap();
    std::fs::write(data.join("store/shop/record.json"), "{}").unwrap();
    let init = live.then(|| {
        Command::new("sleep")
            .arg("100")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    });
    let process = match &init {
        Some(child) => {
            let seen = LocalProcess::observe(child.id()).unwrap();
            (seen.pid, seen.start, format!("{:032x}", seen.boot.unwrap()))
        }
        None => nobody(),
    };
    let fx = Fixture {
        scratch,
        data,
        run,
        init,
    };
    record(&fx, runtime, process);
    let id = format!("{:064x}", 1);
    let mounted = fx.run.join("workspaces/1");
    WorkspaceRecord::mounted(id.clone(), None, "main", mounted.to_str().unwrap(), 0)
        .write(&paths::workspace_dir(&fx.data, "s1").join(format!("{id}.json")))
        .unwrap();
    fx
}

/// Sandbox `s1` as the side that runs it records it, its init `process`.
fn record(fx: &Fixture, runtime: &'static str, process: Recorded) {
    let daemon = DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: "layerfs-0a1b2c3d-s1".to_owned(),
        container: "ab".repeat(32),
        runtime,
        uid: COMMANDS,
        gid: 20,
        directory: fx.run.display().to_string(),
        process,
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(&fx.data, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("box")),
            ("id", t("s1")),
            ("runtime", t(runtime)),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

fn read(fx: &Fixture, fields: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let mut all = vec![("sandbox".to_owned(), t("s1"))];
    all.extend(fields.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    layerfs_inspect::sandbox_page(&Side::Worker(&fx.data), &SdkValue::Map(all), &|| false)
}

fn at(fx: &Fixture, path: &Path, more: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![("path", t(path.to_str().unwrap()))];
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

fn refused(outcome: Result<SdkValue, SdkRefusal>, code: &str, says: &str) {
    match outcome {
        Ok(value) => panic!("answered {value:?}"),
        Err(refusal) => {
            assert_eq!(refusal.code, code, "{}", refusal.message);
            assert!(
                refusal.message.contains(says),
                "{} lacks {says:?}",
                refusal.message
            );
        }
    }
}

fn json(value: &SdkValue) -> String {
    layerfs_home::json::print(value)
}

#[test]
fn the_roots_offer_the_root_of_a_namespaces_sandbox_only() {
    // the Sandbox has stopped: its roots are its records and its run folder
    let fx = fixture("roots", "namespaces", false);
    let roots = read(&fx, &[]).unwrap();
    println!("sandbox.files roots: {}", json(&roots));
    assert_eq!(
        shown(&roots, &fx.run),
        [
            "workspaces/1 mount workspace",
            "tmp folder home",
            "daemon.log file log",
            "/ folder root"
        ]
    );
    // the root is offered from the record alone: nothing of it was opened
    assert_eq!(
        json(entries(&roots).last().unwrap()),
        r#"{"path":"/","depth":1,"kind":"folder","size":null,"mode":null,"mtime":null,"role":"root"}"#
    );

    // a host-process Sandbox has no root of its own: no `/`, and a path
    // outside its home is refused as it always was
    let native = fixture(layerfs_home::names::HOST_MACHINE, "host", false);
    let roots = read(&native, &[]).unwrap();
    assert_eq!(
        shown(&roots, &native.run),
        [
            "workspaces/1 mount workspace",
            "tmp folder home",
            "daemon.log file log"
        ]
    );
    for path in ["/", "/etc/passwd"] {
        refused(
            at(&native, Path::new(path), &[]),
            "InvalidInput",
            "only paths under the home are read",
        );
    }
    refused(
        at(&native, &native.run.join("config"), &[]),
        "InvalidInput",
        "LayerFS keeps it private",
    );
}

#[test]
fn a_stopped_sandbox_reads_its_home_and_answers_unavailable_for_its_root() {
    let fx = fixture("stopped", "namespaces", false);
    let home = fx.run.join("tmp");
    let listed = at(&fx, &home, &[]).unwrap();
    assert_eq!(shown(&listed, &home), ["notes.txt file"]);
    let notes = at(&fx, &home.join("notes.txt"), &[]).unwrap();
    assert_eq!(notes.get_text("content"), Some("by a command\n"));
    for path in [Path::new("/"), Path::new("/etc/passwd"), &fx.run] {
        let refusal = at(&fx, path, &[]).unwrap_err();
        println!(
            "sandbox.files stopped: {}: {}",
            refusal.code, refusal.message
        );
        assert_eq!(refusal.code, "Unavailable", "{}", refusal.message);
        assert!(refusal.message.contains("its daemon is not running"));
        assert!(refusal.message.contains(home.to_str().unwrap()));
    }
    // options and the path's form are checked first, as in every view
    refused(
        at(&fx, Path::new("/etc/../etc"), &[]),
        "InvalidInput",
        "absolute path with no `..`",
    );
}

/// macOS runs no namespaces Sandbox: a record of one that names a live
/// process answers that its root cannot be read here, and why.
#[cfg(target_os = "macos")]
#[test]
fn macos_has_no_root_view() {
    let fx = fixture("macos", "namespaces", true);
    refused(
        at(&fx, Path::new("/etc"), &[]),
        "Unsupported",
        "no Sandbox has a root of its own on macOS",
    );
    let home = fx.run.join("tmp");
    assert_eq!(
        shown(&at(&fx, &home, &[]).unwrap(), &home),
        ["notes.txt file"]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_sandbox_s_root_is_read_through_its_init_with_what_layerfs_keeps_restricted() {
    let mut fx = fixture("root", "namespaces", true);
    if layerfs_home::home::flock::euid() != 0 {
        // a reader that is not root, as on a native Linux home
        refused(at(&fx, Path::new("/etc"), &[]), "Unsupported", "needs root");
        println!("not root: the Root view is Unsupported; its reads are NOT_RUN here");
        return;
    }
    println!("NOT_RUN a reader that is not root: this test runs as root");
    let slash = Path::new("/");
    let top = at(&fx, slash, &[("limit", SdkValue::Int(1000))]).unwrap();
    println!("sandbox.files / : {}", json(&top));
    let kind_of = |answer: &SdkValue, path: &Path| {
        let found = (entries(answer).iter()).find(|entry| entry.get_text("path") == path.to_str());
        found.and_then(|entry| entry.get_text("kind").map(str::to_owned))
    };
    for system in ["/proc", "/sys", "/dev"] {
        assert_eq!(kind_of(&top, Path::new(system)).as_deref(), Some("system"));
    }
    // `/run` and `/tmp` are a Sandbox's own folders
    assert_eq!(kind_of(&top, Path::new("/tmp")).as_deref(), Some("folder"));
    refused(
        at(&fx, Path::new("/proc/1/environ"), &[]),
        "InvalidInput",
        "is in a system folder",
    );
    refused(
        at(&fx, Path::new("/dev"), &[]),
        "InvalidInput",
        "is a system folder",
    );

    // no world-readable rule: the Sandbox's files are its own
    let private = fx.scratch.join("private");
    std::fs::write(&private, "its own\n").unwrap();
    mode(&private, 0o600);
    let file = at(&fx, &private, &[]).unwrap();
    assert_eq!(file.get_text("content"), Some("its own\n"));
    // a path leads through a link of a merged-`/usr` image
    let ls = Path::new("/bin/ls");
    let linked = std::fs::symlink_metadata("/bin").is_ok_and(|found| found.is_symlink());
    match std::fs::symlink_metadata(ls) {
        Ok(found) if found.is_file() => {
            let file = at(&fx, ls, &[("length", SdkValue::Int(64))]).unwrap();
            assert!(file.get_bool("binary"));
            assert_eq!(file.get_int("size"), i64::try_from(found.len()).ok());
            println!("/bin/ls: binary, read through a link: {linked}");
        }
        _ => println!("NOT_RUN /bin/ls: no such regular file here"),
    }
    // an image's file with several names is read, whoever owns it: here
    // one of this test's (root's), outside the run folder
    let applet = fx.scratch.join("busybox");
    std::fs::write(&applet, "applet\n").unwrap();
    std::fs::hard_link(&applet, fx.scratch.join("sh")).unwrap();
    mode(&applet, 0o755);
    let read_applet = at(&fx, &fx.scratch.join("sh"), &[]).unwrap();
    assert_eq!(read_applet.get_text("content"), Some("applet\n"));

    // the run folder is in the root at its own path, with what LayerFS
    // keeps restricted there and its Workspace mount from its record
    let run = at(&fx, &fx.run, &[("depth", SdkValue::Int(2))]).unwrap();
    println!("sandbox.files run folder: {}", json(&run));
    assert_eq!(
        shown(&run, &fx.run),
        [
            "config restricted",
            "daemon.log file log",
            "overlay restricted",
            "tmp folder",
            "tmp/notes.txt file",
            "workspaces folder",
            "workspaces/1 mount"
        ]
    );
    let mount = (entries(&run).iter()).find(|entry| entry.get_text("kind") == Some("mount"));
    // a mount entry names the Workspace by its 2.0 handle: a nameless
    // Workspace is `<sandbox>:<path>`
    let handle = format!("s1:{}", fx.run.join("workspaces/1").display());
    assert_eq!(mount.unwrap().get_text("workspace"), Some(handle.as_str()));
    assert_eq!(mount.unwrap().get("mode"), Some(&SdkValue::Null));
    let kept = "is kept private by LayerFS";
    let within = "is in a folder that is listed, not read";
    refused(at(&fx, &fx.run.join("config"), &[]), "InvalidInput", kept);
    refused(
        at(&fx, &fx.run.join("config/daemon.setup"), &[]),
        "InvalidInput",
        within,
    );
    refused(
        at(&fx, &fx.run.join("overlay/1.sqlite"), &[]),
        "InvalidInput",
        within,
    );
    refused(
        at(&fx, &fx.run.join("daemon.log"), &[]),
        "InvalidInput",
        "is a log",
    );
    refused(
        at(&fx, &fx.run.join("workspaces/1"), &[]),
        "InvalidInput",
        "is a mount point",
    );
    // a bound Store's folder is there too: its database is restricted
    let store = fx.data.join("store/shop");
    let listed = at(&fx, &store, &[]).unwrap();
    assert_eq!(
        shown(&listed, &store),
        ["record.json file", "store.sqlite restricted"]
    );
    let refusal = at(&fx, &store.join("store.sqlite"), &[]).unwrap_err();
    println!(
        "sandbox.files refusal: {}: {}",
        refusal.code, refusal.message
    );
    assert_eq!(refusal.code, "InvalidInput");
    assert!(refusal.message.contains(kept), "{}", refusal.message);

    // a link a command made does not rename what LayerFS keeps: where the
    // open ended is judged too
    let links = fx.scratch.join("links");
    std::fs::create_dir(&links).unwrap();
    mode(&links, 0o755);
    use std::os::unix::fs::symlink;
    symlink(&fx.run, links.join("run")).unwrap();
    symlink(fx.run.join("config"), links.join("keys")).unwrap();
    let renamed = "leads to what LayerFS keeps private under another name";
    refused(
        at(&fx, &links.join("run/config/daemon.setup"), &[]),
        "InvalidInput",
        renamed,
    );
    refused(
        at(&fx, &links.join("keys/daemon.setup"), &[]),
        "InvalidInput",
        renamed,
    );
    refused(
        at(&fx, &links.join("run/config"), &[]),
        "InvalidInput",
        renamed,
    );
    refused(
        at(&fx, &links.join("run/daemon.log"), &[]),
        "InvalidInput",
        renamed,
    );
    // the link itself is the last name of its path, and is not followed
    refused(
        at(&fx, &links.join("run"), &[]),
        "InvalidInput",
        "symbolic link",
    );
    // a walk of the run folder through a link above it lists the folder
    // and enters nothing kept
    symlink(fx.run.parent().unwrap(), links.join("runs")).unwrap();
    let alias = links.join("runs/layerfs-0a1b2c3d-s1");
    let through = at(&fx, &alias, &[("depth", SdkValue::Int(2))]).unwrap();
    let listed = shown(&through, &alias);
    assert!(
        listed.contains(&"tmp/notes.txt file".to_owned()),
        "{listed:?}"
    );
    assert!(
        !listed.iter().any(|entry| entry.starts_with("config/")),
        "{listed:?}"
    );
    // what is not kept is read through the link as through its own path
    let notes = at(&fx, &links.join("run/tmp/notes.txt"), &[]).unwrap();
    assert_eq!(notes.get_text("content"), Some("by a command\n"));

    // in the run folder a file with several names that the command user
    // does not own is not read, by its own path or through a link
    std::fs::write(fx.run.join("linked"), "not its own\n").unwrap();
    std::fs::hard_link(fx.run.join("linked"), fx.run.join("tmp/again")).unwrap();
    let foreign = "is a hard link to a file the Sandbox's commands do not own";
    refused(
        at(&fx, &fx.run.join("linked"), &[]),
        "InvalidInput",
        foreign,
    );
    refused(
        at(&fx, &fx.run.join("tmp/again"), &[]),
        "InvalidInput",
        foreign,
    );
    refused(
        at(&fx, &links.join("run/linked"), &[]),
        "InvalidInput",
        foreign,
    );

    // a mount that is not entered, where this host has one at the top of `/`
    let crossed = ["ext4", "xfs", "btrfs", "zfs", "f2fs", "overlay", "tmpfs"];
    let table = layerfs_sandbox::backend::confined::mounts().unwrap();
    let mut tops = std::collections::BTreeMap::new();
    for mount in &table {
        tops.insert(mount.point.clone(), mount.fs.clone());
    }
    let found = tops.iter().find(|(point, fs)| {
        let system = ["/proc", "/sys", "/dev"]
            .iter()
            .any(|s| point.starts_with(s));
        point.parent() == Some(slash)
            && !system
            && !crossed.contains(&fs.as_str())
            && std::fs::metadata(point).is_ok_and(|found| found.is_dir())
    });
    match found {
        Some((point, fs)) => {
            assert_eq!(kind_of(&top, point).as_deref(), Some("mount"));
            refused(at(&fx, point, &[]), "InvalidInput", "is a mount point");
            refused(
                at(&fx, &point.join("x"), &[]),
                "InvalidInput",
                "is in a mount",
            );
            symlink(point, links.join("m")).unwrap();
            refused(
                at(&fx, &links.join("m/x"), &[]),
                "InvalidInput",
                "is in a mount this view does not enter",
            );
            println!("a {fs} mount at {}: listed, never entered", point.display());
        }
        None => {
            println!("NOT_RUN a mount that is not entered: none of another type at the top of `/`")
        }
    }

    // a record whose process id now names another process: its start
    // time is not the recorded one
    let child = fx.init.as_ref().unwrap().id();
    let seen = LocalProcess::observe(child).unwrap();
    let boot = format!("{:032x}", seen.boot.unwrap());
    record(&fx, "namespaces", (seen.pid, seen.start + 1, boot.clone()));
    refused(at(&fx, slash, &[]), "Unavailable", "root is not there");
    record(&fx, "namespaces", (seen.pid, seen.start, boot));
    assert!(at(&fx, slash, &[]).is_ok());

    // the Sandbox stops: its root is gone and its home is still read
    fx.stop();
    refused(
        at(&fx, slash, &[]),
        "Unavailable",
        "its daemon is not running",
    );
    refused(
        at(&fx, &fx.run, &[]),
        "Unavailable",
        "its daemon is not running",
    );
    let home = fx.run.join("tmp");
    let listed = at(&fx, &home, &[]).unwrap();
    assert_eq!(shown(&listed, &home), ["again file", "notes.txt file"]);
    let roots = read(&fx, &[]).unwrap();
    assert_eq!(
        entries(&roots).last().unwrap().get_text("role"),
        Some("root")
    );
}
