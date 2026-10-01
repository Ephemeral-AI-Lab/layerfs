#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[allow(unsafe_code)]
fn main() -> std::process::ExitCode {
    // SAFETY: this is the process entry, before host configuration can open
    // Store/history or initialize workers. This binary makes no earlier SQLite
    // calls and owns its linked provider's startup/configuration exclusively.
    let engine_guard = match unsafe { layerfs_storage::engine::bootstrap_exclusive() } {
        Ok(guard) => guard,
        Err(error) => {
            let custody = error.custody();
            layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
                "Error: SQLite bootstrap refused: stage={:?}: {}; hard_limit_set_attempted: {}; hard_limit_installed: {}; observed_hard_heap_limit: {:?}\n",
                error.stage(), error.cause(), custody.hard_limit_set_attempted,
                custody.hard_limit_installed, custody.observed_hard_heap_limit,
            ));
            return std::process::ExitCode::FAILURE;
        }
    };
    match layerfs_server::host::run_guarded(engine_guard) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            layerfs_bridge::adapters::native::pipe::diagnostic(&format!("Error: {error}\n"));
            std::process::ExitCode::FAILURE
        }
    }
}
