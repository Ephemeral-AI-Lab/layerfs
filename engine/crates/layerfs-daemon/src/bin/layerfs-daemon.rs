//! Production filesystem daemon entry; command execution belongs to the runtime.

/// The build this binary came from, after the marker
/// `layerfs-build-identity:` that `layerfs home get` looks for in the file.
#[used]
static BUILD_IDENTITY: &[u8] = concat!(
    "layerfs-build-identity:",
    env!("LAYERFS_DAEMON_BUILD_IDENTITY")
)
.as_bytes();

fn main() -> std::process::ExitCode {
    layerfs_daemon::application::main()
}
