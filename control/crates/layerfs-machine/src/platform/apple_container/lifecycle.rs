//! `apple_container` create: see `mod.rs`.
use super::status::{host_check, observe, CAPABILITIES, CLI, DEFAULT_IMAGE, MAIN};
use crate::platform::error::{kept, Code, MachineError};
use crate::setup::vm::{call, checked, data_volume, exists, mib, new_port, CALL, CREATE};
use crate::setup::State;
use crate::setup::spec::{Bundle, Running, Spec, DATA, WORKER_PORT};

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

// `apple_container` power: see `mod.rs`.
use super::status::{gateway, guest};
use crate::setup::vm::{not_found, recorded_port};
use crate::setup::worker;
use crate::setup::spec::Lasting;

/// Starts a stopped container, and tries a failed one once: `container
/// start` runs its main process again over the same volume, and the
/// documented fix of a `failed` machine is `machine start`; the CLI's own
/// answer decides. Lima refuses its `Broken` before any call, because
/// `limactl start` refuses such an instance itself (`lima/power.rs`).
pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    match observe(spec)? {
        State::Missing => return Err(not_found(spec)),
        State::Running => {}
        State::Stopped | State::Failed => {
            checked(spec, CLI, &["start", &spec.vm_name], CREATE)?;
        }
    }
    bring_up(spec, bundle, port)
}

/// `container stop` keeps the container and its volume. A forced stop gives
/// it no grace (`--time 0`, as docker's `-t 0`) and also stops a failed
/// container or one between states (`Unavailable`), which an ordinary stop
/// leaves alone.
pub(crate) fn stop(spec: &Spec, force: bool) -> Result<(), MachineError> {
    let stoppable = match observe(spec) {
        Ok(state) => state == State::Running || (force && state == State::Failed),
        Err(passing) if force && passing.code == Code::Unavailable => true,
        Err(error) => return Err(error),
    };
    if stoppable {
        let mut words = vec!["stop"];
        if force {
            words.extend(["--time", "0"]);
        }
        words.push(&spec.vm_name);
        checked(spec, CLI, &words, CREATE)?;
    }
    Ok(())
}

/// Brings the worker up and records the container network's gateway, read
/// once here (#66): every start replaces it, and a Sandbox's PostgreSQL
/// reach uses it without running `container inspect`. A gateway this start
/// could not read is recorded as none; the reach then looks it up itself,
/// and refuses as before when the machine reports none.
pub(super) fn bring_up(spec: &Spec, bundle: &Bundle, port: u16) -> Result<Running, MachineError> {
    let endpoint = worker::bring_up(spec, &guest(spec), bundle, port)?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            gateway: gateway(spec).ok().and_then(|g| g.parse().ok()),
        },
    })
}
