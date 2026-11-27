//! What a machine is doing: its state and, for the host machine, its worker.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod worker;
