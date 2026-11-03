//! `machine.update`.
use super::list::summary;
use super::spec::{kind_of, machine_of, sandboxes_on};
use crate::common::context::Context;
use crate::common::home::{lock, paths, with_field, write_record};
use crate::common::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;
use layerfs_machine::HostOs;

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
