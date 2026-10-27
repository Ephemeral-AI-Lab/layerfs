//! Production filesystem daemon entry; command execution belongs to the runtime.
fn main() -> std::process::ExitCode {
    layerfs_daemon::application::main()
}
