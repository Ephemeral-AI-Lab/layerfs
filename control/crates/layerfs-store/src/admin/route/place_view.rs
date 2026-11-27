//! The bounded place view the control layer builds from its home.
//!
//! Admission on the engine side never reads a home file. The control layer
//! reads the place records it can see once per admitting Store call and hands
//! the engine a bounded map: each machine's current `MachineRecord.incarnation`,
//! each Sandbox's current generation, and every machine-level `sandbox-retired`
//! tombstone. A name missing from the view is no proof, never death.
//!
//! Only a call whose admission compares placed mount intents reads it: the
//! exclusive removals named by [`admits`] (a Branch removal, a project's file
//! removal), and a project delete and a machine's start reconcile, which
//! install it through [`scoped`] directly.
//! Every other Store call (reads, listings, file writes, merges) builds and
//! sends none: their admission meets only operation intents, which carry no
//! place, so a view could not change it.
//!
//! The view is bounded by the places the home actually records; it never
//! scans a Store, a Workspace or a content pack.
use crate::admin::open::args::OpenArgs;
use layerfs_history::PlaceView;
use layerfs_home::disk::{paths, read_record};
use layerfs_machine::record::machine::MachineRecord;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// The public Store methods whose engine admission compares placed mount
/// intents with the view: the exclusive removals. A shared intent (a file
/// write's private Workspace, a merge) conflicts only with operation intents,
/// which record no place.
pub const ADMITTING: [&str; 2] = ["branch.remove", "project.hide_path"];

/// Whether Store method `method` reads the place view ([`ADMITTING`]).
#[must_use]
pub fn admits(method: &str) -> bool {
    ADMITTING.contains(&method)
}

/// Runs `work` with the place view of `open`'s home installed, unless a
/// caller (a worker frame) installed one already: one admitting call's read.
pub fn scoped<R>(open: &OpenArgs, work: impl FnOnce() -> R) -> R {
    let place = match layerfs_persistence::has_place_view() {
        true => None,
        false => from_open(open),
    };
    layerfs_persistence::with_place_view(place, work)
}

/// Builds the bounded place view from the caller's home alone.
///
/// A missing home, a machine without a record and a Sandbox without a record
/// all contribute nothing. A read error is treated the same way: the view
/// stays conservative and the call is never refused because a place record
/// could not be read. None means the home carried no place evidence at all.
#[must_use]
pub fn from_home(home: &Path) -> Option<PlaceView> {
    let mut view = PlaceView::new();
    for machine in paths::names(&home.join("machine")).unwrap_or_default() {
        if let Ok(Some(record)) = MachineRecord::read(home, &machine) {
            if let Some(incarnation) = record.incarnation() {
                view.set_machine(&machine, incarnation);
            }
        }
        for (name, generation) in tombstones(&paths::sandbox_retired_dir(home, &machine)) {
            view.set_sandbox_retired(&machine, &name, generation);
        }
    }
    for sandbox in paths::names(&home.join("sandbox")).unwrap_or_default() {
        let Ok(Some(record)) = read_record(&paths::sandbox_record(home, &sandbox)) else {
            continue;
        };
        let (Some(generation), Some(machine)) =
            (record.get_text("generation"), record.get_text("machine"))
        else {
            continue;
        };
        view.set_sandbox_generation(machine, &sandbox, generation);
    }
    if view.is_empty() {
        None
    } else {
        Some(view)
    }
}

/// [`from_home`] for a Store call's [`OpenArgs`]: a call without a home
/// carries no place evidence.
#[must_use]
pub fn from_open(open: &OpenArgs) -> Option<PlaceView> {
    from_home(open.home.as_deref()?)
}

/// The view as the worker frame carries it, when home has one.
#[must_use]
pub fn encoded_from_home(home: &Path) -> Option<SdkValue> {
    from_home(home)
        .map(|view| layerfs_store_service::history::gc_admission::encode_place_view(&view))
}

/// The tombstones dir holds, as (Sandbox name, retired generation). A
/// tombstone without a generation names nothing admission can compare.
fn tombstones(dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(Some(record)) = read_record(&path) else {
            continue;
        };
        if let Some(generation) = record.get_text("generation") {
            found.push((name.to_owned(), generation.to_owned()));
        }
    }
    found.sort();
    found
}
