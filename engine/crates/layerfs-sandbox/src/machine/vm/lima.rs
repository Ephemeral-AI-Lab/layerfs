//! `lima` (`limactl`, `vmType: vz`, macOS only): one Lima instance per
//! machine, made from a template written to `limactl create`'s stdin.
//!
//! - The data disk is a raw Lima disk that Lima formats ext4 on first attach
//!   and mounts at `/mnt/lima-<disk>`; it is bind-mounted at `/layerfs/data`
//!   after every boot. Lima finds a formatted disk by its ext4 label
//!   `lima-<disk>`, which holds 16 bytes, so the disk is named `lfs` and 8 hex
//!   digits of the VM name (a longer label is cut, never found, and formatted
//!   again at every boot; found on Lima 2.2.1, #527).
//! - The template mounts nothing (`mounts: []`): the guest never sees the
//!   host home. Its one port forward is guest `127.0.0.1:7070` to host
//!   `127.0.0.1:<port>`; every other guest port is ignored.
//! - `nested` adds `nestedVirtualization` (macOS 15+, M3 or newer): such a
//!   machine hosts Firecracker machines, and the bundle's `firecracker/`
//!   folder (binary, kernel, root filesystem, guest key), when it has one, is
//!   copied to `/layerfs/data/firecracker`.
//! - Commands go through ssh with Lima's own `ssh.config`, then `sudo -n`, so
//!   the quoting is this crate's, not the CLI's.
use super::shared::{call, checked, exists, mib, new_port, not_found, recorded_port, BOOT, CALL};
use crate::machine::error::{kept, Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::kind::HostOs;
use crate::machine::probe::{first_line, nested_missing};
use crate::machine::setup::{with_worker, State, Status};
use crate::machine::spec::{Bundle, Lasting, Running, Spec, WORKER_PORT};
use crate::machine::{facts, tool, worker};
use std::path::PathBuf;

const CLI: &str = "limactl";
const UBUNTU: &str =
    "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg";
/// Where a nested machine keeps Firecracker's files and microVMs.
pub(crate) const FIRECRACKER_ROOT: &str = "/layerfs/data/firecracker";

/// The data disk at `/layerfs/data`, by fact: Lima's mount must be ext4.
const ATTACH: &str = r#"d=/mnt/lima-$1
test "$(findmnt -n -o FSTYPE "$d")" = ext4 || { echo "$d is not an ext4 mount" >&2; exit 1; }
mkdir -p /layerfs/data && { mountpoint -q /layerfs/data || mount --bind "$d" /layerfs/data; }"#;

/// A nested machine's Firecracker files, from the bundle copy, once per bundle hash.
const FIRECRACKER_FILES: &str = r#"src=/layerfs/bundle/firecracker dst=$2
[ -d "$src" ] || exit 0
[ "$(cat "$dst/.sealed" 2>/dev/null)" = "$1" ] && exit 0
mkdir -p "$dst" && cp -Rp --remove-destination "$src/." "$dst/" && echo "$1" > "$dst/.sealed""#;

/// The Lima disk of a machine: a name whose `lima-` label fits 16 bytes.
pub(crate) fn disk_name(vm_name: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in vm_name.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("lfs{:08x}", hash >> 32)
}

/// The instance's template, as JSON (which is YAML).
pub(crate) fn template(spec: &Spec, port: u16) -> String {
    let images = match spec.os_image.as_deref() {
        Some(image) if image.starts_with("https://") || image.starts_with('/') => {
            format!("[{{\"location\": {}}}]", quote(image))
        }
        _ => format!(
            "[{{\"location\": \"{UBUNTU}-arm64.img\", \"arch\": \"aarch64\"}}, \
             {{\"location\": \"{UBUNTU}-amd64.img\", \"arch\": \"x86_64\"}}]"
        ),
    };
    let nested = if spec.nested {
        "\n  \"nestedVirtualization\": true,"
    } else {
        ""
    };
    format!(
        r#"{{
  "vmType": "vz",{nested}
  "cpus": {cpus},
  "memory": "{memory}iB",
  "images": {images},
  "containerd": {{"system": false, "user": false}},
  "mounts": [],
  "additionalDisks": [{{"name": "{disk}", "format": true, "fsType": "ext4"}}],
  "portForwards": [
    {{"guestIP": "127.0.0.1", "guestPort": {WORKER_PORT}, "hostIP": "127.0.0.1", "hostPort": {port}}},
    {{"guestIP": "0.0.0.0", "guestPortRange": [1, 65535], "ignore": true}}
  ]
}}
"#,
        cpus = spec.cpus,
        memory = mib(spec.memory),
        disk = disk_name(&spec.vm_name),
    )
}

fn quote(text: &str) -> String {
    let mut out = Vec::new();
    crate::json::quoted(&mut out, text).expect("writing to a Vec");
    String::from_utf8(out).expect("JSON text is UTF-8")
}

/// The instance's state and folder, from `limactl list`.
pub(crate) fn observe(spec: &Spec) -> Result<(State, Option<PathBuf>), MachineError> {
    let out = call(
        spec,
        CLI,
        &["list", "--format", "{{.Status}}\t{{.Dir}}", &spec.vm_name],
        CALL,
    )?;
    let line = out.stdout.trim();
    if line.is_empty() || out.stderr.contains("No instance matching") {
        if out.ok() || out.stderr.contains("No instance matching") {
            return Ok((State::Missing, None));
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("limactl list failed: {}", out.said()),
        ));
    }
    let (status, dir) = line.split_once('\t').unwrap_or((line, ""));
    let state = match status {
        "Running" => State::Running,
        "Broken" => State::Failed,
        _ => State::Stopped,
    };
    Ok((state, (!dir.is_empty()).then(|| PathBuf::from(dir))))
}

/// The guest of a running instance.
pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    match observe(spec)? {
        (State::Running, Some(dir)) => Ok(Guest::Lima {
            dir,
            vm: spec.vm_name.clone(),
        }),
        (state, _) => Err(MachineError::new(
            Code::Unavailable,
            format!("the lima machine {} is {}", spec.name, state.as_str()),
        )),
    }
}

/// What a lima machine needs on this host before anything is made.
fn host_check(spec: &Spec) -> Result<(), MachineError> {
    let mut missing = Vec::new();
    if HostOs::current() != HostOs::Macos {
        missing.push("host:macos".to_owned());
    }
    if spec.nested {
        missing.extend(nested_missing(HostOs::current()));
    }
    if tool::lookup(&spec.tools, CLI).is_none() {
        missing.push(format!("tool:{CLI}"));
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(MachineError::missing(
        format!(
            "a lima machine{} cannot run on this host",
            if spec.nested {
                " with nested virtualization"
            } else {
                ""
            }
        ),
        missing,
    ))
}

fn disk_exists(spec: &Spec, disk: &str) -> Result<bool, MachineError> {
    let text = checked(spec, CLI, &["disk", "list", "--json"], CALL)?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let name = facts::json_string(line, &["name"])
            .map_err(|e| MachineError::new(Code::Failed, format!("limactl disk list: {e}")))?;
        if name.as_deref() == Some(disk) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    host_check(spec)?;
    if observe(spec)?.0 != State::Missing {
        return Err(exists(spec, "the Lima instance"));
    }
    let disk = disk_name(&spec.vm_name);
    if disk_exists(spec, &disk)? {
        return Err(MachineError::new(
            Code::Exists,
            format!(
                "the Lima disk {disk} for {} already exists; it is left alone",
                spec.vm_name
            ),
        ));
    }
    let port = new_port(spec)?;
    let size = format!("{}iB", mib(spec.disk));
    checked(
        spec,
        CLI,
        &["disk", "create", &disk, "--size", &size, "--format", "raw"],
        CALL,
    )?;
    let command = tool::command(
        &tool::find(&spec.tools, CLI)?,
        ["create", "--name", &spec.vm_name, "--tty=false", "-"],
    );
    let made = tool::run(
        command,
        Some(template(spec, port).as_bytes()),
        CALL,
        "limactl create",
    )?;
    if !made.ok() {
        if observe(spec)?.0 != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("limactl create failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: the disk this create made goes (one cleanup)
        let _ = call(spec, CLI, &["disk", "delete", &disk], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("limactl create failed: {}", made.said()),
        ));
    }
    kept(boot_and_bring_up(spec, bundle, port, true))
}

/// Boots the instance if it is stopped, attaches its disk, and starts its worker.
fn boot_and_bring_up(
    spec: &Spec,
    bundle: &Bundle,
    port: u16,
    boot: bool,
) -> Result<Running, MachineError> {
    if boot {
        checked(spec, CLI, &["start", "--tty=false", &spec.vm_name], BOOT)?;
    }
    let guest = guest(spec)?;
    guest.script(
        &spec.tools,
        "attaching the data disk",
        ATTACH,
        &[&disk_name(&spec.vm_name)],
    )?;
    let endpoint = worker::bring_up(spec, &guest, bundle, port, None)?;
    if spec.nested {
        guest.script(
            &spec.tools,
            "placing Firecracker's files",
            FIRECRACKER_FILES,
            &[&bundle.hash, FIRECRACKER_ROOT],
        )?;
    }
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..spec.state
        },
    })
}

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    match observe(spec)?.0 {
        State::Missing => Err(not_found(spec)),
        State::Failed => Err(MachineError::new(
            Code::Failed,
            format!("the Lima instance {} is broken", spec.vm_name),
        )),
        State::Stopped => boot_and_bring_up(spec, bundle, port, true),
        State::Running => boot_and_bring_up(spec, bundle, port, false),
    }
}

/// `limactl stop` keeps the instance and its disk; the worker ends with it.
pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)?.0 == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], BOOT)?;
    }
    Ok(())
}

/// The instance, then its data disk.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)?.0 != State::Missing {
        checked(spec, CLI, &["delete", "--force", &spec.vm_name], BOOT)?;
    }
    let disk = disk_name(&spec.vm_name);
    if disk_exists(spec, &disk)? {
        checked(spec, CLI, &["disk", "delete", &disk], CALL)?;
    }
    Ok(())
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = tool::lookup(&spec.tools, CLI)
        .and_then(|p| first_line(tool::command(&p, ["--version"]), false));
    let (kernel, dir) = observe(spec)?;
    let free = match (kernel, dir) {
        (State::Running, Some(dir)) => worker::free_disk(
            spec,
            &Guest::Lima {
                dir,
                vm: spec.vm_name.clone(),
            },
        ),
        _ => None,
    };
    Ok(with_worker(spec, kernel, free, version))
}
