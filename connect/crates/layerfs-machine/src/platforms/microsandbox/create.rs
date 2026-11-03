//! `microsandbox` create: see `mod.rs`.
use super::observe::{observe, CLI, DEFAULT_IMAGE};
use super::power::boot_and_bring_up;
use crate::error::{kept, Code, MachineError};
use crate::setup::tool;
use crate::setup::vm::{call, checked, data_volume, exists, mib, new_port, CALL, CREATE};
use crate::setup::State;
use crate::spec::{Bundle, Running, Spec, DATA, WORKER_PORT};

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    tool::find(&spec.tools, CLI)?;
    if observe(spec)? != State::Missing {
        return Err(exists(spec, "the microsandbox"));
    }
    let port = new_port(spec)?;
    let volume = data_volume(spec);
    // a volume already under the name is someone's data: never reused or
    // removed by this create's cleanup (one listing; a failed one refuses)
    let listed = call(spec, CLI, &["volume", "list"], CALL)?;
    if !listed.ok() {
        return Err(MachineError::new(
            Code::Failed,
            format!(
                "msb volume list failed, so the data volume {volume} could not be checked: {}",
                listed.said()
            ),
        ));
    }
    if listed.stdout.split_whitespace().any(|word| word == volume) {
        return Err(MachineError::new(
            Code::Exists,
            format!("the volume {volume} already exists; it is left alone"),
        ));
    }
    let cpus = spec.cpus.to_string();
    let memory = mib(spec.memory);
    let mount = format!("{volume}:{DATA}:kind=disk,size={}", mib(spec.disk));
    let publish = format!("127.0.0.1:{port}:{WORKER_PORT}");
    let image = spec.os_image.as_deref().unwrap_or(DEFAULT_IMAGE);
    // `--net public,host` lets the guest reach the host (a PostgreSQL Store's
    // server on its loopback, G7) as well as the internet; msb denies the host
    // by default and cannot allow it after create
    #[rustfmt::skip]
    let args = [
        "create", "--name", &spec.vm_name, "--cpus", &cpus, "--memory", &memory,
        "--mount-named", &mount, "-p", &publish, "--net", "public,host",
        "--label", "layerfs.role=machine", image,
    ];
    let made = call(spec, CLI, &args, CREATE)?;
    if !made.ok() {
        if observe(spec)? != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("msb create failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: a volume it may have made goes (one cleanup)
        let _ = call(spec, CLI, &["volume", "remove", &volume], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("msb create failed: {}", made.said()),
        ));
    }
    kept(boot_and_bring_up(spec, bundle, port))
}

/// The sandbox record, then its data volume.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? != State::Missing {
        checked(spec, CLI, &["remove", "--force", &spec.vm_name], CREATE)?;
    }
    let out = call(spec, CLI, &["volume", "remove", &data_volume(spec)], CALL)?;
    if out.ok() || out.said().to_lowercase().contains("not found") {
        return Ok(());
    }
    Err(MachineError::new(
        Code::Failed,
        format!("msb volume remove failed: {}", out.said()),
    ))
}
