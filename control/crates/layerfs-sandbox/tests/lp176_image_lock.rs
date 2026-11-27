//! One copy of an image per machine at a time, and the capacity check that
//! runs before the copy touches anything (layerfs-project#175 R01, R03):
//! a second copy of the same image on the same machine waits on the home's
//! lock and then reuses what the first published; another machine's copy of
//! the same image is another lock; a copy whose owner died is taken over at
//! once and a record naming a reused pid holds nothing; a machine short of
//! room is refused `Capacity` before a container, a staged byte or a cache
//! folder exists, and a resumed copy counts the bytes it already staged once.
//!
//! The Docker here is a stand-in shell script (no Engine, no root); one test
//! holds its `docker export` until the test lets it go, so a second copy is
//! really waiting while the first is really sending.
#[allow(dead_code)]
#[path = "support/image.rs"]
mod fixture;
use fixture::{Scratch, Tar};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_sandbox::docker::{self, Docker};
use layerfs_sandbox::image_artifact::{self as cache, architecture};
use layerfs_sandbox::image_load::{load, ImageLock, Loaded, Target};
use layerfs_sandbox::image_lock::PruneGate;
use layerfs_sandbox::image_tar::Filtered;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ID: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";
const HEX: &str = "4444444444444444444444444444444444444444444444444444444444444444";

/// A stand-in `docker` CLI over files in its own directory: the answers
/// `image.artifact`'s copy asks for, and an `export` that waits for `go`
/// while `hold` exists, so a test decides when a copy finishes.
const SCRIPT: &str = r#"#!/bin/sh
D="$(dirname "$0")"
if [ "$1" = "-H" ]; then shift 2; fi
key() { printf '%s' "$1" | tr '/:' '__'; }
case "$1 $2" in
  "image inspect")
    shift 4; missing=0
    for ref in "$@"; do
      f="$D/images/$(key "$ref").json"
      if [ -f "$f" ]; then cat "$f"; echo; else echo "Error: No such image: $ref" >&2; missing=1; fi
    done
    exit $missing ;;
  "image ls") cat "$D/ls.txt" 2>/dev/null; exit 0 ;;
  "container inspect")
    if [ "$3" = "--size" ]; then cat "$D/rootfs_size"; exit 0; fi
    if [ -f "$D/containers/$5" ]; then echo id; exit 0; fi
    echo "Error: No such object: $5" >&2; exit 1 ;;
esac
case "$1" in
  create)
    mkdir -p "$D/containers"
    echo x >> "$D/creates"
    while [ $# -gt 0 ]; do
      case "$1" in
        --name) name="$2"; shift 2 ;;
        --label|--network|--entrypoint) shift 2 ;;
        *) shift ;;
      esac
    done
    : > "$D/containers/$name"
    echo id ;;
  export)
    echo x >> "$D/starts"
    if [ -f "$D/hold" ]; then
      while [ ! -f "$D/go" ]; do sleep 0.02; done
    fi
    echo x >> "$D/exports"
    cat "$D/export.tar" ;;
  history)
    f="$D/history/$(key "$5")"
    if [ -f "$f" ]; then cat "$f"; exit 0; fi
    echo "Error: No such image" >&2; exit 1 ;;
  rm)
    shift
    if [ "$1" = "-f" ]; then shift; fi
    rm -f "$D/containers/$1" ;;
  ps) ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

/// The stand-in Docker's directory.
struct Standin {
    dir: PathBuf,
}

impl Standin {
    fn new(dir: &Path) -> Self {
        fs::create_dir_all(dir.join("images")).unwrap();
        let program = dir.join("docker");
        fs::write(&program, SCRIPT).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(dir.join("rootfs_size"), "0\n").unwrap();
        Self {
            dir: dir.to_owned(),
        }
    }

    fn docker(&self) -> Docker {
        Docker::at(Some(self.dir.join("docker")), None)
    }

    /// The image `id` Docker holds as `refs`, of `size` bytes.
    fn image(&self, id: &str, refs: &[&str], architecture: &str, size: u64) {
        let tags = refs
            .iter()
            .map(|r| format!("\"{r}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"Id":"{id}","RepoTags":[{tags}],"Os":"linux","Architecture":"{architecture}","Size":{size},"Config":{{"Env":["PATH=/usr/bin"]}}}}"#
        );
        for reference in refs.iter().copied().chain([id]) {
            let key = reference.replace(['/', ':'], "_");
            fs::write(self.dir.join("images").join(format!("{key}.json")), &json).unwrap();
        }
        let mut ls = fs::read_to_string(self.dir.join("ls.txt")).unwrap_or_default();
        for reference in refs {
            ls.push_str(reference);
            ls.push('\n');
        }
        fs::write(self.dir.join("ls.txt"), ls).unwrap();
    }

    /// `docker export` answers these bytes.
    fn export(&self, bytes: &[u8]) {
        fs::write(self.dir.join("export.tar"), bytes).unwrap();
    }

    /// `docker export` waits for [`Standin::release`] before answering.
    fn hold(&self) {
        fs::write(self.dir.join("hold"), b"").unwrap();
    }

    fn release(&self) {
        fs::write(self.dir.join("go"), b"").unwrap();
    }

    fn count(&self, name: &str) -> usize {
        fs::read_to_string(self.dir.join(name))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    /// How many `docker create` calls ran.
    fn creates(&self) -> usize {
        self.count("creates")
    }

    /// How many `docker export` calls started.
    fn starts(&self) -> usize {
        self.count("starts")
    }

    /// How many `docker export` calls answered every byte.
    fn exports(&self) -> usize {
        self.count("exports")
    }

    /// The export containers Docker holds here now.
    fn containers(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir.join("containers"))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

/// One tar of a small root filesystem, as `docker export` writes it.
fn export() -> Vec<u8> {
    Tar::new()
        .directory("etc/", 0o755)
        .file("etc/app", 0o644, b"hello\n")
        .file("usr/bin/app", 0o755, &vec![7u8; 2048])
        .finish()
}

/// A tar of 4 MiB, so a test can stage megabytes of it: the staged bytes of a
/// resumed copy have to cover a gap far above the few kilobytes another test
/// writes to the same filesystem while this one runs.
fn big_export() -> Vec<u8> {
    Tar::new()
        .directory("data/", 0o755)
        .file("data/blob", 0o644, &vec![3u8; 4 << 20])
        .finish()
}

/// The same bytes after [`Filtered`] dropped the init layer's files.
fn filtered(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    Filtered::new(bytes).read_to_end(&mut out).unwrap();
    out
}

/// A copy into `machine`'s cache, in this process, recording its events.
fn copy(
    docker: &Docker,
    at_home: &Path,
    machine: &str,
    image: &str,
    events: &Arc<Mutex<Vec<SdkValue>>>,
) -> Result<Loaded, SdkRefusal> {
    let seen = events.clone();
    let mut sink = move |event: SdkValue| seen.lock().unwrap().push(event);
    load(
        docker,
        &mut Events::new(&mut sink),
        &mut Target::local(at_home, machine),
        None,
        image,
    )
}

/// The lock's events of phase `phase` (its `progress` values).
fn phases(events: &Arc<Mutex<Vec<SdkValue>>>, phase: &str) -> Vec<SdkValue> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| event.get("progress"))
        .filter(|progress| progress.get_text("phase") == Some(phase))
        .cloned()
        .collect()
}

/// Waits, bounded, until `done`; a test that waits longer than 20 s hangs.
fn wait_for(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn setup(name: &str) -> (Scratch, Standin, PathBuf) {
    let scratch = Scratch::new(name);
    let standin = Standin::new(&scratch.base.join("standin"));
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    // the home is opened, as every call opens it before a copy: a folder
    // holding a lock record and no `layerfs.json` is not a 2.0 home
    home::open(&home).unwrap();
    standin.image(ID, &["demo/app:1"], architecture(), 4000);
    standin.export(&filtered(&export()));
    (scratch, standin, home)
}

/// A folder with no `layerfs.json`: by the time a copy needs the home's
/// id the image lock's record is in it, so it is no empty home and the
/// copy is refused as foreign, creating no home and nothing in Docker.
#[test]
fn a_folder_that_is_no_home_is_refused_before_an_export() {
    let scratch = Scratch::new("lp176-no-home");
    let standin = Standin::new(&scratch.base.join("standin"));
    let home = scratch.base.join("home");
    fs::create_dir_all(&home).unwrap();
    standin.image(ID, &["demo/app:1"], architecture(), 4000);
    standin.export(&filtered(&export()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let refusal = copy(&standin.docker(), &home, "docker", "demo/app:1", &events).unwrap_err();
    assert_eq!(refusal.code, "Unsupported", "{refusal:?}");
    assert!(
        refusal.message.contains("not a LayerFS 2.0 home"),
        "{}",
        refusal.message
    );
    assert_eq!(standin.creates(), 0, "an export container was made");
    assert!(!home.join("layerfs.json").exists(), "a home was created");
}

#[test]
fn one_copy_of_an_image_per_machine() {
    let (_scratch, standin, home) = setup("lp176-one-writer");
    standin.hold();
    // the first copy: it holds the lock and waits in `docker export`
    let first_events = Arc::new(Mutex::new(Vec::new()));
    let first = {
        let (docker, at_home, events) = (standin.docker(), home.clone(), first_events.clone());
        std::thread::spawn(move || copy(&docker, &at_home, "docker", "demo/app:1", &events))
    };
    wait_for("the first copy to start its export", || {
        standin.starts() == 1
    });
    // the second copy: it says it waits, and it sends nothing meanwhile
    let second_events = Arc::new(Mutex::new(Vec::new()));
    let second = {
        let (docker, at_home, events) = (standin.docker(), home.clone(), second_events.clone());
        std::thread::spawn(move || copy(&docker, &at_home, "docker", "demo/app:1", &events))
    };
    wait_for("the second copy to say it waits", || {
        !phases(&second_events, "lock").is_empty()
    });
    // v19: the wait is the plain `{phase: lock}` event; who it waits for is
    // the owner record, which `machine images` shows as the busy item
    let waited = &phases(&second_events, "lock")[0];
    assert_eq!(waited.get("waiting"), None, "the event names no holder");
    assert_eq!(waited.get("holder"), None, "the event names no holder");
    let holder = ImageLock::holder(&home, "docker", HEX).expect("the holder's record");
    assert!(
        matches!(holder.get_text("phase"), Some("export" | "send")),
        "the holder records where it is: {holder:?}"
    );
    assert_eq!(
        holder.get_int("pid"),
        Some(std::process::id() as i64),
        "the holder is the copy in this process: {holder:?}"
    );
    assert_eq!(holder.get_text("image"), Some(ID));
    assert_eq!(holder.get_text("machine"), Some("docker"));
    assert_eq!(standin.starts(), 1, "one writer at a time");
    assert_eq!(standin.creates(), 1, "one export container at a time");
    assert!(phases(&second_events, "export").is_empty(), "no export yet");
    assert_eq!(standin.exports(), 0, "no byte left the first copy yet");
    assert!(
        !cache::cache(&home).join(format!("{HEX}.2")).exists(),
        "nothing is unpacked while the first copy sends"
    );
    // the first copy finishes: the second reuses what it published
    standin.release();
    let first = first.join().unwrap().unwrap();
    assert!(!first.reused);
    let second = second.join().unwrap().unwrap();
    assert!(second.reused, "the waiter reuses the published image");
    assert_eq!(second.size, first.size);
    assert_eq!(standin.starts(), 1, "one export in all");
    assert!(cache::cache(&home).join(format!("{HEX}.2")).is_dir());
    assert!(!cache::cache(&home)
        .join(format!(".recv-{HEX}.part"))
        .exists());
    assert!(
        !ImageLock::record_path(&home, "docker", HEX).exists(),
        "a finished copy leaves no record"
    );
    assert!(standin.containers().is_empty(), "its container is gone");
}

/// `machine.prune` holds the machine's gate while it reads the copy locks and
/// removes: a copy that takes its lock meanwhile holds it (so the prune sees
/// it) and creates and writes nothing until the prune is over.
#[test]
fn a_copy_waits_for_a_prune_under_way() {
    let (_scratch, standin, home) = setup("lp176-prune-gate");
    let gate = PruneGate::hold(&home, "docker").unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let copying = {
        let (docker, at_home, events) = (standin.docker(), home.clone(), events.clone());
        std::thread::spawn(move || copy(&docker, &at_home, "docker", "demo/app:1", &events))
    };
    wait_for("the copy to take its lock", || {
        ImageLock::try_hold(&home, "docker", ID).unwrap().is_none()
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(!copying.is_finished(), "the copy waits for the prune");
    assert_eq!(standin.creates(), 0, "no export container meanwhile");
    assert!(!cache::cache(&home)
        .join(format!(".recv-{HEX}.part"))
        .exists());
    drop(gate);
    wait_for("the copy to finish", || copying.is_finished());
    let loaded = copying.join().unwrap().unwrap();
    assert!(!loaded.reused);
    assert_eq!(standin.starts(), 1);
}

#[test]
fn another_machine_is_another_lock() {
    let (_scratch, standin, home) = setup("lp176-two-machines");
    let mut sink = |_: SdkValue| {};
    let held = ImageLock::take(&home, "docker", ID, &mut Events::new(&mut sink)).unwrap();
    // the same image on another machine is another lock, so its copy runs
    let events = Arc::new(Mutex::new(Vec::new()));
    let loaded = copy(&standin.docker(), &home, "box", "demo/app:1", &events).unwrap();
    assert!(!loaded.reused);
    assert!(
        phases(&events, "lock").is_empty(),
        "the other machine's copy does not wait"
    );
    assert_eq!(standin.exports(), 1);
    // the record names the machine that holds the lock, not the other one
    let record = home::read_record(&ImageLock::record_path(&home, "docker", HEX))
        .unwrap()
        .expect("the holder's record");
    assert_eq!(record.get_text("machine"), Some("docker"));
    assert_eq!(record.get_text("image"), Some(ID));
    assert_eq!(record.get_int("pid"), Some(std::process::id() as i64));
    assert!(
        record.get("pid_start").is_some(),
        "the record names its process"
    );
    drop(held);
    assert!(!ImageLock::record_path(&home, "docker", HEX).exists());
    // and the lock is free now
    let mut sink = |_: SdkValue| {};
    ImageLock::take(&home, "docker", ID, &mut Events::new(&mut sink)).unwrap();
}

#[test]
fn a_dead_owner_is_taken_over_and_a_reused_pid_holds_nothing() {
    let (_scratch, _standin, home) = setup("lp176-dead-owner");
    let record = ImageLock::record_path(&home, "docker", HEX);
    // a record left by a copy whose pid another process now holds: the pid
    // is alive, the process is not that copy, and no lock is held
    fs::create_dir_all(record.parent().unwrap()).unwrap();
    home::write_record(
        &record,
        &SdkValue::map([
            ("pid", SdkValue::Int(std::process::id() as i64)),
            ("pid_start", SdkValue::text("1")),
            ("pid_boot", SdkValue::text("00")),
            ("machine", SdkValue::text("docker")),
            ("image", SdkValue::text(ID)),
        ]),
    )
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();
    let reused = {
        let seen = events.clone();
        let mut sink = move |event: SdkValue| seen.lock().unwrap().push(event);
        ImageLock::take(&home, "docker", ID, &mut Events::new(&mut sink))
    };
    let reused = reused.unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "a record alone is not a lock"
    );
    assert!(phases(&events, "lock").is_empty(), "no wait on a dead name");
    let mine = home::read_record(&record).unwrap().unwrap();
    assert_eq!(mine.get_int("pid"), Some(std::process::id() as i64));
    assert_ne!(
        mine.get_text("pid_start"),
        Some("1"),
        "this copy replaced it"
    );
    drop(reused);
    assert!(!record.exists());
    // a copy in another process that is killed: the kernel releases its
    // lock, and the waiting copy takes over without a deadline
    let mut helper = Holder(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "holds_the_lock_for_another_copy", "--nocapture"])
            .env("LP176_LOCK_HOLD", &home)
            .env("LP176_LOCK_PARENT", std::process::id().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for("the other process to hold the lock", || {
        home.join("helper-held").exists()
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();
    let waiter = {
        let (at_home, seen) = (home.clone(), events.clone());
        std::thread::spawn(move || {
            let sink = seen.clone();
            let mut sink = move |event: SdkValue| sink.lock().unwrap().push(event);
            ImageLock::take(&at_home, "docker", ID, &mut Events::new(&mut sink))
        })
    };
    wait_for("the second copy to say it waits", || {
        !phases(&events, "lock").is_empty()
    });
    assert!(!waiter.is_finished(), "a live owner is waited for");
    assert_eq!(
        ImageLock::holder(&home, "docker", HEX).and_then(|record| record.get_int("pid")),
        Some(helper.0.id() as i64),
        "the owner record names the other process"
    );
    // killed: no drop runs in it, so its record stays and its lock is freed
    helper.0.kill().unwrap();
    helper.0.wait().unwrap();
    let taken = waiter.join().unwrap().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "a dead owner is taken over at once"
    );
    let mine = home::read_record(&record).unwrap().unwrap();
    assert_eq!(mine.get_int("pid"), Some(std::process::id() as i64));
    drop(taken);
}

/// The helper process of a test, killed and reaped however the test ends (a
/// failed assertion or a panic included), so it never outlives the test.
struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        // SIGKILL cannot be caught, so the wait that follows is bounded; both
        // answer `Ok` for a child the test already killed and reaped
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The other process of the test above: it takes the lock, says so and
/// blocks until the test kills it. Run only with `LP176_LOCK_HOLD` set. It
/// also ends by itself, within 50 ms of its test process dying (killed, so
/// no [`Holder`] drop ran) and at the latest after 60 s, three times the
/// test's longest wait.
#[test]
fn holds_the_lock_for_another_copy() {
    let Ok(at_home) = std::env::var("LP176_LOCK_HOLD") else {
        return;
    };
    let parent: u32 = std::env::var("LP176_LOCK_PARENT")
        .expect("the test process's pid")
        .parse()
        .unwrap();
    let at_home = PathBuf::from(at_home);
    let mut sink = |_: SdkValue| {};
    let _held = ImageLock::take(&at_home, "docker", ID, &mut Events::new(&mut sink)).unwrap();
    fs::write(at_home.join("helper-held"), b"").unwrap();
    let start = Instant::now();
    while std::os::unix::process::parent_id() == parent && start.elapsed() < Duration::from_secs(60)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_short_machine_is_refused_before_any_effect() {
    let (_scratch, standin, home) = setup("lp176-capacity");
    // larger than twice what any machine has free
    let big = u64::MAX / 8;
    standin.image(ID, &["demo/app:1"], architecture(), big);
    let events = Arc::new(Mutex::new(Vec::new()));
    let error = copy(&standin.docker(), &home, "docker", "demo/app:1", &events).unwrap_err();
    assert_eq!(error.code, "Capacity", "{}", error.message);
    let needed = error.detail.get_int("needed_bytes").unwrap();
    let free = error.detail.get_int("free_bytes").unwrap();
    assert!(needed > free, "{needed} {free}");
    assert_eq!(
        needed as u64,
        docker::need(big),
        "a fresh copy needs the whole image"
    );
    assert_eq!(
        error.detail.get_text("path"),
        Some(cache::cache(&home).to_string_lossy().as_ref()),
        "the refusal names the filesystem it measured"
    );
    assert_eq!(
        error.detail.get_text("fix"),
        Some("layerfs machine prune docker --unused")
    );
    assert!(error.message.contains("more bytes"), "{}", error.message);
    // nothing was created on the machine or in the home
    assert_eq!(standin.creates(), 0, "no export container");
    assert_eq!(standin.starts(), 0, "no export");
    assert_eq!(standin.exports(), 0);
    assert!(!cache::cache(&home).exists(), "no image cache folder");
    assert!(!ImageLock::record_path(&home, "docker", HEX).exists());
}

#[test]
fn a_resumed_copy_needs_only_the_bytes_it_has_not_staged() {
    let (_scratch, standin, home) = setup("lp176-resume-capacity");
    let bytes = filtered(&big_export());
    // a machine with room for neither copy: both are refused, and what is
    // already staged is counted once instead of twice
    let big = u64::MAX / 8;
    standin.image(ID, &["demo/app:1"], architecture(), big);
    let fresh = Arc::new(Mutex::new(Vec::new()));
    let error = copy(&standin.docker(), &home, "docker", "demo/app:1", &fresh).unwrap_err();
    assert_eq!(error.code, "Capacity", "{}", error.message);
    let whole = error.detail.get_int("needed_bytes").unwrap();
    assert_eq!(whole as u64, docker::need(big));
    assert!(error.detail.get_int("free_bytes").unwrap() < whole);
    // 3 of the tar's 4 MiB are staged; the copy needs those 3 MiB less
    let staged = 3 << 20;
    cache::write(&cache::cache(&home), HEX, 0, &bytes[..staged]).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let error = copy(&standin.docker(), &home, "docker", "demo/app:1", &events).unwrap_err();
    assert_eq!(error.code, "Capacity", "{}", error.message);
    let left = error.detail.get_int("needed_bytes").unwrap();
    assert_eq!(
        whole - left,
        staged as i64,
        "the staged bytes are not counted twice"
    );
    assert!(
        error.detail.get_int("free_bytes").is_some(),
        "a refusal names the free bytes"
    );
    // and a refusal changes nothing: no container, no export, same part
    assert_eq!(standin.creates(), 0, "no export container");
    assert_eq!(standin.starts(), 0, "no export");
    let part = cache::cache(&home).join(format!(".recv-{HEX}.part"));
    assert_eq!(fs::symlink_metadata(&part).unwrap().len(), staged as u64);
    assert!(!cache::cache(&home).join(format!("{HEX}.2")).exists());
}

#[test]
fn a_native_copy_in_this_process_takes_the_same_lock() {
    // `machine.load` on native Linux copies in this process (`Target::local`)
    // and takes the very lock a worker copy takes
    let (_scratch, standin, home) = setup("lp176-native-lock");
    let mut sink = |_: SdkValue| {};
    let held = ImageLock::take(
        &home,
        layerfs_home::names::HOST_MACHINE,
        ID,
        &mut Events::new(&mut sink),
    )
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let running = {
        let (docker, at_home, seen) = (standin.docker(), home.clone(), events.clone());
        std::thread::spawn(move || {
            copy(
                &docker,
                &at_home,
                layerfs_home::names::HOST_MACHINE,
                "demo/app:1",
                &seen,
            )
        })
    };
    wait_for("the in-process copy to wait on the same lock", || {
        !phases(&events, "lock").is_empty()
    });
    assert_eq!(standin.starts(), 0, "it exported nothing while locked out");
    assert!(!events_has(&events, "export"));
    drop(held);
    assert!(running.join().unwrap().is_ok());
    assert_eq!(standin.starts(), 1);
    assert!(cache::cache(&home).join(format!("{HEX}.2")).is_dir());
}

/// Whether any event of `phase` was sent.
fn events_has(events: &Arc<Mutex<Vec<SdkValue>>>, phase: &str) -> bool {
    !phases(events, phase).is_empty()
}
