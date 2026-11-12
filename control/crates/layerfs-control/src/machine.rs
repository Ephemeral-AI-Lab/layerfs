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

/// Explicit crash stop of a machine and this home's Sandboxes, preserving data.
pub fn stop_force(home: &Path, name: &MachineName) -> Outcome {
    crate::machine_force::stop(home, name)
}
