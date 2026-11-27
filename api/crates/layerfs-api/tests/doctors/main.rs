mod home_machine;
mod sandbox;
mod targets;
mod unreadable_record;

/// The process environment, which every test thread of this binary shares
/// (one binary since #828): a test that changes it holds this lock.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV.lock().unwrap_or_else(|poison| poison.into_inner())
}
