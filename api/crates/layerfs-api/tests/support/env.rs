//! The process environment, which every test thread of a joined test binary
//! shares (one binary per area since #828). A test that reads or changes it
//! (PATH, HOME, tool fallbacks, bundle and cache settings, log settings) holds
//! the turn, and the turn puts the environment back as it found it, so each
//! test starts from the environment the binary started with, as it did in its
//! own binary.
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};

/// One test's turn on the environment; dropping it restores the environment.
pub struct EnvTurn {
    saved: Vec<(OsString, OsString)>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvTurn {
    fn drop(&mut self) {
        for (name, _) in std::env::vars_os() {
            if !self.saved.iter().any(|(saved, _)| *saved == name) {
                std::env::remove_var(name);
            }
        }
        for (name, value) in &self.saved {
            std::env::set_var(name, value);
        }
    }
}

/// Waits for the environment, then takes it as it is now.
pub fn env_lock() -> EnvTurn {
    static ENV: Mutex<()> = Mutex::new(());
    let lock = ENV.lock().unwrap_or_else(|poison| poison.into_inner());
    EnvTurn {
        saved: std::env::vars_os().collect(),
        _lock: lock,
    }
}
