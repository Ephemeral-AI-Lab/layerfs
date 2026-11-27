//! The host worker's entries in the open-Store cache ([`super::cache`]):
//! admission from the real home under the record lock, its own slot cap
//! ([`HOST_STORES`]) and its fenced close.
use super::cache::*;
use super::close::{finish_close, finish_idle, in_use, start_close, CloseReport};
use crate::admin::open::args::OpenArgs;
use crate::admin::open::lease::{self};
use crate::admin::open::service::{Route, Slot};
use crate::admin::record::store::{self, StoreRecord, StoreRef};
use layerfs_home::home::paths;
use layerfs_home::outcome::{refused, with_next, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

pub(super) const HOST_STORES: usize = 16;

/// Runs one host worker method from the real home. The worker takes its own
/// lease and validates the Store under the same lock as maintenance on
/// every admission, including a hit. Routing and the method allowlist are
/// checked by the caller before it invokes this.
pub fn run_host(
    home: &Path,
    store: &str,
    project: &str,
    method: &str,
    fields: SdkValue,
) -> Outcome {
    let SdkValue::Map(mut fields) = fields else {
        return Err(refused(
            "InvalidInput",
            "a Store call needs an argument map",
        ));
    };
    let (open, running) = enter_host(home, store, project)?;
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project)));
    call(method, &open, SdkValue::Map(fields), running)
}

/// [`run_host`] for a write whose content or changes `source` streams.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn run_host_streamed(
    home: &Path,
    store: &str,
    project: &str,
    method: &str,
    fields: SdkValue,
    source: &mut dyn layerfs_store_service::methods::Source,
) -> Outcome {
    let SdkValue::Map(mut fields) = fields else {
        return Err(refused(
            "InvalidInput",
            "a Store call needs an argument map",
        ));
    };
    let (open, running) = enter_host(home, store, project)?;
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project)));
    layerfs_home::trace::step("store_open");
    let outcome = crate::admin::open::service::call_streamed(
        method,
        &open,
        SdkValue::Map(fields),
        running.owner,
        source,
    );
    layerfs_home::trace::step("call");
    drop(running);
    outcome
}

/// Number of host cache slots in use, including opens and closes in progress.
pub fn host_store_count(home: &Path) -> usize {
    count_host(&cache(), home)
}

pub(super) fn count_host(open: &Cache, home: &Path) -> usize {
    open.values()
        .filter(|slot| slot.host.as_deref() == Some(home))
        .count()
}

pub(super) fn enter_host(
    home: &Path,
    store: &str,
    project: &str,
) -> Result<(OpenArgs, Running), SdkRefusal> {
    let path = paths::store_file(home, store);
    crate::admin::record::identity::store_dir(home, store)?;
    loop {
        let record_lock = store::lock(home, store)?;
        let record = StoreRecord::of(home, store)?;
        record.unblocked()?;
        if record.op().is_some_and(|op| op.name() == "store.compact") {
            record.unheld()?;
        }
        let StoreRef::Sqlite { machine, .. } = record.store_ref()? else {
            return Err(refused(
                "Unsupported",
                "host worker Store calls require local SQLite",
            ));
        };
        layerfs_machine::record::MachineRecord::admit_host(home, machine)?;
        if layerfs_machine::in_process() != Some(machine) {
            return Err(refused(
                "Unavailable",
                format!("Store {store} is placed on {machine}"),
            ));
        }
        let mut open = cache();
        if let Some(cached) = open.get_mut(&path) {
            match &mut cached.state {
                Slot::Ready(entry) => {
                    check_identity(&entry.args, &record)?;
                    if cached.host.is_some() {
                        entry.admit(1);
                        let args = OpenArgs::in_root(home, store, None).with_project(project);
                        return Ok((
                            args,
                            Running {
                                path,
                                owner: Route::Host,
                            },
                        ));
                    }
                    // A direct owner cannot be displaced while it holds this
                    // path. A dedicated host worker acquires its own lease.
                    return Err(refused(
                        "Unavailable",
                        format!("Store {store} is already open on the direct route"),
                    ));
                }
                Slot::Opening(pending) => {
                    let pending = pending.clone();
                    drop(open);
                    drop(record_lock);
                    pending.wait()?;
                }
                Slot::Closing(pending) => {
                    let pending = pending.clone();
                    drop(open);
                    drop(record_lock);
                    pending.wait()?;
                }
            }
        } else if count_host(&open, home) < HOST_STORES {
            let pending = reserve(&mut open, path.clone(), Some(home.to_path_buf()));
            drop(open);
            let args = OpenArgs::in_root(home, store, Some(record.to_record().clone()))
                .with_project(project);
            opened(&path, &args, 1, Route::Host, pending, || {
                lease::holders(home, store)?;
                let lease = lease::hold_host_locked(home, store)?;
                drop(record_lock);
                Ok(Some(lease))
            })?;
            return Ok((
                args,
                Running {
                    path,
                    owner: Route::Host,
                },
            ));
        } else {
            let victim = open
                .iter()
                .filter_map(|(path, slot)| match &slot.state {
                    Slot::Ready(entry)
                        if slot.host.as_deref() == Some(home)
                            && entry.calls == 0
                            && entry.settling == 0 =>
                    {
                        Some((path, entry.last))
                    }
                    _ => None,
                })
                .min_by_key(|&(_, last)| last)
                .map(|(path, _)| path.clone());
            drop(record_lock);
            if let Some(victim) = victim {
                let closing = start_close(&mut open, &victim);
                drop(open);
                let _ = finish_idle(&victim, closing);
            } else {
                wait_changed(open);
            }
        }
    }
}

pub(super) fn check_identity(args: &OpenArgs, record: &StoreRecord) -> Result<(), SdkRefusal> {
    let opened = args
        .store_record
        .as_ref()
        .expect("host admission supplied a record");
    let same = ["binding", "cursor"]
        .into_iter()
        .all(|key| opened.get(key) == record.field(key));
    if same {
        Ok(())
    } else {
        Err(refused(
            "Unavailable",
            format!("Store {} changed identity while open", record.name()),
        ))
    }
}

/// Validates a host close against the authoritative record under the same
/// lock as admission, then fences this cache entry before unlocking.
pub fn close_host(
    home: &Path,
    store: &str,
    check: impl FnOnce(&StoreRecord) -> Result<(), SdkRefusal>,
) -> Result<CloseReport, SdkRefusal> {
    crate::admin::record::identity::store_dir(home, store)?;
    let path = paths::store_file(home, store);
    let guard = store::lock_for_host_close(home, store)?;
    check(&StoreRecord::of(home, store)?)?;
    let mut open = cache();
    let Some(cached) = open.get(&path) else {
        return Ok(CloseReport::absent());
    };
    let state = match &cached.state {
        Slot::Opening(_) => "opening",
        Slot::Closing(_) => "closing",
        Slot::Ready(entry) if entry.calls > 0 => "active",
        Slot::Ready(entry) if entry.settling > 0 => "checkpointing",
        Slot::Ready(_) if cached.host.is_none() => "direct",
        Slot::Ready(_) => {
            let closing = start_close(&mut open, &path);
            drop(open);
            drop(guard);
            return finish_close(&path, closing);
        }
    };
    Err(with_next(
        in_use(&path, state),
        "store",
        SdkValue::text(store),
    ))
}
