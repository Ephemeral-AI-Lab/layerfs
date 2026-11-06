//! `machine.platforms` (#535 §9a, D10, P2): the support table for this host
//! and build, the rows of `control/SUPPORT.json`, each with its live probe
//! (`ready`, `missing`, `version`). It starts no machine (P4). Also the
//! per-machine `supports` of `machine.list` and `machine.get` (P3).
use super::spec as record;
use layerfs_bridge::control::SdkValue;
use layerfs_home::context::Context;
use layerfs_home::outcome::Outcome;
use layerfs_platform::{self as adapters, HostOs, Platform, Probe, SupportRow, Supports};

/// `machine.platforms()`: `{ host, platforms: [row] }`.
pub fn platforms(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let host = HostOs::current();
    let tools = record::tools(&context.home)?;
    let rows = adapters::platforms(host)
        .iter()
        .map(|row| row_value(row, adapters::probe(row.platform, &tools)))
        .collect();
    Ok(SdkValue::map([
        ("host", SdkValue::text(host.as_str())),
        ("platforms", SdkValue::List(rows)),
    ]))
}

/// A machine's `supports`: its platform's row on this host.
pub fn supports(platform: Platform) -> SdkValue {
    let Supports {
        sqlite,
        postgresql_sandbox,
        reason,
    } = adapters::support(platform, HostOs::current()).supports();
    SdkValue::map([
        ("sqlite", SdkValue::Bool(sqlite)),
        ("postgresql_sandbox", SdkValue::Bool(postgresql_sandbox)),
        ("reason", optional(reason)),
    ])
}

fn row_value(row: &SupportRow, probe: Probe) -> SdkValue {
    let options = row
        .options
        .iter()
        .map(|option| {
            SdkValue::map([
                ("name", SdkValue::text(option.name)),
                ("kind", SdkValue::text(option.kind.as_str())),
            ])
        })
        .collect();
    SdkValue::map([
        ("platform", SdkValue::text(row.platform.name())),
        ("how", SdkValue::text(row.how.as_str())),
        ("backend", optional(row.backend.map(|b| b.stored_name()))),
        ("kernel", optional(row.kernel.map(|k| k.as_str()))),
        ("sqlite", optional(row.sqlite.map(|s| s.as_str()))),
        (
            "postgresql",
            SdkValue::map([
                (
                    "store_calls",
                    SdkValue::text(row.postgresql.store_calls.as_str()),
                ),
                ("sandbox", SdkValue::Bool(row.postgresql.sandbox)),
                ("reason", optional(row.postgresql.reason)),
            ]),
        ),
        ("ready", SdkValue::Bool(probe.ready)),
        (
            "missing",
            SdkValue::List(probe.missing.into_iter().map(SdkValue::Text).collect()),
        ),
        (
            "version",
            probe.version.map(SdkValue::Text).unwrap_or_default(),
        ),
        ("options", SdkValue::List(options)),
        ("description", SdkValue::text(&row.description)),
    ])
}

fn optional(text: Option<&str>) -> SdkValue {
    text.map(SdkValue::text).unwrap_or_default()
}
