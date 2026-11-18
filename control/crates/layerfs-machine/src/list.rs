//! `machine.list` and `machine.get`: every machine, or one, from the
//! home's records alone (call surface 2.0). Nothing is probed unless the
//! caller asks for `live` (`live.rs`).
use super::live::{self, Seen};
use super::record::MachineRecord;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{observed, paths};
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{failed, with_fix, Outcome};
use layerfs_home::state::State;
use layerfs_home::time::iso;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A list as answers show it: the first 20 names, and the count only when
/// there were more.
fn names(items: &[String]) -> (SdkValue, SdkValue) {
    let names = items.iter().map(SdkValue::text).collect();
    layerfs_home::helpers::shown(names, 0, false)
}

/// The machine as `machine.get` and each `machine.list` item show it:
/// `platform`, `state` with its companion, `observed_at`, then what its
/// record holds (`machine`, `backend`, `supports`, its size, images and
/// settings, `message`, `created_at`) and what is on it (`stores`,
/// `sandboxes`, each with its count when cut).
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
    let (stores, sandboxes) = (names(&stores), names(&sandboxes));
    fields.extend([
        ("message", SdkValue::opt_text(message)),
        ("stores", stores.0),
        ("stores_count", stores.1),
        ("sandboxes", sandboxes.0),
        ("sandboxes_count", sandboxes.1),
        ("created_at", of("created_at").unwrap_or_default()),
    ]);
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// `machine.list()`: `{ items }`, every machine from the home's records.
/// With `live`, every machine is probed once, all at the same time
/// ([`live::observe`]), and each row's state comes from its probe.
/// A machine whose record cannot be read is listed as `broken` `failed`,
/// its doctor as the fix and the read's refusal as `message`; it is not
/// probed, and the other machines are answered as usual.
pub fn list(home: &Path, live: bool, related: Related) -> Outcome {
    let mut records = Vec::new();
    let mut unread = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        match MachineRecord::read(home, &name) {
            Ok(record) => records.extend(record),
            Err(refusal) => unread.push(unreadable(&name, &refusal)),
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
    items.extend(unread);
    items.sort_by(|a, b| a.get_text("machine").cmp(&b.get_text("machine")));
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}

/// The `machine.list` item of machine `name` whose record could not be
/// read (`refusal`): `broken` `failed`, as the home's tree shows it.
fn unreadable(name: &str, refusal: &SdkRefusal) -> SdkValue {
    let state = crate::state::unreadable(name);
    // a built-in machine's platform is its name; another's is in the record
    let platform = layerfs_home::names::BUILT_IN.contains(&name);
    let mut fields = vec![("platform", SdkValue::opt_text(platform.then_some(name)))];
    fields.extend(state.fields());
    fields.extend([
        ("machine", SdkValue::text(name)),
        ("message", SdkValue::text(&refusal.message)),
    ]);
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

/// `machine.get(machine)`: its record and state, with no probe. With
/// `live`, one probe of its provider and one hello to its worker, and the
/// `live` section: `{ disk_free_bytes, tool_version, worker }`. A read:
/// answered while an `op` holds the machine too.
pub fn get(home: &Path, name: &MachineName, live: bool, related: Related) -> Outcome {
    // a record that cannot be read: refused, with its doctor as the fix
    let rows = [
        MachineRecord::of(home, name).map_err(|refusal| match refusal.code.as_str() {
            "Failed" => with_fix(refusal, crate::state::unreadable_fix(name.as_str())),
            _ => refusal,
        })?,
    ];
    if !live {
        return summary(home, &rows[0], None, related);
    }
    let seen = live::observe(home, &rows, related)?.pop().flatten();
    let shown = summary(home, &rows[0], seen.as_ref(), related)?;
    let section = match &seen {
        Some(seen) => seen.section(),
        None => live::unprobed(home, &rows[0]),
    };
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
