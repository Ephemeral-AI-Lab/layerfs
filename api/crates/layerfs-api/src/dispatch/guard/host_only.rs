//! A machine call in the guest worker's build (no `machine` feature): no
//! machine adapter runs inside a machine, so each call is refused
//! `Unsupported` before any effect, naming itself ("machine.stop runs on
//! the host").

/// The `run` of machine call `$call` in the guest worker's build.
macro_rules! host_only {
    ($call:literal) => {
        #[cfg(not(feature = "machine"))]
        fn run(
            _context: &mut crate::dispatch::Context<'_>,
            _args: &layerfs_protocol::sdk::SdkValue,
        ) -> layerfs_home::reply::outcome::Outcome {
            Err(layerfs_home::reply::outcome::refused(
                "Unsupported",
                concat!("machine.", $call, " runs on the host"),
            ))
        }
    };
}
pub(crate) use host_only;
