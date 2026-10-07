//! Original point/operation observations with fixed output and no product hooks.
use super::allocation;
use layerfs_daemon::{bootstrap::OpenedStore, Owner};
use std::{
    fs::File,
    io::{BufRead, Write},
};
pub fn phase(name: &str) {
    println!("ENGINE_PHASE {name} pid={}", std::process::id());
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    assert!(std::io::stdin().lock().read_line(&mut line).unwrap() > 0);
    assert_eq!(line, "continue\n");
}
pub fn point(out: &mut File, name: &str, index: usize, opened: &OpenedStore, owner: &Owner) {
    let local = owner.client().diagnostics().unwrap();
    let cache = opened.store.cache_work().unwrap();
    let (live, allocated, freed) = allocation::snapshot();
    writeln!(out,"{{\"schema\":\"pre-s8-growth-point-v1\",\"stage\":\"{name}\",\"index\":{index},\"rust_live_requested\":{live},\"rust_allocated\":{allocated},\"rust_freed\":{freed},\"cache_bytes\":{},\"cache_objects\":{},\"read_handles\":{},\"credited_bytes\":{},\"outstanding\":{},\"queued\":{},\"receipt_overruns\":{},\"closed_namespaces\":{},\"maintenance_rows\":{},\"maintenance_bytes\":{}}}",cache.charged_cache_bytes,cache.cached_objects,opened.store.read_handles(),local.credited_bytes,local.outstanding,local.queued,local.receipt_overruns,local.closed_namespaces,local.maintenance_rows,local.maintenance_data_bytes).unwrap();
    assert_eq!(local.outstanding, 0);
    assert_eq!(local.credited_bytes, 0);
    assert_eq!(local.receipt_overruns, 0);
    assert_eq!(opened.store.read_handles(), 4);
    assert!(cache.charged_cache_bytes <= 8 * 1024 * 1024);
}
pub fn terminal(out: &mut File) {
    let (live, allocated, freed) = allocation::snapshot();
    writeln!(out,"{{\"schema\":\"pre-s8-growth-terminal-v1\",\"rust_live_requested\":{live},\"rust_allocated\":{allocated},\"rust_freed\":{freed},\"owner_stopped\":true,\"store_released\":true}}").unwrap();
}
