use super::*;

include!("projection/read.rs");
include!("projection/create.rs");
include!("projection/replace.rs");
include!("projection/namespace.rs");
include!("projection/finalize.rs");

impl ProjectionWorkspace for Workspace {
    projection_read_methods!();
    projection_create_methods!();
    projection_replace_methods!();
    projection_namespace_methods!();
    projection_finalize_methods!();
}
