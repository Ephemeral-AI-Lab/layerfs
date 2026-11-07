//! A host operation, typed (layerfs-project#108): the request both sides
//! build and run (`call`) and its values at the edge (`frame`: the
//! `sandbox.host` frame and the one args map).
mod call;
mod frame;

pub use call::{Call, Deploy, Host, Message, Op, OPS};
pub use frame::{decode, encode, Sent};
