//! What killed image copies left, removed by the next copy (spec v20,
//! owner 2026-11-19): `machine load-image` and `sandbox create --image`,
//! when they copy an image to a machine, sweep under that machine's
//! [`PruneGate`] right after they take their own [`ImageLock`]:
//!
//! - the export containers of this home in the host's Docker (labelled
//!   `layerfs.role=image-export` and `layerfs.home=<id>`) of every image
//!   and machine whose copy lock is free, each taken without waiting and
//!   held while `docker rm -v` removes the container; those of the copy's
//!   own image and machine under the lock it holds. Unlabelled containers of
//!   builds before the labels name no home, so no home can prove them its
//!   own: they are left alone.
//! - the unpack temporaries `.tmp-<hex>.2-<id>` in the machine's image
//!   cache of every image whose copy lock in the home is free, each under
//!   its unpack lock `<hex>.2.lock` taken without waiting on the machine
//!   (the `image.artifact` step `sweep`).
//!
//! One attempt inside [`BOUNDS`]: a removal that fails is a line of
//! `log/sweep.jsonl` (the home's for containers; the machine's for its
//! cache) and waits for the next copy; nothing a sweep does fails the copy.
//! `machine prune --unused` keeps removing `.tmp-*` too.
use crate::docker::Docker;
use crate::image_artifact as cache;
use crate::image_load::{existing_home_id, owned_for_home_id, remove_container, Target};
use crate::image_lock::{ImageLock, PruneGate};
use crate::rootfs::{self, Temporary};
use crate::sweep::{Bounds, Budget, BOUNDS};
use layerfs_home::home::log_swept as log;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;
use std::time::{Duration, Instant};

/// Sweeps what killed copies left, for a copy of image `own` that holds its
/// lock on `target`'s machine (see the module).
pub(crate) fn sweep(docker: &Docker, target: &mut Target<'_>, own: &str) {
    let deadline = Instant::now() + BOUNDS.within;
    let (home, machine) = (target.home().to_owned(), target.machine().to_owned());
    let lock_dir = ImageLock::dir(&home, &machine);
    let _gate = match PruneGate::hold(&home, &machine) {
        Ok(gate) => gate,
        Err(refusal) => return log(&home, "image_sweep", &lock_dir, &Err(refusal.message)),
    };
    exports(docker, &home, (&machine, own), deadline);
    // every copy that holds its lock now is left alone, and none starts
    // writing while the gate is held; a lock that cannot be read is held
    let locked = match crate::images::scan_image_locks(&lock_dir) {
        Ok((locked, errors)) => (locked.into_iter())
            .chain(errors.into_keys())
            .filter(|id| id != own)
            .map(SdkValue::Text)
            .collect(),
        Err(refusal) => return log(&home, "image_tmp", &lock_dir, &Err(refusal.message)),
    };
    let within = deadline.saturating_duration_since(Instant::now());
    let swept = target.step(SdkValue::map([
        ("step", SdkValue::text("sweep")),
        ("locked", SdkValue::List(locked)),
        ("within_ms", SdkValue::Int(within.as_millis() as i64)),
    ]));
    if let Err(refusal) = swept {
        log(&home, "image_tmp", &lock_dir, &Err(refusal.message));
    }
}

/// This home's export containers whose copy lock is free, removed with
/// `docker rm -v` under that lock; those of `own` (machine and image) under
/// the lock the copy holds.
fn exports(docker: &Docker, home: &Path, own: (&str, &str), deadline: Instant) {
    let Ok(Some(home_id)) = existing_home_id(home) else {
        return;
    };
    let owned = match owned_for_home_id(docker, &home_id, None, None) {
        Ok(owned) => owned,
        Err(refusal) => {
            let at = Path::new("docker");
            return log(home, "export_container", at, &Err(refusal.message));
        }
    };
    for container in owned {
        if Instant::now() >= deadline {
            return;
        }
        let mine = (container.machine.as_str(), container.image.as_str()) == own;
        let _guard = match mine {
            true => None,
            false => match ImageLock::try_hold(home, &container.machine, &container.image) {
                Ok(Some(lock)) => Some(lock),
                _ => continue,
            },
        };
        let outcome = match remove_container(docker, &container.name) {
            true => Ok(None),
            false => Err(format!("docker rm -v {} did not remove it", container.name)),
        };
        log(
            home,
            "export_container",
            Path::new(&container.name),
            &outcome,
        );
    }
}

/// The `image.artifact` step `sweep` on the machine whose root is `root`
/// and image cache `cache`: `{ locked: [id], within_ms }`. Each unpack
/// temporary of an image not `locked` goes under its unpack lock, taken
/// without waiting: `{ removed }`, the number removed.
pub(crate) fn temporaries(root: &Path, dir: &Path, args: &SdkValue) -> SdkValue {
    let locked = cache::texts(args.get("locked"));
    let within = (args.get_int("within_ms"))
        .and_then(|ms| u64::try_from(ms).ok())
        .map_or(BOUNDS.within, |ms| {
            Duration::from_millis(ms).min(BOUNDS.within)
        });
    let mut budget = Budget::new(Bounds { within, ..BOUNDS });
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if !budget.more() {
                break;
            }
            if let Ok(name) = entry.file_name().into_string() {
                names.push(name);
            }
        }
    }
    let mut removed = 0;
    for name in names {
        let Some(hex) = rootfs::temporary_digest(&name) else {
            continue;
        };
        if locked
            .iter()
            .any(|id| id.strip_prefix("sha256:") == Some(hex))
        {
            continue;
        }
        if budget.time_left().is_zero() {
            break;
        }
        let path = dir.join(&name);
        match rootfs::remove_temporary(dir, &name) {
            Ok(Temporary::Removed) => {
                removed += 1;
                log(root, "image_tmp", &path, &Ok(None));
            }
            Ok(Temporary::Absent | Temporary::Held) => {}
            Err(error) => log(root, "image_tmp", &path, &Err(error.to_string())),
        }
    }
    SdkValue::map([("removed", SdkValue::Int(removed))])
}
