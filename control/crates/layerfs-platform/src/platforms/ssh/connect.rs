//! `ssh` connect: see `mod.rs`.
use super::host_key::{accept_key, accepted};
use super::observe::{dest, guest, ssh_of, END_WORKER};
use super::power::bring_up;
use crate::error::{Code, MachineError};
use crate::setup::forward;
use crate::setup::guest::EXEC;
use crate::spec::{Bundle, Running, Spec, BUNDLE, STATE};

/// Checks the host and starts its worker; nothing on the host is created but
/// `/layerfs/bundle`, `/layerfs/state` and the data root's folders.
pub(crate) fn connect(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let known = accept_key(spec)?;
    let guest = guest(spec, &known)?;
    let kernel = guest.exec(&spec.tools, &["uname", "-s"], EXEC)?;
    if !kernel.ok() {
        return Err(MachineError::new(
            Code::Unavailable,
            format!(
                "ssh to {} failed ({}): {}",
                dest(ssh_of(spec)?),
                kernel.exit(),
                kernel.said()
            ),
        ));
    }
    if kernel.stdout.trim() != "Linux" {
        return Err(MachineError::new(
            Code::Unsupported,
            format!(
                "{} answers {:?}, not Linux",
                ssh_of(spec)?.host,
                kernel.stdout.trim()
            ),
        ));
    }
    bring_up(spec, bundle, &known)
}

/// `machine disconnect`: the worker and forward stop, and what connect wrote
/// goes; the data root never does.
pub(crate) fn disconnect(spec: &Spec) -> Result<(), MachineError> {
    forward::close(spec);
    let guest = guest(spec, &accepted(spec))?;
    guest.script(&spec.tools, "stopping the worker", END_WORKER, &[])?;
    guest.script(
        &spec.tools,
        "removing LayerFS's folders",
        FORGET,
        &[BUNDLE, STATE, &ssh_of(spec)?.data_root],
    )?;
    Ok(())
}

/// What connect wrote, never the data root; a bind mount of it is undone.
const FORGET: &str = r#"rm -rf "$1" "$2"
if [ "$3" != /layerfs/data ] && mountpoint -q /layerfs/data; then umount /layerfs/data; fi; exit 0"#;
