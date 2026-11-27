mod files;
mod images;
mod list_live;
mod logs;
mod sandbox_files;
mod stats;

/// The inspect permits, PATH and tool stand-ins are the process's, which every
/// module of this binary shares (one binary since #828): a test holds this
/// turn while it uses them.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static PROCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    PROCESS.lock().unwrap_or_else(|poison| poison.into_inner())
}
