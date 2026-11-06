//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs `layerfs-machine`'s call with
//! those reads from [`crate::inventory`]. The other machine calls are
//! `layerfs-machine`'s own.
use crate::inventory;
use layerfs_bridge::control::SdkValue;
use layerfs_home::context::Context;
use layerfs_home::outcome::Outcome;
use layerfs_machine::Related;

pub use layerfs_machine::platforms;

/// What a machine holds, read from the Store and Sandbox records.
pub const RELATED: Related = Related {
    stores_on: inventory::stores_on,
    sandboxes_on: inventory::sandboxes_on,
    can_remove: inventory::can_remove_machine,
    can_stop: inventory::can_stop_machine,
    can_change: inventory::can_change_machine,
};

/// `machine.list`.
pub fn list(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::list(context, args, RELATED)
}

/// `machine.get`.
pub fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::get(context, args, RELATED)
}

/// `machine.status`.
pub fn status(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::status(context, args, RELATED)
}

/// `machine.start`.
pub fn start(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::start(context, args, RELATED)
}

/// `machine.stop`.
pub fn stop(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::stop(context, args, RELATED)
}

/// `machine.create`.
pub fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::create(context, args, RELATED)
}

/// `machine.connect`.
pub fn connect(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::connect(context, args, RELATED)
}

/// `machine.remove`.
pub fn remove(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::remove(context, args, RELATED)
}

/// `machine.disconnect`.
pub fn disconnect(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::disconnect(context, args, RELATED)
}

/// `machine.update`.
pub fn update(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::update(context, args, RELATED)
}
