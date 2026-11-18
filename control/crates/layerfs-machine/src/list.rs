//! `machine.list` and `machine.get`: every machine, or one, from the
//! home's records alone (call surface 2.0). Nothing is probed unless the
//! caller asks for `live` (`live.rs`).
use super::live::{self, Seen};
use super::record::MachineRecord;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{observed, paths};
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{failed, Outcome};
use layerfs_home::state::State;
use layerfs_home::time::iso;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A list as answers show it.
fn names(items: &[String]) -> SdkValue {
    SdkValue::List(items.iter().map(SdkValue::text).collect())
}

fn count(items: &[String]) -> SdkValue {
    SdkValue::Int(i64::try_from(items.len()).unwrap_or(i64::MAX))
}

/// The machine as `machine.get` and each `machine.list` item show it:
/// `platform`, `state` with its companion, `observed_at`, then what its
/// record holds (`machine`, `backend`, `supports`, its size, images and
/// settings, `message`, `created_at`) and what is on it (`stores`,
/// `sandboxes`, each with its count).
///
/// Without `seen`, the state is the record's ([`MachineRecord::recorded`])
/// and `observed_at` its last live read's time, null before the first.
/// With `seen` (a live read), the state, its companion, `message` and
/// `observed_at` are the probe's. A machine of a removed platform has no
/// backend, supports nothing and is `broken` `unsupported`.
pub(super) fn summary(
    home: &Path,
    record: &MachineRecord,
    seen: Option<&Seen>,
    related: Related,
) -> Result<SdkValue> {
    let stores = (related.stores_on)(home, record.name())?;
    let sandboxes = (related.sandboxes_on)(home, record.name())?;
    let (platform, backend, supports) = match record.removed() {
        Some(_) => (
            record.platform_name().unwrap_or_default().to_owned(),
            SdkValue::Null,
            SdkValue::List(Vec::new()),
        ),
        None => {
            let kind = record.platform()?;
            (
                kind.name().to_owned(),
                SdkValue::text(super::spec::backend_name(kind)),
                super::platforms::supports(kind),
            )
        }
    };
    let file = paths::machine_record(home, record.name());
    let (state, message, observed) = match seen {
        Some(seen) => (seen.state.clone(), seen.message.clone(), seen.at),
        None => {
            let state = record.recorded(sandboxes.len());
            let message = match record.removed() {
                Some(reason) => Some(reason.to_owned()),
                None => record.message(&state, sandboxes.len()),
            };
            (state, message, observed::read(&file))
        }
    };
    let mut fields = vec![("platform", SdkValue::Text(platform))];
    fields.extend(state.fields());
    if !matches!(state, State::Unknown { .. }) {
        fields.push(("observed_at", SdkValue::opt_text(observed.map(iso))));
    }
    let set = settings(record);
    let of = |key: &str| {
        set.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, v)| v.clone())
    };
    fields.push(("machine", SdkValue::text(record.name())));
    fields.push(("backend", backend));
    fields.push(("supports", supports));
    for key in SETTINGS {
        fields.push((key, of(key).unwrap_or_default()));
    }
    fields.extend([
        ("message", SdkValue::opt_text(message)),
        ("stores", names(&stores)),
        ("stores_count", count(&stores)),
        ("sandboxes", names(&sandboxes)),
        ("sandboxes_count", count(&sandboxes)),
        ("created_at", of("created_at").unwrap_or_default()),
    ]);
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// `machine.list()`: `{ items }`, every machine from the home's records.
/// With `live`, every machine is probed once, all at the same time
/// ([`live::observe`]), and each row's state comes from its probe.
pub fn list(home: &Path, live: bool, related: Related) -> Outcome {
    let mut records = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        if let Some(record) = MachineRecord::read(home, &name)? {
            records.push(record);
        }
    }
    let seen = match live {
        true => live::observe(home, &records, related)?,
        false => records.iter().map(|_| None).collect(),
    };
    let mut items = Vec::new();
    for (record, seen) in records.iter().zip(&seen) {
        items.push(summary(home, record, seen.as_ref(), related)?);
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}

/// `machine.get(machine)`: its record and state, with no probe. With
/// `live`, one probe of its provider and one hello to its worker, and the
/// `live` section: `{ disk_free_bytes, tool_version, worker }`. A read:
/// answered while an `op` holds the machine too.
pub fn get(home: &Path, name: &MachineName, live: bool, related: Related) -> Outcome {
    let rows = [MachineRecord::of(home, name)?];
    if !live {
        return summary(home, &rows[0], None, related);
    }
    let seen = live::observe(home, &rows, related)?.pop().flatten();
    let shown = summary(home, &rows[0], seen.as_ref(), related)?;
    let section = seen.as_ref().map_or_else(live::nothing, Seen::section);
    Ok(layerfs_home::home::with_field(shown, "live", section))
}

/// The keys of a machine's size and settings, in the order answers list them.
const SETTINGS: [&str; 8] = [
    "cpus",
    "memory_bytes",
    "disk_bytes",
    "image",
    "os_image",
    "docker",
    "wsl",
    "daemon",
];

/// What a machine's record says of its make and settings, as answers name
/// them: `machine`, `platform`, `cpus`, `memory_bytes`, `disk_bytes`,
/// `image`, `os_image`, `docker`, `wsl`, `daemon` and `created_at`; null
/// what it does not hold.
fn settings(record: &MachineRecord) -> Vec<(&'static str, SdkValue)> {
    let of = |field: &str| record.field(field).cloned().unwrap_or_default();
    vec![
        ("machine", SdkValue::text(record.name())),
        ("platform", SdkValue::opt_text(record.platform_name())),
        ("cpus", of("cpus")),
        ("memory_bytes", of("memory")),
        ("disk_bytes", of("disk")),
        ("image", of("image")),
        ("os_image", of("os_image")),
        ("docker", of("docker")),
        ("wsl", of("wsl")),
        ("daemon", of("daemon")),
        (
            "created_at",
            SdkValue::opt_text(record.int("created").map(iso)),
        ),
    ]
}

/// The answer of a lifecycle call that changed machine `record`: `ok`,
/// `outcome` (the word for what it did), then each of `keys` its record
/// holds ([`settings`]; `backend` is its platform's).
pub(super) fn changed(record: &MachineRecord, outcome: &str, keys: &[&str]) -> Result<SdkValue> {
    let mut set = settings(record);
    if keys.contains(&"backend") {
        let backend = super::spec::backend_name(record.platform()?);
        set.push(("backend", SdkValue::text(backend)));
    }
    let mut fields = vec![
        ("ok".to_owned(), SdkValue::Bool(true)),
        ("outcome".to_owned(), SdkValue::text(outcome)),
    ];
    for key in keys {
        let value = set.iter().find(|(name, _)| name == key);
        let value = value.map(|(_, value)| value.clone()).unwrap_or_default();
        fields.push(((*key).to_owned(), value));
    }
    Ok(SdkValue::Map(fields))
}
