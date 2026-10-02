//! Separate retained-output proof, never called by the performance phase.
use super::{
    json::{object, Json},
    model::{self, Args, Result},
    prepare,
};
use phase6_live_probe::{
    minio::{digest, hex},
    strict_catalog::{LogicalUse, PlacementDomain, StrictCatalog},
    strict_read::{Access, ReadOwner},
};
use rusqlite::{Connection, OpenFlags};
fn need(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
struct Receipt {
    json: String,
    db: Connection,
}
impl Receipt {
    fn number(&self, path: &str) -> Result<u64> {
        let n: i64 = self
            .db
            .query_row(
                "SELECT json_extract(?1,?2)",
                rusqlite::params![self.json, path],
                |r| r.get(0),
            )
            .map_err(|e| format!("perf field {path}: {e}"))?;
        u64::try_from(n).map_err(|_| format!("negative perf field {path}"))
    }
    fn text(&self, path: &str) -> Result<String> {
        self.db
            .query_row(
                "SELECT json_extract(?1,?2)",
                rusqlite::params![self.json, path],
                |r| r.get(0),
            )
            .map_err(|e| format!("perf field {path}: {e}"))
    }
    fn eq(&self, path: &str, expected: u64) -> Result<()> {
        need(
            self.number(path)? == expected,
            &format!("retained performance work mismatch {path}"),
        )
    }
}
pub fn run(args: &Args) -> Result<Json> {
    let path = args.perf.as_ref().ok_or("verify --perf required")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let perf = Receipt {
        json: String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?,
        db: Connection::open_in_memory().map_err(|e| e.to_string())?,
    };
    need(perf.text("$.phase")? == "perf", "proof perf phase")?;
    need(
        perf.text("$.status")? == "PASS",
        "proof perf completed operation",
    )?;
    need(perf.text("$.case")? == args.case, "proof case identity")?;
    need(
        perf.text("$.operation_contract_id")? == "sp1-component-speed-v1",
        "proof operation contract",
    )?;
    need(
        perf.text("$.provider_bucket")?
            == std::env::var("SP1_MINIO_BUCKET").map_err(|e| e.to_string())?,
        "proof provider bucket identity",
    )?;
    let provider = model::provider()?;
    let catalog = StrictCatalog::open_read_only(&args.store)?;
    let observer = Connection::open_with_flags(&args.store, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let row = args.row()?;
    let mut proof = object([
        ("performance_sha256", hex(&digest(&bytes)).into()),
        ("canonical_bytes_verified", 0u64.into()),
        ("logical_bytes_verified", 0u64.into()),
        ("body_bytes_verified", 0u64.into()),
    ]);
    match row {
        "candidate-hydration" => {
            for (field, value) in [
                ("rows", 8192),
                ("last_stamp", 8192),
                ("builds", 1),
                ("slot_lookups", 8192),
                ("pages", 65),
                ("page_fullscan_steps", 0),
                ("page_sorts", 0),
            ] {
                perf.eq(&format!("$.metrics.candidate.{field}"), value)?;
            }
            let id = model::fixture_id(args, "whole0")?;
            let (count,min,max,bad):(i64,i64,i64,i64)=observer.query_row("SELECT count(*),min(stamp),max(stamp),sum(CASE WHEN slot!=stamp-1 OR object_id!=?1 OR signature!=?2 THEN 1 ELSE 0 END) FROM candidate_entries c JOIN saves s USING(save_id) WHERE domain=0 AND s.status=2",rusqlite::params![id.as_bytes().as_slice(),[0x53u8;32].as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|e|e.to_string())?;
            need(
                (count, min, max, bad) == (8192, 1, 8192, 0),
                "persisted synthetic ring exact slots/stamps/ID/signature",
            )?;
            perf.eq("$.metrics.provider.put_calls", 0)?;
            perf.eq("$.metrics.provider.get_calls", 0)?;
            proof.put("candidate_rows_verified", 8192u64);
        }
        "metadata-transfer" => {
            let save = i64::try_from(perf.number("$.save_id")?).map_err(|e| e.to_string())?;
            let order = i64::try_from(perf.number("$.body_order")?).map_err(|e| e.to_string())?;
            let (state,ready,domain):(i64,i64,i64)=observer.query_row("SELECT s.status,b.ready,b.domain FROM saves s JOIN bodies b USING(save_id) WHERE s.save_id=?1 AND b.body_order=?2",[save,order],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
            need(
                (state, ready, domain) == (0, 1, 1),
                "known-private transfer body ACK custody",
            )?;
            let (hash, body) = catalog.body(
                catalog.capture(Some(save))?,
                PlacementDomain::Metadata,
                order,
            )?;
            let body = body.ok_or("metadata body absent")?;
            let expected = prepare::transfer(args)?;
            need(
                body == expected,
                "transfer source groups/complete envelope parity",
            )?;
            need(hash == digest(&body), "transfer persisted body digest")?;
            let h =
                layerfs_storage::pack::layout::parse_header(&body).map_err(|e| e.to_string())?;
            need(h.group_count == 32, "transfer group count")?;
            for i in 0..h.group_count {
                layerfs_storage::pack::layout::group_view(&body, h, i)
                    .map_err(|e| e.to_string())?;
            }
            perf.eq("$.body_argument_bytes", body.len() as u64)?;
            perf.eq("$.transfer_groups", 32)?;
            perf.eq("$.transfer_page_calls", body.len().div_ceil(8192) as u64)?;
            perf.eq("$.metrics.native.connect_attempts", 1)?;
            perf.eq("$.metrics.native.calls[1]", 0)?;
            perf.eq("$.metrics.native.calls[7]", 1)?;
            perf.eq(
                "$.metrics.native.calls[8]",
                body.len().div_ceil(8192) as u64,
            )?;
            perf.eq("$.metrics.native.calls[9]", 1)?;
            proof.put("body_bytes_verified", body.len());
            proof.put("transfer_groups_verified", 32u64);
            proof.put("ack_scope", "body_ready_private_save0_retained");
        }
        _ => {
            let name = match row {
                "metadata-local" | "metadata-native" => "pool0",
                "small-exact-reuse" | "payload-full-read" => "whole0",
                _ => "whole1",
            };
            let id = model::fixture_id(args, name)?;
            need(
                perf.text("$.object_id")? == hex(id.as_bytes()),
                "perf sealed canonical ID",
            )?;
            let is_writer = [
                "small-new-full",
                "small-exact-reuse",
                "payload-prefix-write",
            ]
            .contains(&row);
            let save = if is_writer {
                Some(i64::try_from(perf.number("$.save_id")?).map_err(|e| e.to_string())?)
            } else {
                None
            };
            if let Some(save) = save {
                let status: i64 = observer
                    .query_row("SELECT status FROM saves WHERE save_id=?1", [save], |r| {
                        r.get(0)
                    })
                    .map_err(|e| e.to_string())?;
                need(status == 1, "writer persisted READYsave")?;
            }
            let usage = if name == "pool0" {
                LogicalUse::MetadataGraph
            } else {
                LogicalUse::RegularFileGraph
            };
            let access = Access {
                catalog: &catalog,
                provider: &provider,
                scope: catalog.capture(save)?,
                logical_use: usage,
            };
            let mut owner = ReadOwner::new()?;
            let got = owner.read(&access, &model::capacities()?, &[id])?;
            let actual = got.first().ok_or("proof canonical response")?;
            let expected = args.canonical(name)?;
            need(*actual == expected, "independent canonical byte comparison")?;
            proof.put("canonical_bytes_verified", actual.len());
            if name != "pool0" {
                let actual_raw = layerfs_content::whole_file_payload(actual)
                    .map_err(|e| e.to_string())?
                    .ok_or("whole logical value")?;
                let expected_raw = layerfs_content::whole_file_payload(&expected)
                    .map_err(|e| e.to_string())?
                    .ok_or("original logical value")?;
                need(actual_raw == expected_raw, "independent full logical bytes")?;
                proof.put("logical_bytes_verified", actual_raw.len());
            }
            let (puts, gets) = match row {
                "small-new-full" => (1, 0),
                "small-exact-reuse" => (0, 1),
                "payload-prefix-write" => (1, 1),
                "payload-full-read" => (0, 1),
                "payload-prefix-read" => (0, 2),
                _ => (0, 0),
            };
            perf.eq("$.metrics.provider.put_calls", puts)?;
            perf.eq("$.metrics.provider.get_calls", gets)?;
            if is_writer {
                let reuse = u64::from(row == "small-exact-reuse");
                let prefix = u64::from(row == "payload-prefix-write");
                perf.eq("$.metrics.writer.exact_reuses", reuse)?;
                perf.eq("$.metrics.writer.objects", 1 - reuse)?;
                perf.eq("$.metrics.delta.prepared_full", 1 - reuse)?;
                perf.eq("$.metrics.delta.prefix_selected", prefix)?;
                perf.eq("$.metrics.delta.trials", prefix)?;
                perf.eq("$.metrics.writer.payload_packs", 1 - reuse)?;
                let selected = catalog
                    .location(access.scope, PlacementDomain::FilePayload, id)?
                    .ok_or("writer output locator")?;
                let deps:i64=observer.query_row("SELECT count(*) FROM dependencies d JOIN locators l USING(locator_id) WHERE l.id=?1 AND l.domain=0 AND l.body_order=?2",rusqlite::params![id.as_bytes().as_slice(),selected.location.pack_id],|r|r.get(0)).map_err(|e|e.to_string())?;
                need(
                    deps == prefix as i64,
                    "persisted FULL/PREFIX dependency selection",
                )?;
            }
            if row == "metadata-native" {
                perf.eq("$.metrics.native.connect_attempts", 1)?;
                perf.eq("$.metrics.native.calls[0]", 1)?;
                need(
                    perf.number("$.metrics.native.calls[5]")? > 0,
                    "native SQL body route",
                )?;
            }
            if row != "metadata-local" {
                perf.eq("$.metrics.native.connect_attempts", 1)?;
                if is_writer {
                    perf.eq("$.metrics.native.calls[1]", 1)?;
                } else {
                    perf.eq("$.metrics.native.calls[0]", 1)?;
                }
            }
            proof.put("read", model::read_metrics(&owner));
        }
    }
    proof.put("provider", model::provider_metrics(&provider)?);
    proof.put("provider_custody", "OWNED_RETAINED");
    proof.put("master_immutability_scope","external family matches sealed master and sample identities before/after; this proof reads sample only");
    Ok(proof)
}
