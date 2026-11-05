//! A Sandbox folder's bounded activity (#551 B8-158): the command running
//! now and the Workspaces closed most recently. It is written where the
//! command or the unmount runs, so a read model on that side sees it, and
//! keeps at most [`RUNNING`] and [`CLOSED`] entries. A file that cannot be
//! read is no activity, never an error: the Sandbox record is the truth.
use crate::common::home::{self, entity_lock, read_record, write_entity};
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// Commands this folder records as running at once.
pub const RUNNING: usize = 8;
/// Closed Workspaces this folder remembers.
pub const CLOSED: usize = 16;
const FILE: &str = "activity.json";

fn file(directory: &Path) -> std::path::PathBuf {
    directory.join(FILE)
}

fn items(value: &SdkValue, field: &str) -> Vec<SdkValue> {
    match value.get(field) {
        Some(SdkValue::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// Sets `field` to `items`, replacing an earlier value.
fn set(fields: &mut Vec<(String, SdkValue)>, field: &str, items: Vec<SdkValue>) {
    fields.retain(|(name, _)| name != field);
    fields.push((field.to_owned(), SdkValue::List(items)));
}

/// The folder's activity as the read model shows it: `running` and
/// `recently_closed`, each a list, empty when nothing was recorded.
pub fn read(directory: &Path) -> SdkValue {
    let value = read_record(&file(directory))
        .ok()
        .flatten()
        .unwrap_or_default();
    SdkValue::map([
        ("running", SdkValue::List(items(&value, "running"))),
        (
            "recently_closed",
            SdkValue::List(items(&value, "recently_closed")),
        ),
    ])
}

/// Rewrites the activity file under its own short lock, bounding each list.
fn update(
    directory: &Path,
    change: impl FnOnce(Vec<(String, SdkValue)>) -> Vec<(String, SdkValue)>,
) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    let _lock = entity_lock::take(
        &directory.join("activity.lock"),
        entity_lock::WAIT,
        "another call was recording this Sandbox's activity",
    )?;
    let current = read_record(&file(directory))?.unwrap_or_default();
    let fields = match current {
        SdkValue::Map(fields) => fields,
        _ => Vec::new(),
    };
    write_entity(&file(directory), &SdkValue::Map(change(fields)))
}

/// Records that `command` started in the Workspace at `path`; the token
/// [`finished`] takes. `None` when the record could not be written: the
/// command still runs, only its visibility is lost.
pub fn started(directory: &Path, path: &str, command: &str) -> Option<String> {
    let token = format!("{}-{}", std::process::id(), home::now_ms());
    let entry = SdkValue::map([
        ("token", SdkValue::text(&token)),
        ("path", SdkValue::text(path)),
        ("command", SdkValue::text(command)),
        ("started", SdkValue::Int(home::now_ms())),
    ]);
    update(directory, |mut fields| {
        let mut running = items(&SdkValue::Map(fields.clone()), "running");
        running.push(entry);
        while running.len() > RUNNING {
            running.remove(0);
        }
        set(&mut fields, "running", running);
        fields
    })
    .ok()?;
    Some(token)
}

/// Removes the running entry `token` names, after its command ended.
pub fn finished(directory: &Path, token: &str) {
    let _ = update(directory, |mut fields| {
        let running = items(&SdkValue::Map(fields.clone()), "running")
            .into_iter()
            .filter(|entry| entry.get_text("token") != Some(token))
            .collect();
        set(&mut fields, "running", running);
        fields
    });
}

/// Records that a Workspace ended: its `name` when it had one, its `path`,
/// its `branch` and when it closed.
pub fn closed(directory: &Path, name: Option<&str>, path: &str, branch: Option<&str>) {
    let entry = SdkValue::map([
        ("name", SdkValue::opt_text(name.map(str::to_owned))),
        ("path", SdkValue::text(path)),
        ("branch", SdkValue::opt_text(branch.map(str::to_owned))),
        ("closed", SdkValue::Int(home::now_ms())),
    ]);
    let _ = update(directory, |mut fields| {
        let mut closed = items(&SdkValue::Map(fields.clone()), "recently_closed");
        closed.push(entry);
        while closed.len() > CLOSED {
            closed.remove(0);
        }
        set(&mut fields, "recently_closed", closed);
        fields
    });
}
