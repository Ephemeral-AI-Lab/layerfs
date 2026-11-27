//! The ten call families, one folder each and one file per call (rules 8,
//! 10); each family's `mod.rs` lists its calls.
mod backup;
mod branch;
mod commit;
mod file;
pub(crate) mod home;
mod machine;
mod project;
mod sandbox;
mod store;
mod workspace;

use crate::dispatch::{Families, Family};

/// The ten families, in the order help lists them, the same from the
/// `layerfs` binary, Node and Python.
pub fn families() -> Families {
    &[
        Family {
            name: "home",
            list: home::calls,
        },
        Family {
            name: "project",
            list: project::calls,
        },
        Family {
            name: "branch",
            list: branch::calls,
        },
        Family {
            name: "commit",
            list: commit::calls,
        },
        Family {
            name: "file",
            list: file::calls,
        },
        Family {
            name: "sandbox",
            list: sandbox::calls,
        },
        Family {
            name: "workspace",
            list: workspace::calls,
        },
        Family {
            name: "machine",
            list: machine::calls,
        },
        Family {
            name: "store",
            list: store::calls,
        },
        Family {
            name: "backup",
            list: backup::calls,
        },
    ]
}
