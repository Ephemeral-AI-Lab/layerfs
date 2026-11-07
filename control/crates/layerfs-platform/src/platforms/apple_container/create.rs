//! `apple_container` create: see `mod.rs`.
use super::observe::{host_check, observe, CAPABILITIES, CLI, DEFAULT_IMAGE, MAIN};
use super::power::bring_up;
use crate::error::{kept, Code, MachineError};
use crate::setup::vm::{call, checked, data_volume, exists, mib, new_port, CALL, CREATE};
use crate::setup::State;
use crate::spec::{Bundle, Running, Spec, DATA, WORKER_PORT};

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    host_check(spec)?;
    if observe(spec)? != State::Missing {
        return Err(exists(spec, "the container"));
    }
    let volume = data_volume(spec);
    if call(spec, CLI, &["volume", "inspect", &volume], CALL)?.ok() {
        return Err(MachineError::new(
            Code::Exists,
            format!("the volume {volume} already exists; it is left alone"),
        ));
    }
    let port = new_port(spec)?;
    checked(
        spec,
        CLI,
        &["volume", "create", "-s", &mib(spec.disk), &volume],
        CREATE,
    )?;
    let cpus = spec.cpus.to_string();
    let memory = mib(spec.memory);
    let mount = format!("type=volume,source={volume},target={DATA}");
    let publish = format!("127.0.0.1:{port}:{WORKER_PORT}");
    let image = spec.os_image.as_deref().unwrap_or(DEFAULT_IMAGE);
    let mut args = vec![
        "run",
        "--detach",
        "--name",
        &spec.vm_name,
        "--cpus",
        &cpus,
        "--memory",
        &memory,
    ];
    args.extend(CAPABILITIES);
    args.extend([
        "--mount",
        &mount,
        "--publish",
        &publish,
        "--label",
        "layerfs.role=machine",
    ]);
    args.extend([image, "sh", "-c", MAIN]);
    let made = call(spec, CLI, &args, CREATE)?;
    if !made.ok() {
        if observe(spec)? != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("container run failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: the volume this create made goes (one cleanup)
        let _ = call(spec, CLI, &["volume", "delete", &volume], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("container run failed: {}", made.said()),
        ));
    }
    kept(bring_up(spec, bundle, port))
}

/// The container, then its data volume.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? != State::Missing {
        checked(spec, CLI, &["delete", "--force", &spec.vm_name], CREATE)?;
    }
    let volume = data_volume(spec);
    if call(spec, CLI, &["volume", "inspect", &volume], CALL)?.ok() {
        checked(spec, CLI, &["volume", "delete", &volume], CALL)?;
    }
    Ok(())
}
