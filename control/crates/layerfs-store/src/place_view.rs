//! The bounded place view the control layer builds from its home and the
//! Store's recorded machine homes.
//!
//! Admission on the engine side never reads a home file. The control layer
//! reads the place records it can see once per Store call and hands the engine
//! a bounded map: each machine's current MachineRecord.incarnation, each
//! Sandbox's current generation, and every machine-level sandbox-retired
//! tombstone. The caller's home is read first; then every `machines.<machine>`
//! entry of the Store record that records a `home` path is read and merged
//! into the same view. That is how a shared PostgreSQL Store proves an older
//! placement dead when another home recorded the newer one. A home it cannot
//! read supplies no authority; a name missing from the view is no proof, never
//! death.
//!
//! The view is bounded by the places the homes actually record and by
//! MAXIMUM_PLACES; it never scans a Store, a Workspace or a content pack.
use crate::open_args::OpenArgs;
use crate::record::StoreRecord;
use layerfs_bridge::control::SdkValue;
use layerfs_history::{ordered_token_parts, PlaceView};
use layerfs_home::home::{paths, read_record};
use layerfs_machine::record::MachineRecord;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The most places one view carries. A home with more is truncated rather
/// than making admission unbounded; a missing name is conservative live.
pub const MAXIMUM_PLACES: usize = 4096;

/// Builds the bounded place view from the caller's home alone.
///
/// A missing home, a machine without a record and a Sandbox without a record
/// all contribute nothing. A read error is treated the same way: the view
/// stays conservative and the call is never refused because a place record
/// could not be read. None means the home carried no place evidence at all.
#[must_use]
pub fn from_home(home: &Path) -> Option<PlaceView> {
    let homes = [home.to_path_buf()];
    from_homes(&homes)
}

/// Builds the merged view from the caller's home and every `home` path the
/// Store record's `machines.<machine>` entries record, in that order.
///
/// An entry without a home, a relative path and an unreachable path all
/// contribute no proof, so a Store whose second home is offline keeps its
/// conservative live answer.
#[must_use]
pub fn from_store(home: &Path, record: &StoreRecord) -> Option<PlaceView> {
    let mut homes = vec![home.to_path_buf()];
    for recorded in record.machine_homes() {
        if !homes.contains(&recorded) {
            homes.push(recorded);
        }
    }
    from_homes(&homes)
}

/// [`from_store`] for a Store call's [`OpenArgs`]: the caller's home and the
/// Store record it carries. A call without a record reads its home alone.
#[must_use]
pub fn from_open(open: &OpenArgs) -> Option<PlaceView> {
    let home = open.home.as_deref()?;
    let (Some(store), Some(record)) = (open.store.as_deref(), open.store_record.as_ref()) else {
        return from_home(home);
    };
    from_store(home, &StoreRecord::from_record(store, record.clone()))
}

/// Builds the merged bounded view from `homes`, in order.
///
/// The same machine or Sandbox recorded by two homes keeps the newer ordered
/// value. Two values that cannot be ordered and differ prove nothing, so that
/// place is left out rather than guessed. The merged places are ordered by
/// their ordered token, newest first, and capped at [`MAXIMUM_PLACES`].
#[must_use]
pub fn from_homes(homes: &[PathBuf]) -> Option<PlaceView> {
    let mut merged = Merged::default();
    for home in homes {
        merged.observe_home(home);
    }
    merged.view(MAXIMUM_PLACES)
}

/// The view as the worker frame carries it, when home has one.
#[must_use]
pub fn encoded_from_home(home: &Path) -> Option<SdkValue> {
    from_home(home).map(|view| layerfs_store_service::history::admission::encode_place_view(&view))
}

/// The merged cross-home view as the worker frame carries it.
#[must_use]
pub fn encoded_from_store(home: &Path, record: &StoreRecord) -> Option<SdkValue> {
    from_store(home, record)
        .map(|view| layerfs_store_service::history::admission::encode_place_view(&view))
}

/// [`encoded_from_store`] for a Store call's [`OpenArgs`].
#[must_use]
pub fn encoded_from_open(open: &OpenArgs) -> Option<SdkValue> {
    from_open(open).map(|view| layerfs_store_service::history::admission::encode_place_view(&view))
}

/// One place value read from one or more homes. A conflict leaves no value.
#[derive(Default)]
struct Field {
    value: Option<String>,
    conflicted: bool,
}

impl Field {
    /// Merges one recorded value. Equal values are one place; two ordered
    /// values keep the newer; an unordered difference (including one side
    /// ordered and the other not) proves nothing.
    fn observe(&mut self, recorded: &str) {
        if self.conflicted {
            return;
        }
        let Some(current) = self.value.as_deref() else {
            self.value = Some(recorded.to_owned());
            return;
        };
        if current == recorded {
            return;
        }
        match (ordered_token_parts(current), ordered_token_parts(recorded)) {
            (Some(current_parts), Some(recorded_parts)) => {
                if recorded_parts > current_parts {
                    self.value = Some(recorded.to_owned());
                }
            }
            _ => {
                self.value = None;
                self.conflicted = true;
            }
        }
    }
}

/// One Sandbox's two place fields.
#[derive(Default)]
struct SandboxFields {
    generation: Field,
    retired: Field,
}

/// The accumulated places of every home read so far.
#[derive(Default)]
struct Merged {
    machines: BTreeMap<String, Field>,
    sandboxes: BTreeMap<(String, String), SandboxFields>,
}

/// One place a view can carry, ordered after the merge.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Place {
    Machine(String),
    Generation(String, String),
    Retired(String, String),
}

impl Merged {
    fn sandbox(&mut self, machine: &str, sandbox: &str) -> &mut SandboxFields {
        self.sandboxes
            .entry((machine.to_owned(), sandbox.to_owned()))
            .or_default()
    }

    /// Reads one home's machine records, Sandboxes and tombstones. A folder
    /// or record that cannot be read contributes nothing.
    fn observe_home(&mut self, home: &Path) {
        for machine in paths::names(&home.join("machine")).unwrap_or_default() {
            if let Ok(Some(record)) = MachineRecord::read(home, &machine) {
                if let Some(incarnation) = record.incarnation() {
                    self.machines
                        .entry(machine.clone())
                        .or_default()
                        .observe(incarnation);
                }
            }
            for (name, generation) in tombstones(&paths::sandbox_retired_dir(home, &machine)) {
                self.sandbox(&machine, &name).retired.observe(&generation);
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
            self.sandbox(machine, &sandbox)
                .generation
                .observe(generation);
        }
    }

    /// The merged view, capped at `cap` places ordered by their token, newest
    /// first and then by name. A place with no value (a conflict) is absent.
    fn view(&self, cap: usize) -> Option<PlaceView> {
        let mut places: Vec<(String, Place)> = Vec::new();
        for (machine, field) in &self.machines {
            if let Some(token) = &field.value {
                places.push((token.clone(), Place::Machine(machine.clone())));
            }
        }
        for ((machine, sandbox), fields) in &self.sandboxes {
            if let Some(token) = &fields.generation.value {
                places.push((
                    token.clone(),
                    Place::Generation(machine.clone(), sandbox.clone()),
                ));
            }
            if let Some(token) = &fields.retired.value {
                places.push((
                    token.clone(),
                    Place::Retired(machine.clone(), sandbox.clone()),
                ));
            }
        }
        // Newest ordered token first; an unordered value has no order and
        // sorts after every ordered one. The name tie-break is stable.
        places.sort_by(|(left, left_place), (right, right_place)| {
            token_order(right)
                .cmp(&token_order(left))
                .then_with(|| left_place.cmp(right_place))
        });
        places.truncate(cap);

        let mut view = PlaceView::new();
        for (_, place) in places {
            match place {
                Place::Machine(machine) => {
                    if let Some(token) = self.machines.get(&machine).and_then(|f| f.value.clone()) {
                        view.set_machine(&machine, token);
                    }
                }
                Place::Generation(machine, sandbox) => {
                    if let Some(token) = self
                        .sandboxes
                        .get(&(machine.clone(), sandbox.clone()))
                        .and_then(|f| f.generation.value.clone())
                    {
                        view.set_sandbox_generation(&machine, &sandbox, token);
                    }
                }
                Place::Retired(machine, sandbox) => {
                    if let Some(token) = self
                        .sandboxes
                        .get(&(machine.clone(), sandbox.clone()))
                        .and_then(|f| f.retired.value.clone())
                    {
                        view.set_sandbox_retired(&machine, &sandbox, token);
                    }
                }
            }
        }
        if view.is_empty() {
            None
        } else {
            Some(view)
        }
    }
}

/// The sort key of one place token: ordered tokens first (newest first in a
/// descending sort), then their milliseconds and nonce; an unordered token
/// has no order and sorts last.
fn token_order(token: &str) -> (u8, u64, &str, &str) {
    match ordered_token_parts(token) {
        Some((milliseconds, nonce)) => (1, milliseconds, nonce, token),
        None => (0, 0, "", token),
    }
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

