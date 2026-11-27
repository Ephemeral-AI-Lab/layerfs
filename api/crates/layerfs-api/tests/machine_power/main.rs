mod docker_default_image;
mod force;
mod force_native;
mod no_start;
mod start_stop;
mod take_down;
mod worker_dial;

/// The process environment, which every test thread of this binary shares
/// (one binary since #828): a test that changes it holds this lock.
#[cfg(feature = "machine")]
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV.lock().unwrap_or_else(|poison| poison.into_inner())
}
