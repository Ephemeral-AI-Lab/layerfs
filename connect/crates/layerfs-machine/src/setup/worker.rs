//! Setup steps 2 to 4 inside any VM or ssh guest (H §7.1.1): the guest's
//! folders and user-namespace block, the bundle and Sandbox image copied in
//! once, the worker started as root behind a 0600 secret file, and the
//! readiness wait. The adapters make the kernel and its data disk first.
use super::guest::{Guest, EXEC};
use super::tool;
use crate::error::{Code, MachineError};
use crate::facts;
use crate::platforms::firecracker::FIRECRACKER_DIR;
use crate::spec::{Bundle, Endpoint, Spec, BUNDLE, DATA, IMAGE, STATE, WORKER_PORT};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// Hashing the bundle comes first in the worker, so its readiness takes a while.
pub(crate) const READY: Duration = Duration::from_secs(60);

/// The guest's folders on the data disk and, VM-wide, no user namespaces but
/// LayerFS's own: `user.max_user_namespaces=0`, read back. `/proc/sys` is
/// remounted read-write first where a guest mounts it read-only (Apple
/// Container).
const PREPARE: &str = r#"set -e
umask 022
mkdir -p "$1/store" "$1/sandbox" /layerfs/state
chmod 755 "$1" "$1/store" "$1/sandbox"
chmod 700 /layerfs/state
mount -o remount,rw /proc/sys 2>/dev/null || true
echo 0 > /proc/sys/user/max_user_namespaces
cat /proc/sys/user/max_user_namespaces"#;

/// Step 3: ends a worker an earlier start left (by its pid, if that pid's
/// command line is still exactly the bundle's `layerfs worker`: a guest
/// that restarted reuses pids, even this script's own), then starts it
/// detached as root, its settings in its environment and its secret in a file.
/// It serves until `machine.stop` ends it (#535 D11).
const LAUNCH: &str = r#"s=/layerfs/state
if [ -f $s/worker.pid ]; then
  p=$(cat $s/worker.pid)
  # exactly the bundle's worker: a guest that restarted reuses its pids
  if [ "$p" != $$ ] && [ "$(tr '\0' ' ' < /proc/$p/cmdline 2>/dev/null)" = "$2/layerfs worker " ]; then
    kill "$p" 2>/dev/null; i=0
    while kill -0 "$p" 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i+1)); done
  fi
fi
LAYERFS_WORKER_LISTEN="$1" LAYERFS_WORKER_SECRET=$s/secret setsid nohup "$2/layerfs" worker >> $s/worker.log 2>&1 < /dev/null &
echo $! > $s/worker.pid"#;

/// One readiness look: whether the worker still runs, then the guest's TCP sockets.
const LOOK: &str = r#"p=$(cat /layerfs/state/worker.pid 2>/dev/null)
if [ -z "$p" ] || kill -0 "$p" 2>/dev/null; then echo alive; else echo ended; fi
cat /proc/net/tcp /proc/net/tcp6 2>/dev/null; true"#;

/// Step 2 and 3's preparation in a guest whose data disk is at `/layerfs/data`.
pub(crate) fn prepare(spec: &Spec, guest: &Guest) -> Result<(), MachineError> {
    let left = guest.script(&spec.tools, "preparing the guest", PREPARE, &[DATA])?;
    if left.trim() != "0" {
        return Err(MachineError::new(
            Code::Unsupported,
            format!(
                "the guest of {} still allows user namespaces ({left})",
                spec.name
            ),
        ));
    }
    Ok(())
}

/// Step 2: the bundle, copied in once per hash (a changed bundle replaces
/// the copy), then the machine's Sandbox image archive if it is not there.
pub(crate) fn install(spec: &Spec, guest: &Guest, bundle: &Bundle) -> Result<(), MachineError> {
    let tools = &spec.tools;
    let marker = format!("{BUNDLE}/.sealed");
    if guest.read_file(tools, &marker)?.as_deref().map(str::trim) != Some(bundle.hash.as_str()) {
        guest.script(tools, "clearing the old bundle", "rm -rf \"$1\"", &[BUNDLE])?;
        // a nested Lima host alone takes Firecracker's files ([`crate::platforms::lima`])
        guest.copy_in(tools, &bundle.dir, None, Some(FIRECRACKER_DIR), BUNDLE)?;
        guest.write_file(tools, &marker, &format!("{}\n", bundle.hash))?;
    }
    if let Some(image) = &spec.image {
        let (Some(dir), Some(file)) = (image.parent(), image.file_name().and_then(|f| f.to_str()))
        else {
            return Err(MachineError::new(
                Code::InvalidInput,
                format!("the Sandbox image {} names no file", image.display()),
            ));
        };
        let target = format!("{IMAGE}/{file}");
        if guest
            .read_file(tools, &format!("{target}.copied"))?
            .is_none()
        {
            guest.copy_in(tools, dir, Some(file), None, IMAGE)?;
            guest.write_file(tools, &format!("{target}.copied"), "")?;
        }
    }
    Ok(())
}

/// Step 3 and 4: a new secret, the worker started on `bind:7070` in the
/// guest, then the readiness wait at `address` on the host.
pub(crate) fn launch(
    spec: &Spec,
    guest: &Guest,
    bind: Ipv4Addr,
    address: SocketAddr,
    forward_pid: Option<u32>,
) -> Result<Endpoint, MachineError> {
    let tools = &spec.tools;
    let secret = tool::secret()?;
    guest.write_file(tools, &format!("{STATE}/secret"), &secret)?;
    let listen = format!("{bind}:{WORKER_PORT}");
    guest.script(tools, "starting the worker", LAUNCH, &[&listen, BUNDLE])?;
    ready(spec, guest, WORKER_PORT, address)?;
    Ok(Endpoint {
        address,
        secret,
        forward_pid,
    })
}

/// The readiness wait (not a retry): until the worker listens on
/// `guest_port` in the guest and the host's loopback `address` accepts a
/// connection, at most [`READY`]. A worker that ends meanwhile is reported
/// with its log's end.
pub(crate) fn ready(
    spec: &Spec,
    guest: &Guest,
    guest_port: u16,
    address: SocketAddr,
) -> Result<(), MachineError> {
    let deadline = Instant::now() + READY;
    let mut guest_ready = false;
    loop {
        if !guest_ready {
            let out = guest.exec(&spec.tools, &["sh", "-c", LOOK], EXEC)?;
            if out.stdout.starts_with("ended") {
                return Err(MachineError::new(
                    Code::Failed,
                    format!(
                        "the worker of {} ended: {}",
                        spec.name,
                        log_tail(spec, guest)
                    ),
                ));
            }
            guest_ready = out.ok() && facts::listening(&out.stdout, guest_port);
        }
        if guest_ready && TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(MachineError::new(
                Code::Failed,
                format!(
                    "the worker of {} did not answer at {address} within {} s; its log ends: {}",
                    spec.name,
                    READY.as_secs(),
                    log_tail(spec, guest)
                ),
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn log_tail(spec: &Spec, guest: &Guest) -> String {
    let argv = [
        "sh",
        "-c",
        "tail -n 8 /layerfs/state/worker.log 2>/dev/null",
    ];
    guest
        .exec(&spec.tools, &argv, EXEC)
        .map(|out| out.stdout.trim().replace('\n', "; "))
        .unwrap_or_else(|e| e.message)
}

/// Free bytes of the guest's data disk, if `df` answers.
pub(crate) fn free_disk(spec: &Spec, guest: &Guest) -> Option<u64> {
    let out = guest.exec(&spec.tools, &["df", "-kP", DATA], EXEC).ok()?;
    out.ok().then(|| facts::df_free(&out.stdout)).flatten()
}

/// Whether the host reaches a worker at `endpoint` now.
pub(crate) fn answers(endpoint: Option<&crate::spec::Endpoint>) -> bool {
    endpoint.is_some_and(|e| hello(e).unwrap_or(false))
}

/// The worker protocol's first step only: the secret as one frame (a 4-byte
/// big-endian length, then JSON), answered `{"ready":true}` within 2 s; the
/// connection then closes with no request. A bare connection is not enough:
/// an `ssh -L` or Lima port forward accepts on the host while no worker
/// listens in the guest.
fn hello(endpoint: &Endpoint) -> std::io::Result<bool> {
    use std::io::{Read, Write};
    let wait = Some(Duration::from_secs(2));
    let mut stream = TcpStream::connect_timeout(&endpoint.address, Duration::from_secs(1))?;
    stream.set_read_timeout(wait)?;
    stream.set_write_timeout(wait)?;
    let secret = endpoint.secret.replace('\\', "\\\\").replace('"', "\\\"");
    let hello = format!("{{\"secret\":\"{secret}\"}}");
    let mut frame = (hello.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(hello.as_bytes());
    stream.write_all(&frame)?;
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 4096 {
        return Ok(false);
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let text: String = String::from_utf8_lossy(&body)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    Ok(text.contains("\"ready\":true"))
}

/// Steps 2 to 4 for a guest whose kernel runs and whose data disk is mounted.
pub(crate) fn bring_up(
    spec: &Spec,
    guest: &Guest,
    bundle: &Bundle,
    port: u16,
    forward_pid: Option<u32>,
) -> Result<Endpoint, MachineError> {
    prepare(spec, guest)?;
    install(spec, guest, bundle)?;
    launch(
        spec,
        guest,
        spec.kind.worker_bind(),
        loopback(port),
        forward_pid,
    )
}

pub(crate) fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Refuses a bundle folder without the Linux binaries a guest runs.
pub(crate) fn check_bundle(bundle: &Bundle) -> Result<(), MachineError> {
    let missing: Vec<String> = ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"]
        .iter()
        .filter(|name| !Path::new(&bundle.dir).join(name).is_file())
        .map(|name| format!("bundle:{name}"))
        .collect();
    if bundle.hash.is_empty() || !bundle.hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(MachineError::new(
            Code::InvalidInput,
            "the bundle hash is not hex",
        ));
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(MachineError {
        code: Code::InvalidInput,
        message: format!("the bundle {} lacks Linux binaries", bundle.dir.display()),
        missing,
        vm_exists: false,
    })
}
