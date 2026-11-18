//! `machine.stats`: what a machine has and uses now; with `--with space`,
//! where its data disk goes (`machine.space` before the call surface 2.0).
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_home::time::iso;
use layerfs_sandbox::inspect::{machine_space, machine_stats};

/// Its answer: the machine's processors, memory and data disk.
const FORM: Form = Form::of(
    &[],
    &[
        "cpus",
        "memory_used_bytes",
        "memory_total_bytes",
        "disk_free_bytes",
        "disk_total_bytes",
        "incomplete",
    ],
    &[
        "cpus",
        "memory_used_bytes",
        "memory_total_bytes",
        "disk_free_bytes",
        "disk_total_bytes",
        "incomplete",
        "disk_used_bytes",
        "reasons",
        "machine",
        "platform",
        "observed_at",
        "scope",
        "cpu_busy_ms",
        "cpu_total_ms",
        "worker",
        "data_disk",
        "stores",
        "sandboxes",
    ],
);

/// The parts of `--with space`, in the order it lists them.
const PARTS: [&str; 6] = [
    "stores",
    "sandboxes",
    "images",
    "other_runs",
    "logs",
    "other",
];

/// Its table entry. It is quiet: a UI polls it.
pub(crate) const CALL: Call = call(
    "machine",
    "stats",
    &[a("machine", Name)],
    &[],
    "a machine's observed processors, memory and data disk, its Stores' sizes and live Sandboxes",
    run,
)
.quiet()
.form(&FORM)
.with(&[Section::own(
    "space",
    &[
        "observed_at",
        "layerfs_bytes",
        "outside_bytes",
        "parts",
        "stores",
        "sandboxes",
        "unreadable_count",
        "incomplete",
        "reasons",
    ],
)]);

/// Runs `machine.stats`: one bounded request where the machine's data is,
/// and with `--with space` one bounded walk of LayerFS's folders there. It
/// takes no lock or lease, starts nothing, dials no daemon and writes
/// nothing, the call log included.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let read = machine_stats(&context.home, &machine)?;
    let mut answer = stats(&read);
    if context.with("space") {
        let walked = machine_space(&context.home, &machine)?;
        answer = layerfs_home::home::with_field(answer, "space", space(&walked));
    }
    Ok(answer)
}

/// Microseconds as whole milliseconds.
fn ms(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Int(usec) => SdkValue::Int(usec / 1000),
        other => other,
    }
}

/// `layerfs_sandbox::inspect::machine_stats`'s answer as the row names it.
fn stats(read: &SdkValue) -> SdkValue {
    let resources = field(read, "resources");
    let of = |name: &str| field(&resources, name);
    let mut reasons = words(resources.get_text("reason"));
    reasons.extend(words(read.get_text("reason")));
    let complete = !matches!(read.get("complete"), Some(SdkValue::Bool(false)));
    let stores = rows(read.get("stores"), |store| {
        SdkValue::map([
            ("store", field(store, "name")),
            ("size", field(store, "bytes")),
            ("allocated_bytes", field(store, "allocated")),
        ])
    });
    let sandboxes = rows(field(read, "sandboxes").get("items"), |sandbox| {
        SdkValue::map([
            ("sandbox", field(sandbox, "name")),
            ("live", field(sandbox, "live")),
            ("live_reason", field(sandbox, "live_reason")),
            ("started_at", at(field(sandbox, "started_at"))),
            ("usage", field(sandbox, "usage")),
            ("usage_reason", field(sandbox, "usage_reason")),
        ])
    });
    SdkValue::map([
        ("cpus", of("cpus")),
        ("memory_used_bytes", of("memory_used")),
        ("memory_total_bytes", of("memory_total")),
        ("disk_free_bytes", of("disk_free")),
        ("disk_total_bytes", of("disk_total")),
        ("incomplete", SdkValue::Bool(!complete)),
        ("disk_used_bytes", of("disk_used")),
        ("reasons", SdkValue::List(reasons)),
        ("machine", field(read, "machine")),
        ("platform", field(read, "platform")),
        ("observed_at", at(field(read, "observed_at"))),
        ("scope", of("scope")),
        ("cpu_busy_ms", ms(of("cpu_busy_usec"))),
        ("cpu_total_ms", ms(of("cpu_total_usec"))),
        ("worker", field(read, "worker")),
        ("data_disk", field(read, "data_disk")),
        ("stores", stores),
        ("sandboxes", sandboxes),
    ])
}

/// `layerfs_sandbox::inspect::machine_space`'s answer as the `space`
/// section names it.
fn space(walked: &SdkValue) -> SdkValue {
    let parts = field(walked, "parts");
    let parts = PARTS.map(|part| {
        SdkValue::map([
            ("part", SdkValue::text(part)),
            ("bytes", field(&parts, part)),
        ])
    });
    let stores = rows(walked.get("stores"), |store| {
        SdkValue::map([
            ("store", field(store, "name")),
            ("allocated_bytes", field(store, "bytes")),
        ])
    });
    let sandboxes = rows(walked.get("sandboxes"), |sandbox| {
        SdkValue::map([
            ("sandbox", field(sandbox, "name")),
            ("allocated_bytes", field(sandbox, "bytes")),
            ("overlays_bytes", field(sandbox, "overlays")),
            ("writes_bytes", field(sandbox, "writes")),
            ("home_bytes", field(sandbox, "home")),
            ("log_bytes", field(sandbox, "log")),
            ("runtime_bytes", field(sandbox, "runtime")),
            ("reason", field(sandbox, "reason")),
        ])
    });
    let complete = !matches!(walked.get("complete"), Some(SdkValue::Bool(false)));
    SdkValue::map([
        ("observed_at", at(field(walked, "observed_at"))),
        ("layerfs_bytes", field(walked, "layerfs")),
        ("outside_bytes", field(walked, "outside")),
        ("parts", SdkValue::List(parts.into())),
        ("stores", stores),
        ("sandboxes", sandboxes),
        ("unreadable_count", field(walked, "unreadable")),
        ("incomplete", SdkValue::Bool(!complete)),
        ("reasons", SdkValue::List(words(walked.get_text("reason")))),
    ])
}

// Small helpers the machine family's entries share to answer what a library
// function returned under the row's names.

/// `value`'s field `name`, null when it has none.
pub(super) fn field(value: &SdkValue, name: &str) -> SdkValue {
    value.get(name).cloned().unwrap_or_default()
}

/// The list `value` as an answer shows it: its first 20 items, and their
/// count only when it was cut.
#[cfg(feature = "machine")]
pub(super) fn cut(value: SdkValue) -> (SdkValue, SdkValue) {
    let items = match value {
        SdkValue::List(items) => items,
        _ => Vec::new(),
    };
    layerfs_home::helpers::shown(items, 0, false)
}

/// Each row of the list `value`, as `row` answers it; an empty list when
/// there is none.
pub(super) fn rows(value: Option<&SdkValue>, row: impl Fn(&SdkValue) -> SdkValue) -> SdkValue {
    match value {
        Some(SdkValue::List(items)) => SdkValue::List(items.iter().map(row).collect()),
        _ => SdkValue::List(Vec::new()),
    }
}

/// A time in milliseconds since 1970 as answers spell it (`..._at`); text
/// that is already a time stays as it is.
pub(super) fn at(value: SdkValue) -> SdkValue {
    match value {
        SdkValue::Int(ms) => SdkValue::Text(iso(ms)),
        other => other,
    }
}

/// The comma-joined reason words a library answer gives, as a list.
pub(super) fn words(value: Option<&str>) -> Vec<SdkValue> {
    let words = value.unwrap_or("").split(',').map(str::trim);
    words
        .filter(|word| !word.is_empty())
        .map(SdkValue::text)
        .collect()
}
