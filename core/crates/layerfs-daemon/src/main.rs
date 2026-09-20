#![forbid(unsafe_code)]
mod config;
mod headless;
mod run;
fn main() -> std::process::ExitCode {
    match run::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            layerfs_bridge::adapters::native::pipe::diagnostic(&format!("Error: {error}\n"));
            std::process::ExitCode::FAILURE
        }
    }
}
