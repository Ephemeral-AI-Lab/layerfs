//! The `backup` calls, with the Store checks `backup.restore` makes
//! (layerfs-project#105).

/// The `backup` family.
pub mod family {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::context::Context;
    use layerfs_home::outcome::Outcome;

    /// Runs one `backup` call.
    pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
        layerfs_store::backup::family::run(context, call, args, crate::store::RELATED)
    }
}
