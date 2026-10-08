//! Private serial state over the existing caller-owned raw construction port.
mod codec;
mod initial;
mod roots;
mod store;
mod types;

pub(crate) use codec::{change, key, serial, QUALIFY_CONTEXT, QUALIFY_INODE, QUALIFY_QUEUE};
pub(crate) use codec::{
    TOPOLOGY_PLACED, TOPOLOGY_QUEUE, TOPOLOGY_ROOTED, TOPOLOGY_SCANNED, TOPOLOGY_TERRITORY,
};
pub(crate) use initial::InitialRows;
pub(crate) use roots::RebuiltRoots;
pub(crate) use store::SerialState;
pub(crate) use types::DroppedParents;
