#[allow(dead_code)]
#[path = "../support/verify/adversary.rs"]
mod adversary;
#[allow(dead_code)]
#[path = "../support/fault/base_file_sweep.rs"]
mod base_file_sweep;
mod base_files_drop;
mod base_files_kill;
mod directories;
mod drop;
#[allow(dead_code)]
#[path = "../support/fault/proxy.rs"]
mod fault_proxy;
#[allow(dead_code)]
#[path = "../support/fault/sweep.rs"]
mod fault_sweep;
mod first_drop;
mod first_kill;
mod kill;
mod race_custody;
mod race_drop;
mod race_kill;
mod similar_drop;
mod similar_kill;
