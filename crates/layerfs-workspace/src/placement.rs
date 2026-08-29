use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspacePlacement {
    Host {
        root: PathBuf,
    },
    Container {
        container_id: ContainerId,
        root: PathBuf,
    },
}

impl WorkspacePlacement {
    pub(crate) fn root(&self) -> &PathBuf {
        match self {
            Self::Host { root } | Self::Container { root, .. } => root,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceProjection {
    Fuse,
    Materialize,
}
