mod mutate;
mod tree;

pub use mutate::{Mutation, MutationResult};
pub use tree::{Metadata, NodeId, NodeKind, RootHandle, RootId, TreeNode};

pub(crate) use mutate::apply_delta_entry;
