//! `machine.update`.
use super::list::summary;
use super::spec::machine_of;
use crate::common::context::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::lock;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_platform::HostOs;

/// `machine.update(name, { cpus, memory, image, docker, wsl, daemon })`
/// (O11): the built-in machines' image, Docker socket, WSL distribution
/// and the daemon their Sandboxes run (an absolute path to a file; it
/// replaces what `import-env` recorded). Resizing a VM is refused while
/// Sandboxes run on it.
pub fn update(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let _lock = lock::take(&home)?;
    let record = machine_of(&home, name)?;
    let kind = record.platform()?;
    let resize = args.get("cpus").is_some() || args.get("memory").is_some();
    if kind.built_in() && resize {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in; it has no size of its own"),
        ));
    }
    let built_in_only = ["docker", "wsl", "daemon"];
    if !kind.built_in() && built_in_only.iter().any(|f| args.get(f).is_some()) {
        return Err(refused(
            "InvalidInput",
            "docker, wsl and daemon are the built-in machines' settings",
        ));
    }
    if let Some(daemon) = args.get_text("daemon") {
        let path = std::path::Path::new(daemon);
        if !path.is_absolute() || !path.is_file() {
            return Err(refused(
                "InvalidInput",
                format!("daemon must be the absolute path of a file; {daemon} is not one"),
            ));
        }
    }
    if args.get("wsl").is_some() && HostOs::current() != HostOs::Windows {
        return Err(refused("InvalidInput", "wsl is a Windows host's setting"));
    }
    // a running Sandbox keeps the image and settings it started with
    crate::inventory::can_change_machine(&home, name)?;
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
    for field in ["image", "docker", "wsl", "daemon"] {
        if let Some(value) = args.get(field) {
            record = record.with(field, value.clone());
        }
    }
    record.write(&home)?;
    summary(&home, &record, None)
}
