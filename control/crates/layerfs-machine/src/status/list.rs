//! `machine.list`: every machine from the home's records alone (call
//! surface 2.0), and the summary `machine.get` shows of one (`get.rs`).
//! Nothing is probed unless the caller asks for `live` (`live.rs`).
use super::live::{self, Seen};
use crate::record::machine::{Holders, MachineRecord};
use crate::Related;
use layerfs_home::disk::paths;
use layerfs_home::record::observed;
use layerfs_home::reply::outcome::{failed, Outcome};
use layerfs_home::reply::state::{Reason, State};
use layerfs_home::reply::time::iso;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A list as answers show it: the first 20 names, and the count only when
/// there were more.
fn names(items: &[String]) -> (SdkValue, SdkValue) {
    let names = items.iter().map(SdkValue::text).collect();
    layerfs_home::reply::values::shown(names, 0, false)
}

/// Whether the machine shares this computer's loopback, as its platform's
/// adapter declares it: `shared` or `apart`; null for a removed platform or
/// a record that describes no machine.
fn loopback(home: &Path, record: &MachineRecord) -> SdkValue {
    if record.removed().is_some() {
        return SdkValue::Null;
    }
    match crate::record::adapter_spec::provider_spec(home, record) {
        Ok(spec) if crate::platform::shares_loopback(&spec) => SdkValue::text("shared"),
        Ok(_) => SdkValue::text("apart"),
        Err(_) => SdkValue::Null,
    }
}

/// The machine as `machine.get` and each `machine.list` item show it:
/// `platform`, `state` with its companion, `observed_at`, then what its
/// record holds (`machine`, `backend`, `supports`, `loopback`, its size, images and
/// settings, `message`, `created_at`) and what is on it (`stores`,
/// `sandboxes`, each with its count when cut).
///
/// Without `seen`, the state is the record's ([`MachineRecord::recorded`])
/// and `observed_at` its last live read's time, null before the first.
/// With `seen` (a live read), the state, its companion, `message` and
/// `observed_at` are the probe's. A machine of a removed platform has no
/// backend, supports nothing and is `broken` `unsupported`.
pub(crate) fn summary(
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
                SdkValue::text(crate::record::adapter_spec::backend_name(kind)),
                crate::platform::available::supports(kind),
            )
        }
    };
    let loopback = loopback(home, record);
    let file = paths::machine_record(home, record.name());
    let (state, message, observed) = match seen {
        // a VM a live read found gone shows the line that removes what is
        // on it first, as its records do afterwards (area 10)
        Some(seen) => match &seen.state {
            State::Broken {
                reason: Reason::Missing,
                ..
            } => {
                let held = Holders {
                    sandboxes: &sandboxes,
                    stores: &stores,
                    hosted: &[],
                };
                let fix = crate::record::machine::no_vm_name_fix(record.name(), held);
                let state = State::broken(Reason::Missing, Some(&fix));
                (state, seen.message.clone(), seen.at)
            }
            state => (state.clone(), seen.message.clone(), seen.at),
        },
        None => {
            // a record with no `vm_name` shows the line that removes what
            // is on it first; the old Firecracker records it hosted are
            // read for it alone
            let hosted = match record.nameless() {
                true => (related.hosted_on)(home, record.name())?,
                false => Vec::new(),
            };
            let held = Holders {
                sandboxes: &sandboxes,
                stores: &stores,
                hosted: &hosted,
            };
            let state = record.recorded_held(held);
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
    fields.push(("loopback", loopback));
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
    let state = crate::status::state::unreadable(name);
    // a built-in machine's platform is its name; another's is in the record
    let platform = layerfs_home::names::built_in_platform(name);
    let mut fields = vec![("platform", SdkValue::opt_text(platform))];
    fields.extend(state.fields());
    fields.extend([
        ("machine", SdkValue::text(name)),
        ("message", SdkValue::text(&refusal.message)),
    ]);
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
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
pub(crate) fn changed(record: &MachineRecord, outcome: &str, keys: &[&str]) -> Result<SdkValue> {
    let mut set = settings(record);
    if keys.contains(&"backend") {
        let backend = crate::record::adapter_spec::backend_name(record.platform()?);
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
