//! Production filesystem daemon entry; command execution belongs to the runtime.
// The daemon is the unix daemon (FUSE serving); Windows reaches Stores
// in-process (#352 F-020) and has no daemon binary - this stub exits with
// the same refusal instead of building an empty crate.
#[cfg(unix)]
fn main() -> std::process::ExitCode {
    layerfs_daemon::application::main()
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("the layerfs daemon is a unix daemon; Windows reaches Stores in-process");
    std::process::ExitCode::FAILURE
}
