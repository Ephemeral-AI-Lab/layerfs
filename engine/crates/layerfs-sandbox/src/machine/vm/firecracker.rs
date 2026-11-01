//! `firecracker`: one Firecracker microVM per machine, run inside a `lima`
//! machine made with `nested` (macOS has no KVM; nested KVM under `vz` needs
//! an M3 or newer and macOS 15+). Every file lives on the host's data disk
//! under `/layerfs/data/firecracker`: `bin/firecracker`, the guest kernel
//! `vmlinux`, the base root filesystem `rootfs.ext4` (an sshd that accepts
//! the key, util-linux) and that key, `id_ed25519`. They come from the
//! bundle's `firecracker/` folder when the nested host is made; a bundle
//! without them leaves creation refusing `Unsupported`, naming each missing
//! file.
//!
//! Each machine gets `vms/<vm>/` (its own root filesystem copy, config and
//! `vm.env`), a tap `lfsfc<n>` with the `/30` network `172.30.<n>.0` (host
//! `.1`, guest `.2`; `n` is lasting state, at most 250 per host), and the
//! data disk `disks/<vm>.ext4`, mounted at `/layerfs/data` in the microVM.
//! The worker is reached by `ssh -L 127.0.0.1:<port>:172.30.<n>.2:7070`
//! to the Lima host, started detached. The microVM's ssh host key is not
//! checked: its address is a point-to-point tap inside the host, reused by
//! the next VM of that number, and every root filesystem is one image's copy.
use super::shared::{new_port, not_found, BOOT};
use super::{forward, lima};
use crate::machine::error::{kept, Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::probe::first_line;
use crate::machine::setup::{with_worker, State, Status};
use crate::machine::spec::{Bundle, Lasting, Running, Spec, WORKER_PORT};
use crate::machine::{tool, worker};

const ROOT: &str = lima::FIRECRACKER_ROOT;

/// One line per missing thing, so the refusal can name each.
const CHECK: &str = r#"test -c /dev/kvm || echo "/dev/kvm"
for f in bin/firecracker vmlinux rootfs.ext4 id_ed25519; do test -f "$1/$f" || echo "$1/$f"; done
for t in mkfs.ext4 ip flock ssh truncate; do command -v $t >/dev/null || echo "tool:$t"; done
true"#;

/// The data disk, made once; exit 17 when one is already there.
const DISK: &str = r#"set -e
f="$1/disks/$2.ext4"
test ! -e "$f" || { echo "$f already exists" >&2; exit 17; }
mkdir -p "$1/disks" && truncate -s "$3" "$f.part" && mkfs.ext4 -q -F "$f.part" && mv "$f.part" "$f""#;

/// The VM's folder under a lock: network `n` (`$6`, or the first free one
/// for 0), a root filesystem copy and the config. Prints `n`.
const CREATE: &str = r#"set -eu
root=$1 name=$2 disk=$3 cpus=$4 mem=$5 want=$6
d="$root/vms/$name"
mkdir -p "$root/vms"
exec 9>"$root/vms/.lock"
flock 9
test ! -e "$d" || { echo "$d already exists" >&2; exit 17; }
taken() { grep -qx "INDEX=$1" "$root"/vms/*/vm.env 2>/dev/null; }
if [ "$want" -gt 0 ]; then n=$want; ! taken "$n" || { echo "network $n is taken" >&2; exit 1; }
else n=1; while taken "$n"; do n=$((n+1)); done; fi
[ "$n" -le 250 ] || { echo "every network number is taken" >&2; exit 1; }
mkdir "$d"
printf 'INDEX=%s\nTAP=lfsfc%s\nHOST_IP=172.30.%s.1\nGUEST_IP=172.30.%s.2\n' "$n" "$n" "$n" "$n" > "$d/vm.env"
cp --sparse=always "$root/rootfs.ext4" "$d/rootfs.ext4"
mac=$(printf '06:00:AC:1E:%02X:02' "$n")
huge='"None"'
[ "$(( $(awk '/^HugePages_Free/ {print $2}' /proc/meminfo) * 2 ))" -ge "$mem" ] && huge='"2M"'
cat > "$d/config.json" <<EOF
{"boot-source":{"kernel_image_path":"$root/vmlinux","boot_args":"console=ttyS0 reboot=k panic=1 pci=off ip=172.30.$n.2::172.30.$n.1:255.255.255.252::eth0:off"},
 "drives":[{"drive_id":"rootfs","path_on_host":"$d/rootfs.ext4","is_root_device":true,"is_read_only":false},
  {"drive_id":"data","path_on_host":"$disk","is_root_device":false,"is_read_only":false}],
 "machine-config":{"vcpu_count":$cpus,"mem_size_mib":$mem,"huge_pages":$huge},
 "network-interfaces":[{"iface_id":"eth0","guest_mac":"$mac","host_dev_name":"lfsfc$n"}]}
EOF
echo "$n""#;

/// Boots the microVM (unless it runs), waits for its sshd (a readiness wait,
/// at most 60 s), and mounts its data disk at `/layerfs/data`.
const BOOT_VM: &str = r#"set -eu
root=$1 d="$1/vms/$2"
. "$d/vm.env"
G="-o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5"
if ! { [ -f "$d/firecracker.pid" ] && kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null; }; then
  ip link show "$TAP" >/dev/null 2>&1 || ip tuntap add dev "$TAP" mode tap
  ip addr replace "$HOST_IP/30" dev "$TAP"
  ip link set "$TAP" up
  setsid nohup "$root/bin/firecracker" --no-api --config-file "$d/config.json" > "$d/firecracker.log" 2>&1 < /dev/null &
  echo $! > "$d/firecracker.pid"
fi
i=0
until ssh -i "$root/id_ed25519" $G "root@$GUEST_IP" true 2>/dev/null; do
  kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null || { echo "firecracker ended: $(tail -n 5 "$d/firecracker.log")" >&2; exit 1; }
  i=$((i+1)); [ "$i" -lt 120 ] || { echo "the microVM did not answer ssh within 60 s" >&2; exit 1; }
  sleep 0.5
done
ssh -i "$root/id_ed25519" $G "root@$GUEST_IP" 'mkdir -p /layerfs/data && { mountpoint -q /layerfs/data || mount -t ext4 /dev/vdb /layerfs/data; } && test "$(findmnt -n -o FSTYPE /layerfs/data)" = ext4'"#;

/// `running|stopped`, then the guest address; exit 3 when there is no such VM.
const INSPECT: &str = r#"d="$1/vms/$2"
test -f "$d/vm.env" || exit 3
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then echo running; else echo stopped; fi
echo "$GUEST_IP""#;

/// Syncs and resets the guest (Firecracker exits on a reset), removes its
/// tap, and with `delete` its folder and data disk.
const STOP: &str = r#"root=$1 d="$1/vms/$2"
test -f "$d/vm.env" || exit 0
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
  timeout 10 ssh -i "$root/id_ed25519" -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR "root@$GUEST_IP" 'sync; reboot -f' >/dev/null 2>&1 || true
  i=0; while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i+1)); done
  kill -0 "$pid" 2>/dev/null && kill -9 "$pid"
fi
rm -f "$d/firecracker.pid"
ip link del "$TAP" 2>/dev/null || true
if [ "${3:-}" = delete ]; then rm -rf "$d" "$1/disks/$2.ext4"; fi
true"#;

fn host_of(spec: &Spec) -> Result<&Spec, MachineError> {
    spec.host.as_deref().ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            "a firecracker machine names its lima host",
        )
    })
}

/// The host's guest, which must run.
fn host_guest(spec: &Spec) -> Result<Guest, MachineError> {
    lima::guest(host_of(spec)?)
}

/// The microVM's guest, through its running host.
pub(crate) fn machine_guest(spec: &Spec) -> Result<Guest, MachineError> {
    Ok(guest(spec, host_guest(spec)?))
}

fn guest(spec: &Spec, host: Guest) -> Guest {
    Guest::Firecracker {
        host: Box::new(host),
        root: ROOT.to_owned(),
        vm: spec.vm_name.clone(),
    }
}

/// The microVM's VMM state and guest address, or `None` when it does not exist.
fn inspect(spec: &Spec, host: &Guest) -> Result<Option<(bool, String)>, MachineError> {
    let out = host.exec(
        &spec.tools,
        &["sh", "-c", INSPECT, "sh", ROOT, &spec.vm_name],
        BOOT,
    )?;
    match out.code {
        Some(3) => Ok(None),
        Some(0) => {
            let mut lines = out.stdout.lines();
            let running = lines.next() == Some("running");
            Ok(Some((running, lines.next().unwrap_or_default().to_owned())))
        }
        _ => Err(MachineError::new(
            Code::Failed,
            format!("reading the microVM failed: {}", out.said()),
        )),
    }
}

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    tool::find(&spec.tools, "ssh")?;
    let host = host_guest(spec)?;
    let lacking = host.script(&spec.tools, "checking the Firecracker host", CHECK, &[ROOT])?;
    let missing: Vec<String> = lacking
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            format!(
                "host:{}:{}",
                host_of(spec).map(|h| h.name.as_str()).unwrap_or(""),
                l.trim()
            )
        })
        .collect();
    if !missing.is_empty() {
        return Err(MachineError::missing(
            "the Firecracker host lacks what a microVM needs (Firecracker's files come from the bundle's firecracker/ folder)",
            missing,
        ));
    }
    if inspect(spec, &host)?.is_some() {
        return Err(super::shared::exists(spec, "the microVM"));
    }
    let disk = format!("{ROOT}/disks/{}.ext4", spec.vm_name);
    let bytes = spec.disk.to_string();
    let made = host.exec(
        &spec.tools,
        &["sh", "-c", DISK, "sh", ROOT, &spec.vm_name, &bytes],
        BOOT,
    )?;
    match made.code {
        Some(0) => {}
        Some(17) => return Err(MachineError::new(Code::Exists, made.said())),
        _ => {
            return Err(MachineError::new(
                Code::Failed,
                format!("making the data disk failed: {}", made.said()),
            ))
        }
    }
    let (cpus, mem) = (spec.cpus.to_string(), (spec.memory >> 20).to_string());
    let want = spec.state.network.unwrap_or(0).to_string();
    let argv = [
        "sh",
        "-c",
        CREATE,
        "sh",
        ROOT,
        &spec.vm_name,
        &disk,
        &cpus,
        &mem,
        &want,
    ];
    let out = host.exec(&spec.tools, &argv, BOOT)?;
    let network = out.stdout.trim().parse::<u8>().ok().filter(|_| out.ok());
    let Some(network) = network else {
        if inspect(spec, &host)?.is_some() {
            return Err(MachineError::new(
                Code::Failed,
                format!("creating the microVM failed: {}", out.said()),
            )
            .kept());
        }
        // nothing was made under the name: the disk this create made goes (one cleanup)
        let _ = host.exec(&spec.tools, &["rm", "-f", &disk], BOOT);
        return Err(MachineError::new(
            Code::Failed,
            format!("creating the microVM failed: {}", out.said()),
        ));
    };
    let lasting = Lasting {
        port: Some(new_port(spec)?),
        network: Some(network),
    };
    kept(boot_and_bring_up(spec, bundle, &host, lasting))
}

/// Boots the microVM unless it runs, opens a new forward, starts the worker.
fn boot_and_bring_up(
    spec: &Spec,
    bundle: &Bundle,
    host: &Guest,
    lasting: Lasting,
) -> Result<Running, MachineError> {
    let out = host.exec(
        &spec.tools,
        &["sh", "-c", BOOT_VM, "sh", ROOT, &spec.vm_name],
        BOOT,
    )?;
    out.answer("booting the microVM")?;
    let Some((_, ip)) = inspect(spec, host)? else {
        return Err(not_found(spec));
    };
    let port = lasting.port.map_or_else(tool::free_port, Ok)?;
    forward::close(spec);
    let Guest::Lima { dir, vm } = host else {
        return Err(MachineError::new(
            Code::InvalidInput,
            "a firecracker host is a lima machine",
        ));
    };
    #[rustfmt::skip]
    let options: Vec<String> = [
        "-F", &dir.join("ssh.config").display().to_string(),
        "-o", "ControlMaster=no", "-o", "ControlPath=none",
    ].map(String::from).to_vec();
    let target = format!("{ip}:{WORKER_PORT}");
    let pid = forward::open(spec, &options, &format!("lima-{vm}"), port, &target)?;
    let endpoint = worker::bring_up(spec, &guest(spec, host.clone()), bundle, port, Some(pid))
        .inspect_err(|_| forward::end(spec, pid, port))?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..lasting
        },
    })
}

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let host = host_guest(spec)?;
    if inspect(spec, &host)?.is_none() {
        return Err(not_found(spec));
    }
    boot_and_bring_up(spec, bundle, &host, spec.state)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    let host = host_guest(spec)?;
    host.script(
        &spec.tools,
        "stopping the microVM",
        STOP,
        &[ROOT, &spec.vm_name],
    )?;
    Ok(())
}

/// The forward, the microVM, its folder and its data disk.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    let host = host_guest(spec)?;
    host.script(
        &spec.tools,
        "removing the microVM",
        STOP,
        &[ROOT, &spec.vm_name, "delete"],
    )?;
    Ok(())
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = tool::lookup(&spec.tools, "limactl")
        .and_then(|p| first_line(tool::command(&p, ["--version"]), false));
    let host = match host_guest(spec) {
        Ok(host) => host,
        Err(error) => {
            let mut status = with_worker(spec, State::Stopped, None, version);
            status.down_reason = Some(format!("its Lima host is down: {}", error.message));
            return Ok(status);
        }
    };
    let kernel = match inspect(spec, &host)? {
        None => State::Missing,
        Some((true, _)) if forward::running(spec) => State::Running,
        Some(_) => State::Stopped,
    };
    let free = (kernel == State::Running)
        .then(|| worker::free_disk(spec, &guest(spec, host)))
        .flatten();
    Ok(with_worker(spec, kernel, free, version))
}
