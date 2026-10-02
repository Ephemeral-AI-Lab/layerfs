//! Exactly one operation; semantic proof/digests/reopen live in verify.rs.
use super::{
    json::{object, Json},
    model::{self, Args, Clock, Result},
    native,
};
use layerfs_content::AdvisoryPredecessors;
use phase6_live_probe::{
    strict_catalog::{LogicalUse, PlacementDomain, StrictCatalog},
    strict_client::Catalog,
    strict_read::{Access, ReadOwner},
    strict_writer::Writer,
};
use std::{sync::Arc, time::Instant};
pub fn run(args: &Args) -> Result<Json> {
    let row = args.row()?;
    let provider = model::provider()?;
    let caps = model::capacities()?;
    let open = Instant::now();
    let local = Arc::new(StrictCatalog::open_writable(&args.store)?);
    let open_ns = open.elapsed().as_nanos() as u64;
    let mut receipt=object([("catalog_open_ns",open_ns.into()),("catalog_open_scope","untimed schema/header/custody admission and fresh TEMP epoch; no object/candidate reads".into()),("save_id",Json::Null),("body_order",Json::Null),("object_id",Json::Null),("metrics",object([("writer",Json::Null),("delta",Json::Null),("pool",Json::Null),("provider",Json::Null),("native",Json::Null),("candidate",Json::Null),("read",Json::Null)]))]);
    match row {
        "small-new-full" | "small-exact-reuse" | "payload-prefix-write" => {
            let name = if row == "small-exact-reuse" {
                "whole0"
            } else {
                "whole1"
            };
            let mut argument = args.argument(name)?;
            if row == "payload-prefix-write" {
                argument = argument.with_predecessors(
                    AdvisoryPredecessors::explicit(model::fixture_id(args, "whole0")?)
                        .map_err(|e| e.to_string())?,
                );
            }
            let id = argument.id();
            let canonical_bytes = argument.canonical_len();
            let (catalog, server) = native::start(local.clone())?;
            let clock = Clock::start()?;
            let operation = (|| -> Result<_> {
                let init = Instant::now();
                let mut writer = Writer::new(catalog.clone(), provider.clone(), caps)?;
                let init_ns = init.elapsed().as_nanos() as u64;
                let offer = Instant::now();
                writer.offer(LogicalUse::RegularFileGraph, argument)?;
                let offer_ns = offer.elapsed().as_nanos() as u64;
                let finish = Instant::now();
                writer.finish_storage()?;
                let finish_ns = finish.elapsed().as_nanos() as u64;
                Ok((writer, init_ns, offer_ns, finish_ns))
            })();
            let timing = clock.stop()?;
            let (writer, init_ns, offer_ns, finish_ns) = match operation {
                Ok(value) => value,
                Err(error) => return failed(receipt, timing, error, &provider, Some(&catalog)),
            };
            let mut metrics = model::writer_metrics(&writer);
            metrics.put("provider", model::provider_metrics(&provider)?);
            metrics.put("native", native::metrics(&catalog)?);
            metrics.put("candidate", Json::Null);
            metrics.put("read", Json::Null);
            receipt.put("metrics", metrics);
            receipt.put("save_id", writer.save as u64);
            receipt.put("object_id", phase6_live_probe::minio::hex(id.as_bytes()));
            receipt.put("canonical_argument_bytes", canonical_bytes);
            receipt.put(
                "argument_contract",
                "deterministic sealed canonical C2 API argument in memory before operation",
            );
            receipt.put("ack_scope", "save_ready");
            receipt.put("writer_init_ns", init_ns);
            receipt.put("writer_offer_ns", offer_ns);
            receipt.put("writer_finish_ns", finish_ns);
            merge(&mut receipt, timing);
            drop(writer);
            drop(catalog);
            native::finish(server)?;
        }
        "metadata-local" | "metadata-native" | "payload-full-read" | "payload-prefix-read" => {
            let name = match row {
                "metadata-local" | "metadata-native" => "pool0",
                "payload-full-read" => "whole0",
                _ => "whole1",
            };
            let id = model::fixture_id(args, name)?;
            let usage = if name == "pool0" {
                LogicalUse::MetadataGraph
            } else {
                LogicalUse::RegularFileGraph
            };
            let pair = if row != "metadata-local" {
                Some(native::start(local.clone())?)
            } else {
                None
            };
            let catalog: Arc<dyn Catalog> = if let Some((c, _)) = &pair {
                c.clone()
            } else {
                local.clone()
            };
            let clock = Clock::start()?;
            let operation = (|| -> Result<_> {
                let scope = catalog.capture(None)?;
                let mut owner = ReadOwner::new()?;
                let access = Access {
                    catalog: catalog.as_ref(),
                    provider: &provider,
                    scope,
                    logical_use: usage,
                };
                let values = owner.read(&access, &caps, &[id])?;
                Ok((owner, values))
            })();
            let timing = clock.stop()?;
            let (owner, values) = match operation {
                Ok(v) => v,
                Err(e) => {
                    return failed(
                        receipt,
                        timing,
                        e,
                        &provider,
                        pair.as_ref().map(|(c, _)| c.as_ref()),
                    )
                }
            };
            let native_metrics = if let Some((c, _)) = &pair {
                native::metrics(c)?
            } else {
                Json::Null
            };
            let metrics = object([
                ("read", model::read_metrics(&owner)),
                ("provider", model::provider_metrics(&provider)?),
                ("native", native_metrics),
                ("writer", Json::Null),
                ("delta", Json::Null),
                ("pool", Json::Null),
                ("candidate", Json::Null),
            ]);
            receipt.put("metrics", metrics);
            receipt.put("object_id", phase6_live_probe::minio::hex(id.as_bytes()));
            receipt.put(
                "canonical_returned_bytes",
                values.iter().map(|b| b.len() as u64).sum::<u64>(),
            );
            receipt.put("ack_scope", "authenticated_canonical_read");
            merge(&mut receipt, timing);
            drop(catalog);
            if let Some((c, server)) = pair {
                drop(c);
                native::finish(server)?;
            }
        }
        "candidate-hydration" => {
            let clock = Clock::start()?;
            let operation = (|| -> Result<_> {
                let scope = local.capture(None)?;
                let mut after = 0u64;
                let mut rows = 0u64;
                loop {
                    let page =
                        local.candidate_page(scope, PlacementDomain::FilePayload, after, 128)?;
                    if page.is_empty() {
                        break;
                    }
                    rows += page.len() as u64;
                    after = page.last().ok_or("candidate page")?.stamp;
                }
                Ok((rows, after))
            })();
            let timing = clock.stop()?;
            let (rows, after) = match operation {
                Ok(v) => v,
                Err(e) => return failed(receipt, timing, e, &provider, None),
            };
            let w = local.candidate_snapshot_work()?;
            let metrics = object([
                (
                    "candidate",
                    object([
                        ("rows", rows.into()),
                        ("last_stamp", after.into()),
                        ("builds", w.builds.into()),
                        ("slot_lookups", w.slot_lookups.into()),
                        ("pages", w.pages.into()),
                        ("vm_steps", w.vm_steps.into()),
                        ("page_vm_steps", w.page_vm_steps.into()),
                        ("page_fullscan_steps", w.page_fullscan_steps.into()),
                        ("page_sorts", w.page_sorts.into()),
                        ("temp_pages", w.temp_pages.into()),
                        ("temp_page_bytes", w.temp_page_bytes.into()),
                    ]),
                ),
                ("provider", model::provider_metrics(&provider)?),
                ("native", Json::Null),
                ("read", Json::Null),
                ("writer", Json::Null),
                ("delta", Json::Null),
                ("pool", Json::Null),
            ]);
            receipt.put("metrics", metrics);
            receipt.put("ack_scope", "completed_candidate_snapshot");
            merge(&mut receipt, timing);
        }
        "metadata-transfer" => {
            let body = std::fs::read(args.root.join("transfer.pack")).map_err(|e| e.to_string())?;
            let digest_text = std::fs::read_to_string(args.root.join("transfer.sha256"))
                .map_err(|e| e.to_string())?;
            let hash = model::parse_digest(digest_text.trim())?;
            // Declared setup: local save allocation, no native connection. The registered
            // timed operation is register_body's normal ready-body ACK, not save/C5 close.
            let setup = Instant::now();
            let save = local.begin_save()?;
            let setup_ns = setup.elapsed().as_nanos() as u64;
            let (c, server) = native::start(local.clone())?;
            let clock = Clock::start()?;
            let operation = c
                .register_body(PlacementDomain::Metadata, save, hash, Some(&body))
                .map_err(|e| e.to_string());
            let timing = clock.stop()?;
            let order = match operation {
                Ok(v) => v,
                Err(e) => return failed(receipt, timing, e, &provider, Some(&c)),
            };
            receipt.put("save_id", save as u64);
            receipt.put("body_order", order as u64);
            receipt.put("body_argument_bytes", body.len());
            receipt.put("body_argument_digest", digest_text.trim());
            receipt.put("transfer_groups", 32u64);
            receipt.put("transfer_page_calls", body.len().div_ceil(8192));
            receipt.put("untimed_save_allocation_ns", setup_ns);
            receipt.put("ack_scope", "body_ready_private_save0_retained_for_proof");
            receipt.put(
                "metrics",
                object([
                    ("native", native::metrics(&c)?),
                    ("provider", model::provider_metrics(&provider)?),
                    ("read", Json::Null),
                    ("candidate", Json::Null),
                    ("writer", Json::Null),
                    ("delta", Json::Null),
                    ("pool", Json::Null),
                ]),
            );
            merge(&mut receipt, timing);
            drop(c);
            native::finish(server)?;
        }
        _ => return Err("unregistered perf route".into()),
    }
    receipt.put(
        "sql_bytes",
        std::fs::metadata(&args.store)
            .map_err(|e| e.to_string())?
            .len(),
    );
    receipt.put(
        "catalog_route",
        if matches!(row, "metadata-local" | "candidate-hydration") {
            "local StrictCatalog"
        } else {
            "authenticated NativeCatalog host loopback, first connection inside operation"
        },
    );
    receipt.put("provider_custody", "OWNED_RETAINED");
    Ok(receipt)
}
fn merge(target: &mut Json, source: Json) {
    if let (Json::Map(t), Json::Map(s)) = (target, source) {
        t.extend(s)
    }
}
fn failed(
    mut receipt: Json,
    timing: Json,
    error: String,
    provider: &phase6_live_probe::minio::Minio,
    native: Option<&phase6_live_probe::strict_remote::NativeCatalog>,
) -> Result<Json> {
    merge(&mut receipt, timing);
    receipt.put("status", "FAIL");
    receipt.put("error", error);
    receipt.put(
        "metrics",
        object([
            ("provider", model::provider_metrics(provider)?),
            (
                "native",
                match native {
                    Some(c) => native::metrics(c)?,
                    None => Json::Null,
                },
            ),
            ("writer", Json::Null),
            ("delta", Json::Null),
            ("pool", Json::Null),
            ("candidate", Json::Null),
            ("read", Json::Null),
        ]),
    );
    receipt.put(
        "partial_metrics_reason",
        "operation failed before complete result; available clocks and request statistics retained",
    );
    receipt.put(
        "service_cleanup",
        "process exit after operation error; no resend or speculative mutation",
    );
    Ok(receipt)
}
