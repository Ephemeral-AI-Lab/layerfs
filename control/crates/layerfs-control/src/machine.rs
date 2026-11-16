//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs `layerfs-machine`'s call with
//! those reads from [`crate::inventory`]. The other machine calls are
//! `layerfs-machine`'s own.
use crate::inventory;
use layerfs_home::events::Events;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_machine::{MachineCreate, MachineUpdate, Related};
use std::path::Path;

/// What a machine holds, read from the Store and Sandbox records.
pub const RELATED: Related = Related {
    stores_on: inventory::stores_on,
    postgresql_on: inventory::postgresql_on,
    sandboxes_on: inventory::sandboxes_on,
    can_remove: inventory::can_remove_machine,
    can_stop: inventory::can_stop_machine,
    can_change: inventory::can_change_machine,
};

/// `machine.list`.
pub fn list(home: &Path) -> Outcome {
    layerfs_machine::list(home, RELATED)
}

/// `machine.get`.
pub fn get(home: &Path, name: &MachineName) -> Outcome {
    layerfs_machine::get(home, name, RELATED)
}

/// `machine.status`.
pub fn status(home: &Path, name: &MachineName) -> Outcome {
    layerfs_machine::status(home, name, RELATED)
}

/// `machine.start`.
pub fn start(home: &Path, name: &MachineName) -> Outcome {
    layerfs_machine::start(home, name, RELATED)
}

/// `machine.start` with stage progress.
pub fn start_with_progress(home: &Path, events: &mut Events<'_>, name: &MachineName) -> Outcome {
    layerfs_machine::start_with_progress(home, name, RELATED, events)
}

/// `machine.stop`.
pub fn stop(home: &Path, name: &MachineName) -> Outcome {
    layerfs_machine::stop(home, name, RELATED)
}

/// `machine.create`.
pub fn create(home: &Path, events: &mut Events<'_>, request: &MachineCreate) -> Outcome {
    layerfs_machine::create(home, events, request, RELATED)
}

/// `machine.remove`.
pub fn remove(home: &Path, name: &MachineName) -> Outcome {
    layerfs_machine::remove(home, name, RELATED)
}

/// `machine.update`.
pub fn update(home: &Path, request: &MachineUpdate) -> Outcome {
    layerfs_machine::update(home, request, RELATED)
}

/// `machine.images`: the Sandbox images on this machine, and the host's
/// Docker images it can copy.
#[cfg(unix)]
pub fn images(home: &Path, name: &MachineName) -> Outcome {
    layerfs_sandbox::images::list(home, name)
}

/// machine.load: copies an image to this machine without a Sandbox.
#[cfg(unix)]
pub fn load(home: &Path, events: &mut Events<'_>, name: &MachineName, image: &str) -> Outcome {
    layerfs_sandbox::image_load::machine_load(home, events, name, image)
}

/// machine.prune: removes one image, or every unused one.
#[cfg(unix)]
pub fn prune(home: &Path, name: &MachineName, image: Option<&str>, unused: bool) -> Outcome {
    layerfs_sandbox::images::prune(home, name, image, unused)
}

// Windows: the Docker image face is the unix runtime; refused until its own port (#454).
#[cfg(windows)]
/// Windows refuses the machine Docker image face until its own port (#454).
pub fn images(_home: &Path, _name: &MachineName) -> Outcome {
    crate::machine_images_not_ported()
}
#[cfg(windows)]
/// Windows refuses the machine Docker image face until its own port (#454).
pub fn load(_home: &Path, _events: &mut Events<'_>, _name: &MachineName, _image: &str) -> Outcome {
    crate::machine_images_not_ported()
}
#[cfg(windows)]
/// Windows refuses the machine Docker image face until its own port (#454).
pub fn prune(_home: &Path, _name: &MachineName, _image: Option<&str>, _unused: bool) -> Outcome {
    crate::machine_images_not_ported()
}

/// Explicit crash stop of a machine and this home's Sandboxes, preserving data.
#[cfg(unix)]
pub fn stop_force(home: &Path, name: &MachineName) -> Outcome {
    crate::machine_force::stop(home, name)
}

#[cfg(windows)]
/// Windows refuses the crash-stop face until its own port (#454).
pub fn stop_force(_home: &Path, _name: &MachineName) -> Outcome {
    crate::machine_images_not_ported()
}
