//! The Lima host side of Firecracker: where its files live on the host and
//! how they get there, and how a command reaches a microVM through it.
use crate::error::MachineError;
use crate::setup::guest::Guest;
use crate::spec::{Bundle, Spec};

/// Where a nested machine keeps Firecracker's files and microVMs.
pub(crate) const FIRECRACKER_ROOT: &str = "/layerfs/data/firecracker";

/// The bundle folder of Firecracker's files: copied to a nested Lima host's
/// data disk only, never into a guest's bundle copy.
pub(crate) const FIRECRACKER_DIR: &str = "firecracker";

/// The script in a Firecracker host that runs `$@` in one microVM as root.
pub(crate) const FIRECRACKER_GUEST: &str = r#"d="$1/vms/$2"; key="$1/id_ed25519"; shift 2
. "$d/vm.env"
exec ssh -i "$key" -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5 "root@$GUEST_IP" -- "$@""#;

/// A nested machine's Firecracker files: the bundle's `firecracker/`
/// folder, copied from the host straight to the data disk once per bundle
/// hash. A bundle without it copies nothing; a microVM's create then names
/// each missing file.
pub(crate) fn firecracker_files(
    spec: &Spec,
    guest: &Guest,
    bundle: &Bundle,
) -> Result<(), MachineError> {
    let from = bundle.dir.join(FIRECRACKER_DIR);
    let marker = format!("{FIRECRACKER_ROOT}/.sealed");
    if !from.is_dir()
        || guest
            .read_file(&spec.tools, &marker)?
            .as_deref()
            .map(str::trim)
            == Some(bundle.hash.as_str())
    {
        return Ok(());
    }
    guest.copy_in(&spec.tools, &from, None, None, FIRECRACKER_ROOT)?;
    guest.write_file(&spec.tools, &marker, &format!("{}\n", bundle.hash))
}
