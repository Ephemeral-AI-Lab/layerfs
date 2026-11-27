mod files;
mod machine_files;
mod root_files;
mod run_root_layout;
mod space;

/// The inspect permits and helpers are the process's, which every module of
/// this binary shares (one binary since #828): a test that takes a permit or
/// waits for the helpers holds this turn.
fn permits() -> std::sync::MutexGuard<'static, ()> {
    static PERMITS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PERMITS.lock().unwrap_or_else(|poison| poison.into_inner())
}
