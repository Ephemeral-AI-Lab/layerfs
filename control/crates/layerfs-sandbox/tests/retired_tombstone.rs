//! Which removals write a Sandbox's retired tombstone (#784 review). A
//! tombstone equal to the generation, with the home record gone, proves
//! that Sandbox's mounts dead, so only a removal whose runtime is confirmed
//! stopped, confirmed gone or never started writes one. A removal that
//! leaves a runtime it did not stop writes none: no evidence is the safe
//! outcome. Each case drives `sandbox.remove`, the close its side runs, or
//! `sandbox.create`, never the tombstone write alone. Nothing runs a daemon.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_home::home::{self, paths, read_record, write_record};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};
use layerfs_sandbox::host::{Host, Message};
use std::path::{Path, PathBuf};

const GENERATION: &str = "p1:aa11.sandbox:0000000000000010:aa";

/// A record of `fields`.
fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new home.
fn new_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-tomb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    home::ensure(&dir).unwrap();
    dir
}

/// `sandbox.remove(name, { force })`.
fn removed(home: &Path, name: &str, force: bool) -> Result<Removed, SdkRefusal> {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = RemoveRequest {
        name: name.to_owned(),
        force,
    };
    remove(home, &mut events, &request, local)
}

/// The `left` of a remove that answered the Sandbox gone.
fn gone_left(outcome: Result<Removed, SdkRefusal>) -> Vec<String> {
    match outcome {
        Ok(Removed::AlreadyGone { left }) => left,
        Ok(Removed::Stopped { left, .. }) => panic!("taken as stopped: {left:?}"),
        Err(refusal) => panic!("{refusal:?}"),
    }
}

/// Whether the home holds a retired tombstone of `name` on `machine`.
fn tombstoned(home: &Path, machine: &str, name: &str) -> bool {
    paths::sandbox_tombstone(home, machine, name).exists()
}

/// Machine `dev` (`fields` over a running lima VM's record), with Store
/// `shop` and its project `api` recorded on it.
fn on_dev(home: &Path, fields: &[(&str, SdkValue)]) {
    let mut machine = vec![
        ("name", t("dev")),
        ("platform", t("lima")),
        ("vm_name", t("layerfs-00000000-dev")),
        ("state", t("running")),
    ];
    for (name, value) in fields {
        machine.retain(|(kept, _)| kept != name);
        machine.push((name, value.clone()));
    }
    write_record(&paths::machine_record(home, "dev"), &map(machine)).unwrap();
    write_record(
        &paths::store_record(home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("dev")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
}

/// Sandbox `name`'s started record on `machine`, with generation
/// [`GENERATION`] and `daemon` and `extra` fields.
fn sandbox(home: &Path, name: &str, machine: &str, daemon: SdkValue, extra: &[(&str, &str)]) {
    let mut fields = vec![
        ("name", t(name)),
        ("id", t(name)),
        ("project", t("shop/api")),
        ("machine", t(machine)),
        ("generation", t(GENERATION)),
        ("daemon", daemon),
    ];
    fields.extend(extra.iter().map(|(n, v)| (*n, t(v))));
    write_record(&paths::sandbox_record(home, name), &map(fields)).unwrap();
}

/// A native daemon record this build does not read (no `port`), under the
/// name this home gives Sandbox `name`.
fn unread_daemon(home: &Path, name: &str) -> SdkValue {
    let id = home::opened(home).unwrap().id;
    let given = format!("{}{name}", layerfs_home::names::provider_prefix(&id));
    SdkValue::map([
        ("runtime", t("process")),
        ("platform", t("mac")),
        ("name", t(&given)),
        ("container", t(&"ab".repeat(32))),
        ("directory", t("/nonexistent/lfs-tomb")),
    ])
}

/// A record of the retired docker backend.
fn docker_daemon() -> SdkValue {
    SdkValue::map([
        ("platform", t("docker")),
        ("name", t("lfs-0123abcd-old")),
        ("container", t(&"c".repeat(64))),
    ])
}

/// Store `shop` with project `api` on machine `native`, whose daemon and
/// image archive are never run here; `false` (the home removed) where the
/// Store's filesystem is unsupported.
fn native_store(home: &Path) -> bool {
    use layerfs_store::project::add::{add_project, ProjectAdd};
    match layerfs_store::sqlite::create::create(home, "shop", None) {
        Ok(_) => {}
        Err(refusal) if refusal.code == "Unsupported" => {
            eprintln!("NOT_RUN: unsupported Store filesystem: {refusal:?}");
            let _ = std::fs::remove_dir_all(home);
            return false;
        }
        Err(refusal) => panic!("create: {refusal:?}"),
    }
    let source = home.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), b"hello\n").unwrap();
    add_project(&ProjectAdd {
        home: home.to_path_buf(),
        store: layerfs_home::names::StoreName::parse("shop").unwrap(),
        name: "api".to_owned(),
        source,
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    layerfs_store::open::close_idle(std::time::Duration::ZERO);
    let machine = paths::machine_record(home, layerfs_home::names::HOST_MACHINE);
    let record = read_record(&machine).unwrap().unwrap_or(SdkValue::Null);
    let record = home::with_field(record, "daemon", t("/bin/sh"));
    let record = home::with_field(record, "image_archive", t("/bin/sh"));
    let record = home::with_field(record, "platform", t(layerfs_home::names::HOST_MACHINE));
    write_record(&machine, &record).unwrap();
    true
}

/// A local record this build does not read is forgotten by its records
/// alone, no signal sent: its daemon may still run, so no tombstone.
#[test]
fn an_unread_record_is_forgotten_without_a_tombstone() {
    let home = new_home("unread");
    sandbox(
        &home,
        "s1",
        layerfs_home::names::HOST_MACHINE,
        unread_daemon(&home, "s1"),
        &[],
    );
    let left = gone_left(removed(&home, "s1", false));
    assert!(
        left.iter().any(|said| said.contains("was not stopped")),
        "{left:?}"
    );
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    assert!(
        !tombstoned(&home, layerfs_home::names::HOST_MACHINE, "s1"),
        "a tombstone proves a daemon that may still run dead"
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// A native record this build reads whose daemon keys are gone: its close
/// answers `NoSuchSandbox` without inspecting the runtime, so the record
/// goes and no tombstone is written (only a daemon its side found gone or
/// stopped, `containerGone`, proves its mounts dead).
#[cfg(target_os = "macos")]
#[test]
fn a_sandbox_without_daemon_keys_is_removed_without_a_tombstone() {
    let home = new_home("nokeys");
    if !native_store(&home) {
        return;
    }
    let SdkValue::Map(mut daemon) = unread_daemon(&home, "s1") else {
        unreachable!()
    };
    let dir = paths::sandbox_dir(&home, "s1");
    daemon.retain(|(name, _)| name != "directory");
    daemon.extend([
        ("directory".to_owned(), t(&dir.display().to_string())),
        ("pid".to_owned(), SdkValue::Int(i64::from(i32::MAX))),
        ("start".to_owned(), t("1")),
        ("boot".to_owned(), t(&"0".repeat(32))),
        ("port".to_owned(), SdkValue::Int(1)),
    ]);
    sandbox(
        &home,
        "s1",
        layerfs_home::names::HOST_MACHINE,
        SdkValue::Map(daemon),
        &[],
    );
    gone_left(removed(&home, "s1", false));
    assert!(!dir.exists());
    assert!(
        !tombstoned(&home, layerfs_home::names::HOST_MACHINE, "s1"),
        "no runtime was inspected, so nothing proves its mounts dead"
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// A retired docker record on a machine whose worker is not reached: the
/// home's record goes, the worker keeps its own and the container stays,
/// so no tombstone is written.
#[test]
fn a_forget_whose_worker_keeps_its_record_writes_no_tombstone() {
    let home = new_home("kept");
    on_dev(&home, &[]);
    sandbox(
        &home,
        "s1",
        "dev",
        docker_daemon(),
        &[("backend", "docker")],
    );
    let left = gone_left(removed(&home, "s1", false));
    assert!(
        left.iter()
            .any(|said| said.contains("worker keeps its record")),
        "{left:?}"
    );
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    assert!(!tombstoned(&home, "dev", "s1"));
    std::fs::remove_dir_all(home).unwrap();
}

/// A machine whose VM a live read found missing does not prove the
/// Sandbox's runtime gone: a provider listing can miss a VM that comes back,
/// its daemon still running. Its records go with no tombstone, and `left`
/// names the repair that ends its mount intents. A machine record with no
/// 2.0 name says nothing of what runs there: none is written either.
/// (Before the #787 review a missing VM wrote the tombstone.)
#[test]
fn a_missing_vm_and_a_nameless_machine_write_no_tombstone() {
    let home = new_home("vm");
    on_dev(&home, &[("reason", t("missing"))]);
    sandbox(&home, "s1", "dev", docker_daemon(), &[]);
    let left = gone_left(removed(&home, "s1", false));
    assert!(
        left.iter().any(|said| said.contains("VM was found missing")
            && said.contains("no retired tombstone was written")
            && said.contains("layerfs store repair-mount-holders shop/api <branch> list")),
        "{left:?}"
    );
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    assert!(
        !tombstoned(&home, "dev", "s1"),
        "a VM recorded missing may still run the daemon: no tombstone"
    );

    on_dev(&home, &[("vm_name", SdkValue::Null)]);
    sandbox(&home, "s2", "dev", docker_daemon(), &[]);
    let left = gone_left(removed(&home, "s2", false));
    assert!(
        left.iter().any(|said| said.contains("no 2.0 name")),
        "{left:?}"
    );
    assert!(!paths::sandbox_dir(&home, "s2").exists());
    assert!(!tombstoned(&home, "dev", "s2"));
    std::fs::remove_dir_all(home).unwrap();
}

/// A machine recorded stopped, whose forced `machine.stop` ended the
/// Sandbox's runtime (`machine_stopped`): its daemon ended with the VM, so
/// the remove writes the tombstone of the generation it recorded.
#[test]
fn a_machine_recorded_stopped_writes_the_tombstone() {
    let home = new_home("stopped");
    on_dev(&home, &[("state", t("stopped"))]);
    let record = map(vec![
        ("name", t("s1")),
        ("id", t("s1")),
        ("project", t("shop/api")),
        ("machine", t("dev")),
        ("generation", t(GENERATION)),
        ("platform", t("lima")),
        ("runtime", t("namespaces")),
        ("backend", t("namespaces")),
        ("machine_stopped", SdkValue::Bool(true)),
        ("machine_stop_unknown", SdkValue::Bool(false)),
    ]);
    write_record(&paths::sandbox_record(&home, "s1"), &record).unwrap();
    let left = gone_left(removed(&home, "s1", false));
    assert!(
        left.iter()
            .any(|said| said.contains("machine dev is stopped")),
        "{left:?}"
    );
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    let tombstone = read_record(&paths::sandbox_tombstone(&home, "dev", "s1"))
        .unwrap()
        .expect("the stopped machine's tombstone");
    assert_eq!(tombstone.get_text("generation"), Some(GENERATION));
    std::fs::remove_dir_all(home).unwrap();
}

/// The close a Sandbox's own side runs forgets a retired docker record and
/// a record it does not read without stopping anything: no tombstone.
#[test]
fn the_close_forgets_docker_and_unread_records_without_a_tombstone() {
    let home = new_home("close");
    let host = |name: &str| Host {
        home: home.clone(),
        store: "shop".to_owned(),
        project: "shop/api".to_owned(),
        sandbox: Some(name.to_owned()),
        store_record: None,
        store_path: None,
        deploy: None,
        postgresql: None,
        message: Message::Absent,
        started: None,
        reach_hint: None,
    };
    sandbox(
        &home,
        "s1",
        layerfs_home::names::HOST_MACHINE,
        docker_daemon(),
        &[],
    );
    let answer = layerfs_sandbox::lifecycle::close(&host("s1"), false, &[], &NoDrain).unwrap();
    assert_eq!(answer.get_text("kind"), Some("Forgotten"), "{answer:?}");
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    assert!(!tombstoned(&home, layerfs_home::names::HOST_MACHINE, "s1"));

    sandbox(
        &home,
        "s2",
        layerfs_home::names::HOST_MACHINE,
        unread_daemon(&home, "s2"),
        &[],
    );
    let answer = layerfs_sandbox::lifecycle::close(&host("s2"), false, &[], &NoDrain).unwrap();
    assert_eq!(answer.get_text("kind"), Some("Forgotten"), "{answer:?}");
    assert!(!paths::sandbox_dir(&home, "s2").exists());
    assert!(!tombstoned(&home, layerfs_home::names::HOST_MACHINE, "s2"));
    std::fs::remove_dir_all(home).unwrap();
}

/// What the stand-in for a Sandbox's own side was sent: the create's place.
static SENT: std::sync::Mutex<Option<Option<(String, String)>>> = std::sync::Mutex::new(None);

/// A stand-in for the side that starts a Sandbox on `native`: it records
/// the place the create sent and answers the Sandbox started.
fn started_here(host: Host, call: layerfs_sandbox::host::Call) -> layerfs_home::outcome::Outcome {
    let layerfs_sandbox::host::Call::Create { place, .. } = call else {
        return Err(layerfs_home::outcome::refused(
            "UnknownMethod",
            "not a create",
        ));
    };
    *SENT.lock().unwrap() = Some(place);
    let text = |s: &str| Some(s.to_owned());
    Ok(layerfs_sandbox::host::Started {
        sandbox: host.sandbox.clone(),
        name: text("lfs-stand-in"),
        image: text("local"),
        platform: text("mac"),
        runtime: text("process"),
        route: text("nfs"),
        runtime_id: Some("ab".repeat(32)),
        network: None,
        control: None,
    }
    .encode())
}

/// `sandbox.create` of a name a removal retired: once the started record,
/// with its newer generation, is written, the name's older tombstone goes
/// (that record proves the older generation's mounts dead by itself), and
/// the create sent its side the generation it recorded. Its side is a
/// stand-in (`create_on`): nothing runs a daemon.
#[test]
fn a_recreate_drops_the_older_tombstone() {
    let home = new_home("recreate");
    if !native_store(&home) {
        return;
    }

    let id = home::opened(&home).unwrap().id;
    let old = format!("p1:{id}.sandbox:0000000000000010:aa");
    layerfs_sandbox::record::SandboxTombstone {
        name: "s1".to_owned(),
        machine: layerfs_home::names::HOST_MACHINE.to_owned(),
        generation: old,
        retired_ms: 1,
    }
    .write(&home)
    .unwrap();
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = layerfs_sandbox::family::CreateRequest {
        project: "shop/api".to_owned(),
        name: "s1".to_owned(),
        ..Default::default()
    };
    let created = layerfs_sandbox::family::create_on(&home, &mut events, &request, started_here)
        .unwrap_or_else(|refusal| panic!("{refusal:?}"));
    let generation = created.record.generation().expect("a minted generation");
    assert_eq!(created.record.machine(), layerfs_home::names::HOST_MACHINE);
    assert_eq!(
        SENT.lock().unwrap().clone(),
        Some(Some((
            layerfs_home::names::HOST_MACHINE.to_owned(),
            generation.to_owned()
        ))),
        "the create sent its side another place"
    );
    assert!(
        !tombstoned(&home, layerfs_home::names::HOST_MACHINE, "s1"),
        "the older tombstone stayed"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// A close here has no mounted Workspace to drain.
struct NoDrain;

impl layerfs_sandbox::lifecycle::WorkspaceDrain for NoDrain {
    fn drain(&self, _: &layerfs_sandbox::host::Host, _: &str) {}
}

/// The in-process runner `remove` takes (the workspace crate's dispatch in
/// the product): the Sandbox's own dispatch, with no Workspace to drain.
fn local(
    host: layerfs_sandbox::host::Host,
    call: layerfs_sandbox::host::Call,
) -> layerfs_home::outcome::Outcome {
    layerfs_sandbox::dispatch::call(host, call, &NoDrain)
}
