//! The `project` calls, with the check `project.remove` makes
//! (layerfs-project#105).

/// The `project` family.
pub mod family {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::context::Context;
    use layerfs_home::outcome::Outcome;

    /// Runs one `project` call.
    pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
        layerfs_store::project::family::run(context, call, args, crate::store::RELATED)
    }
}
