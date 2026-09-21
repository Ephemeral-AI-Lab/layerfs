#![forbid(unsafe_code)]
fn main() -> std::process::ExitCode {
    match layerfs_daemon::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            layerfs_bridge::adapters::native::pipe::diagnostic(&format!("Error: {error}\n"));
            std::process::ExitCode::FAILURE
        }
    }
}
