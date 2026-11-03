//! `firecracker` create: see `mod.rs`.
use super::observe::{host_guest, host_of, inspect, ROOT};
use super::power::boot_and_bring_up;
use super::scripts::{CHECK, CREATE, DISK, STOP};
use crate::error::{kept, Code, MachineError};
use crate::setup::forward;
use crate::setup::tool;
use crate::setup::vm::{new_port, BOOT};
use crate::spec::{Bundle, Lasting, Running, Spec};

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
        return Err(crate::setup::vm::exists(spec, "the microVM"));
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
