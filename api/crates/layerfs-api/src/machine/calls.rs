//! The machine calls (S §3.7, O11, O34). Record writes take the home lock;
//! a create or connect holds it only to claim and to record, never while
//! the VM boots. One attempt: a create that fails before its VM exists
//! removes what it made, one that fails after keeps the VM as `failed`, and
//! an `Unknown` outcome keeps everything and is logged.
use super::bundle::bundle;
use super::record::{
    self, backend_name, hosted_on, kind_of, machine_of, refusal, removing, sandboxes_on,
    save_running, stores_on, update as update_record,
};
use crate::calls::Context;
use crate::home::{self, lock, paths, read_record, with_field, write_record};
use crate::host::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{self as adapters, Code, HostOs, Platform};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long `machine.list` and `machine.get` wait for their live probes, all
/// together. A probe that has not answered by then is `unknown`.
const PROBE: Duration = Duration::from_secs(10);

/// A machine's live state and, when it could not be read, why.
type Live = Option<(String, Option<String>)>;

fn home_id(home: &Path) -> Result<String> {
    Ok(home::open(home)?.id)
}

/// The live state of each VM and ssh machine, read as `machine.status`
/// reads it: in parallel, starting nothing, outside the home lock, all within
/// [`PROBE`]. A probe that fails or does not answer in time is `unknown`, with
/// the reason; a command it still runs then is killed with its process group
/// and reaped by the probe's thread. Nothing is written back. `None` keeps
/// the record's state: a built-in machine, or one being created or removed.
fn live(home: &Path, records: &[(String, SdkValue)]) -> Result<Vec<Live>> {
    let id = home_id(home)?;
    let (send, answers) = mpsc::channel();
    let late = format!("its probe did not answer within {} s", PROBE.as_secs());
    let mut out: Vec<Live> = vec![None; records.len()];
    let mut waiting = 0;
    let until = Instant::now() + PROBE;
    // the probes' commands are stopped just after the list stops waiting,
    // so a late probe is reported as late, not as its stopped command
    let stop = until + Duration::from_millis(200);
    for (index, (name, record)) in records.iter().enumerate() {
        let in_flight = record.get_text("state") == Some("creating") || removing(record).is_some();
        if kind_of(record)?.built_in() || in_flight {
            continue;
        }
        match record::spec(home, &id, name, record) {
            Ok(spec) => {
                let send = send.clone();
                std::thread::spawn(move || {
                    drop(send.send((index, adapters::status_within(&spec, stop))));
                });
                out[index] = Some(("unknown".to_owned(), Some(late.clone())));
                waiting += 1;
            }
            Err(why) => out[index] = Some(("unknown".to_owned(), Some(why.message))),
        }
    }
    while waiting > 0 {
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            break;
        };
        let Ok((index, answer)) = answers.recv_timeout(left) else {
            break;
        };
        waiting -= 1;
        out[index] = Some(match answer {
            Ok(status) => (status.state.as_str().to_owned(), None),
            Err(error) => ("unknown".to_owned(), Some(refusal(error).message)),
        });
    }
    Ok(out)
}

fn summary(home: &Path, name: &str, record: &SdkValue, live: Live) -> Result<SdkValue> {
    let kind = kind_of(record)?;
    let (state, error) = match live {
        Some((state, error)) => (Some(state), error),
        None => (record.get_text("state").map(str::to_owned), None),
    };
    let list = |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        ("platform", SdkValue::text(kind.name())),
        ("backend", SdkValue::text(backend_name(kind))),
        (
            "state",
            SdkValue::text(state.as_deref().unwrap_or(if kind.built_in() {
                "ready"
            } else {
                "stopped"
            })),
        ),
        ("cpus", record.get("cpus").cloned().unwrap_or_default()),
        ("memory", record.get("memory").cloned().unwrap_or_default()),
        ("disk", record.get("disk").cloned().unwrap_or_default()),
        ("stores", list(stores_on(home, name)?)),
        ("sandboxes", list(sandboxes_on(home, name)?)),
    ];
    for extra in ["image", "host", "nested", "docker", "wsl", "error"] {
        if let Some(value) = record
            .get(extra)
            .filter(|_| extra != "error" || error.is_none())
        {
            fields.push((extra, value.clone()));
        }
    }
    if let Some(error) = error {
        fields.push(("error", SdkValue::Text(error)));
    }
    if let Some(ssh) = record.get("ssh") {
        fields.push(("ssh", ssh.clone()));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// `machine.list()`: every machine with its live state, and the platforms
/// that could be created here.
pub fn list(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let home = &context.home;
    let mut records = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        if let Some(record) = read_record(&paths::machine_record(home, &name))? {
            records.push((name, record));
        }
    }
    let states = live(home, &records)?;
    let mut machines = Vec::new();
    for ((name, record), state) in records.iter().zip(states) {
        machines.push(summary(home, name, record, state)?);
    }
    let tools = record::tools(home)?;
    let mut can = Vec::new();
    for kind in [
        Platform::AppleContainer,
        Platform::Microsandbox,
        Platform::Lima,
        Platform::Firecracker,
        Platform::Ssh,
    ] {
        let probe = adapters::probe(kind, &tools);
        can.push(SdkValue::map([
            ("platform", SdkValue::text(kind.name())),
            ("ready", SdkValue::Bool(probe.ready)),
            (
                "missing",
                SdkValue::List(probe.missing.into_iter().map(SdkValue::Text).collect()),
            ),
            (
                "version",
                probe.version.map(SdkValue::Text).unwrap_or_default(),
            ),
        ]));
    }
    Ok(SdkValue::map([
        ("machines", SdkValue::List(machines)),
        ("can_create", SdkValue::List(can)),
    ]))
}

/// `machine.get(name)`: its record with its live state, Stores and Sandboxes.
pub fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(&context.home, name)?;
    let rows = [(name.to_owned(), record)];
    let state = live(&context.home, &rows)?.pop().flatten();
    summary(&context.home, name, &rows[0].1, state)
}

/// `machine.status(name)`: live state, free disk, the tool's version and
/// why it is down (O34). A stopped VM answers without being started.
pub fn status(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = &context.home;
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(home, name)?;
    let spec = record::spec(home, &home_id(home)?, name, &record)?;
    let status = adapters::status(&spec).map_err(refusal)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("platform", SdkValue::text(spec.kind.name())),
        ("state", SdkValue::text(status.state.as_str())),
        (
            "free_disk",
            status
                .free_disk
                .map(|b| SdkValue::Int(b as i64))
                .unwrap_or_default(),
        ),
        (
            "tool_version",
            status.tool_version.map(SdkValue::Text).unwrap_or_default(),
        ),
        (
            "down_reason",
            status.down_reason.map(SdkValue::Text).unwrap_or_default(),
        ),
        (
            "stores",
            SdkValue::List(
                stores_on(home, name)?
                    .into_iter()
                    .map(SdkValue::Text)
                    .collect(),
            ),
        ),
    ]))
}

/// A new machine's name: valid, not built in, not taken.
fn new_name(home: &Path, name: &str) -> Result<()> {
    crate::names::name("machine", name).map_err(|why| refused("InvalidInput", why))?;
    if crate::names::BUILT_IN.contains(&name) {
        return Err(refused(
            "InvalidInput",
            format!("{name} is a built-in machine"),
        ));
    }
    if paths::machine_dir(home, name).exists() {
        return Err(refused("Exists", format!("a machine named {name} exists")));
    }
    Ok(())
}

/// `machine.create(name, platform, { cpus, memory, disk, image, nested, host })`.
pub fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("").to_owned();
    let platform = args.get_text("platform").unwrap_or("");
    let kind = Platform::parse(platform)
        .ok_or_else(|| refused("InvalidInput", format!("no platform named {platform}")))?;
    if !kind.created() {
        return Err(refused(
            "InvalidInput",
            format!(
                "a {platform} machine is {}",
                if kind == Platform::Ssh {
                    "connected, not created"
                } else {
                    "built in"
                }
            ),
        ));
    }
    if args.get("nested").is_some() && kind != Platform::Lima {
        return Err(refused("InvalidInput", "only a lima machine is nested"));
    }
    if kind == Platform::Firecracker && args.get_text("host").is_none() {
        return Err(refused(
            "InvalidInput",
            "a firecracker machine names its lima --nested host",
        ));
    }
    let probe = adapters::probe(kind, &record::tools(&home)?);
    if !probe.ready {
        return Err(refusal(adapters::MachineError::missing(
            format!("{platform} cannot be created on this host"),
            probe.missing,
        )));
    }
    let bundle = bundle()?;
    let id = home_id(&home)?;
    let mut fields = vec![
        ("name", SdkValue::text(&name)),
        ("platform", SdkValue::text(kind.name())),
        (
            "cpus",
            SdkValue::Int(args.get_int("cpus").unwrap_or(record::DEFAULT_CPUS)),
        ),
        (
            "memory",
            SdkValue::Int(args.get_int("memory").unwrap_or(record::DEFAULT_MEMORY)),
        ),
        (
            "disk",
            SdkValue::Int(args.get_int("disk").unwrap_or(record::DEFAULT_DISK)),
        ),
        ("vm_name", SdkValue::text(record::vm_name(&id, &name))),
        ("state", SdkValue::text("creating")),
        ("created", SdkValue::Int(home::now_ms())),
    ];
    if let Some(image) = args.get_text("image") {
        fields.push(("image", SdkValue::text(image)));
    }
    if matches!(args.get("nested"), Some(SdkValue::Bool(true))) {
        fields.push(("nested", SdkValue::Bool(true)));
    }
    if let Some(host) = args.get_text("host") {
        fields.push(("host", SdkValue::text(host)));
    }
    let record = SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
    // claim the name under the lock; the VM is made outside it
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, &name)?;
        if let Some(host) = args.get_text("host") {
            let host_record = machine_of(&home, host)?;
            if kind_of(&host_record)? != Platform::Lima
                || !matches!(host_record.get("nested"), Some(SdkValue::Bool(true)))
            {
                return Err(refused(
                    "InvalidInput",
                    format!("{host} is not a lima machine made with --nested"),
                ));
            }
        }
        write_record(&paths::machine_record(&home, &name), &record)?;
        record::spec(&home, &id, &name, &record)
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
            return Err(refusal);
        }
    };
    context.progress(SdkValue::map([
        ("phase", SdkValue::text("create")),
        ("machine", SdkValue::text(&name)),
    ]));
    // a first call that needs the machine waits for its create
    let _start = super::start::start_lock(&home, &name)?;
    finish(&home, &name, adapters::create(&spec, &bundle))
}

/// Records the outcome of a create or connect (H §7.1.1 step 5).
fn finish(
    home: &Path,
    name: &str,
    outcome: std::result::Result<adapters::Running, adapters::MachineError>,
) -> Outcome {
    let _lock = lock::take(home)?;
    match outcome {
        Ok(running) => {
            save_running(home, name, &running)?;
            let record = machine_of(home, name)?;
            summary(home, name, &record, None)
        }
        Err(error) if error.code == Code::Unknown => {
            update_record(home, name, |r| {
                let r = with_field(r, "state", SdkValue::text("unknown"));
                with_field(r, "error", SdkValue::text(&error.message))
            })?;
            home::log_line(
                home,
                "uncertain.jsonl",
                &SdkValue::map([
                    ("at", SdkValue::Int(home::now_ms())),
                    ("call", SdkValue::text("machine.create")),
                    ("machine", SdkValue::text(name)),
                    ("message", SdkValue::text(&error.message)),
                ]),
            )?;
            Err(refusal(error))
        }
        Err(error) if error.vm_exists => {
            update_record(home, name, |r| {
                let r = with_field(r, "state", SdkValue::text("failed"));
                with_field(r, "error", SdkValue::text(&error.message))
            })?;
            Err(refusal(error))
        }
        Err(error) => {
            std::fs::remove_dir_all(paths::machine_dir(home, name)).map_err(failed)?;
            Err(refusal(error))
        }
    }
}

/// `machine.connect(name, host, { user, port, identity, known_hosts, root })`.
pub fn connect(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("").to_owned();
    let host = args.get_text("host").unwrap_or("");
    let known_hosts = match args.get_text("known_hosts") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".ssh/known_hosts"))
            .ok_or_else(|| refused("InvalidInput", "known_hosts is unset and so is HOME"))?,
    };
    let probe = adapters::probe(Platform::Ssh, &record::tools(&home)?);
    if !probe.ready {
        return Err(refusal(adapters::MachineError::missing(
            "ssh is not usable here",
            probe.missing,
        )));
    }
    let bundle = bundle()?;
    let id = home_id(&home)?;
    let ssh = SdkValue::map([
        ("host", SdkValue::text(host)),
        (
            "user",
            SdkValue::text(args.get_text("user").unwrap_or("root")),
        ),
        ("port", SdkValue::Int(args.get_int("port").unwrap_or(22))),
        (
            "identity",
            args.get("identity").cloned().unwrap_or_default(),
        ),
        (
            "known_hosts",
            SdkValue::text(known_hosts.display().to_string()),
        ),
        (
            "root",
            SdkValue::text(args.get_text("root").unwrap_or(adapters::DATA)),
        ),
    ]);
    let record = SdkValue::map([
        ("name", SdkValue::text(&name)),
        ("platform", SdkValue::text("ssh")),
        ("ssh", ssh),
        ("state", SdkValue::text("creating")),
        ("created", SdkValue::Int(home::now_ms())),
    ]);
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, &name)?;
        write_record(&paths::machine_record(&home, &name), &record)?;
        // the accepted host key is checked strictly against the caller's file
        let copied = std::fs::copy(
            &known_hosts,
            paths::machine_dir(&home, &name).join("known_hosts"),
        );
        match copied
            .map_err(|e| refused("InvalidInput", format!("{}: {e}", known_hosts.display())))
            .and_then(|_| record::spec(&home, &id, &name, &record))
        {
            Ok(spec) => spec,
            Err(refusal) => {
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
                return Err(refusal);
            }
        }
    };
    // a second machine on the same host is one kernel twice
    let identity = match adapters::host_identity(&spec) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
            return Err(refusal(error));
        }
    };
    {
        let _lock = lock::take(&home)?;
        for other in paths::names(&home.join("machine")).map_err(failed)? {
            let same = read_record(&paths::machine_record(&home, &other))?.and_then(|r| {
                r.get("ssh")
                    .and_then(|s| s.get_text("host_id"))
                    .map(|h| h == identity)
            });
            if other != name && same == Some(true) {
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
                return Err(refused(
                    "Exists",
                    format!("machine {other} is the same host"),
                ));
            }
        }
        update_record(&home, &name, |r| {
            let ssh = with_field(
                r.get("ssh").cloned().unwrap_or_default(),
                "host_id",
                SdkValue::text(&identity),
            );
            with_field(r, "ssh", ssh)
        })?;
    }
    finish(&home, &name, adapters::connect(&spec, &bundle))
}

/// `InUse` while the machine holds Stores, Sandboxes or Firecracker machines.
fn idle(home: &Path, name: &str) -> Result<()> {
    let stores = stores_on(home, name)?;
    let sandboxes = sandboxes_on(home, name)?;
    let hosted = hosted_on(home, name)?;
    if stores.is_empty() && sandboxes.is_empty() && hosted.is_empty() {
        return Ok(());
    }
    Err(refused(
        "InUse",
        format!(
            "machine {name} holds Stores [{}], Sandboxes [{}] and hosts [{}]",
            stores.join(", "),
            sandboxes.join(", "),
            hosted.join(", ")
        ),
    ))
}

/// The home lock covers only the checks and the record writes: the record
/// says `removing` (with this process) while the VM is deleted under the
/// machine's start lock, so no call starts it, moves to it or runs on it
/// meanwhile. A refused delete puts the record back as it was; one whose
/// outcome is unknown records `unknown`.
fn take_down(context: &mut Context<'_>, args: &SdkValue, ssh: bool) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    machine_of(&home, name)?;
    let _start = super::start::start_lock(&home, name)?;
    let (record, spec) = {
        let _lock = lock::take(&home)?;
        let (record, spec) = removable(&home, name, ssh)?;
        let marked = with_field(
            record.clone(),
            "removing",
            SdkValue::map([("pid", SdkValue::Int(i64::from(std::process::id())))]),
        );
        write_record(&paths::machine_record(&home, name), &marked)?;
        (record, spec)
    };
    if let Err(error) = adapters::remove(&spec) {
        // an unknown outcome says so in the record; a refused one leaves it as it was
        let record = match error.code {
            Code::Unknown => with_field(
                with_field(record, "state", SdkValue::text("unknown")),
                "error",
                SdkValue::text(&error.message),
            ),
            _ => record,
        };
        let _lock = lock::take(&home)?;
        write_record(&paths::machine_record(&home, name), &record)?;
        return Err(refusal(error));
    }
    let _lock = lock::take(&home)?;
    std::fs::remove_dir_all(paths::machine_dir(&home, name)).map_err(failed)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("removed", SdkValue::Bool(true)),
    ]))
}

/// The checks before a remove or disconnect, under the home lock: the
/// machine's record and spec.
fn removable(home: &Path, name: &str, ssh: bool) -> Result<(SdkValue, adapters::Spec)> {
    let record = machine_of(home, name)?;
    let kind = kind_of(&record)?;
    if kind.built_in() {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in and never removed"),
        ));
    }
    if (kind == Platform::Ssh) != ssh {
        let call = if ssh {
            "machine remove"
        } else {
            "machine disconnect"
        };
        return Err(refused(
            "InvalidInput",
            format!("{name} is a {} machine; use {call}", kind.name()),
        ));
    }
    idle(home, name)?;
    let spec = record::spec(home, &home_id(home)?, name, &record)?;
    Ok((record, spec))
}

/// `machine.remove(name)`: deletes a VM and its disk.
pub fn remove(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    take_down(context, args, false)
}

/// `machine.disconnect(name)`: forgets an ssh host; its data root stays.
pub fn disconnect(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    take_down(context, args, true)
}

/// `machine.update(name, { cpus, memory, image, docker, wsl })` (O11): the
/// built-in machines' image, Docker socket and WSL distribution. Resizing
/// a VM is refused while Sandboxes run on it.
pub fn update(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let _lock = lock::take(&home)?;
    let record = machine_of(&home, name)?;
    let kind = kind_of(&record)?;
    let resize = args.get("cpus").is_some() || args.get("memory").is_some();
    if kind.built_in() && resize {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in; it has no size of its own"),
        ));
    }
    if !kind.built_in() && (args.get("docker").is_some() || args.get("wsl").is_some()) {
        return Err(refused(
            "InvalidInput",
            "docker and wsl are the built-in machines' settings",
        ));
    }
    if args.get("wsl").is_some() && HostOs::current() != HostOs::Windows {
        return Err(refused("InvalidInput", "wsl is a Windows host's setting"));
    }
    // a running Sandbox keeps the image and settings it started with
    let running = sandboxes_on(&home, name)?;
    if !running.is_empty() {
        return Err(refused(
            "InUse",
            format!(
                "Sandboxes [{}] run on {name}; close them before changing it",
                running.join(", ")
            ),
        ));
    }
    if resize {
        return Err(refused(
            "Unsupported",
            format!(
                "resizing a {} machine is not built yet; nothing was changed",
                kind.name()
            ),
        ));
    }
    let mut record = record;
    for field in ["image", "docker", "wsl"] {
        if let Some(value) = args.get(field) {
            record = with_field(record, field, value.clone());
        }
    }
    write_record(&paths::machine_record(&home, name), &record)?;
    summary(&home, name, &record, None)
}
