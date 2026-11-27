mod family;
mod host_names;
mod nameless_records;
mod platforms_setup;
mod provider_names;
mod refusal_order;
mod removed_platforms;

/// The process environment, which every test thread of this binary shares
/// (one binary since #828): a test that changes it holds this lock.
#[cfg(feature = "machine")]
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV.lock().unwrap_or_else(|poison| poison.into_inner())
}
