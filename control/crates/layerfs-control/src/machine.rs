//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs `layerfs-machine`'s call with
//! those reads from [`crate::inventory`]. The other machine calls are
//! `layerfs-machine`'s own.
use crate::inventory;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::outcome::Outcome;
use layerfs_machine::Related;
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
pub fn list(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::list(home, args, RELATED)
}

/// `machine.get`.
pub fn get(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::get(home, args, RELATED)
}

/// `machine.status`.
pub fn status(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::status(home, args, RELATED)
}

/// `machine.start`.
pub fn start(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::start(home, args, RELATED)
}

/// `machine.stop`.
pub fn stop(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::stop(home, args, RELATED)
}

/// `machine.create`.
pub fn create(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::create(home, events, args, RELATED)
}

/// `machine.connect`.
pub fn connect(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::connect(home, args, RELATED)
}

/// `machine.remove`.
pub fn remove(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::remove(home, args, RELATED)
}

/// `machine.disconnect`.
pub fn disconnect(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::disconnect(home, args, RELATED)
}

/// `machine.update`.
pub fn update(home: &Path, args: &SdkValue) -> Outcome {
    layerfs_machine::update(home, args, RELATED)
}
