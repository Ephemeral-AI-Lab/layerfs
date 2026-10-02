//! Untimed acquisition from immutable, independently sealed old C1/C2 objects.
use super::{
    json::{object, Json},
    model::{self, Args, Result},
};
use layerfs_content::ObjectRole;
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    pack::layout,
};
use phase6_live_probe::{
    minio::{digest, hex, Minio},
    strict_catalog::{
        CandidateRow, LogicalUse, PhysicalBase, PlacementDomain, Registration, StrictCatalog,
    },
    strict_read::{Access, ReadOwner},
};
use rusqlite::{Connection, OpenFlags};
use std::{collections::BTreeMap, path::Path};
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(bytes).map_err(|e| e.to_string())
}
pub fn transfer(args: &Args) -> Result<Vec<u8>> {
    let db = Connection::open_with_flags(
        args.evidence
            .join("strict-oracle-v2/domain-packs/metadata.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let source: Vec<u8> = db
        .query_row("SELECT data FROM object_packs WHERE pack_id=3", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    let h = layout::parse_header(&source).map_err(|e| e.to_string())?;
    if h.lane != layout::PackLane::Native || h.group_count != 1 {
        return Err("sealed transfer source lane/count".into());
    }
    let g = layout::group_view(&source, h, 0).map_err(|e| e.to_string())?;
    let raw = &source[g.start..g.end];
    let offset = layout::HEADER_LEN + 256 * layout::DIRECTORY_ENTRY_LEN;
    let length = offset + 32 * raw.len();
    let mut body = vec![0; length];
    body[..8].copy_from_slice(&layout::PACK_MAGIC);
    body[8..12].copy_from_slice(&layout::VERSION_NATIVE_STORED.to_le_bytes());
    body[12..16].copy_from_slice(&32u32.to_le_bytes());
    body[16..20].copy_from_slice(&(length as u32).to_le_bytes());
    for i in 0..32 {
        let at = offset + i * raw.len();
        let d = layout::HEADER_LEN + i * layout::DIRECTORY_ENTRY_LEN;
        body[d..d + 4].copy_from_slice(&(at as u32).to_le_bytes());
        body[d + 4..d + 8].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        body[d + 8..d + 12].copy_from_slice(&(g.decoded_length as u32).to_le_bytes());
        body[at..at + raw.len()].copy_from_slice(raw);
    }
    let header = layout::parse_header(&body).map_err(|e| e.to_string())?;
    for i in 0..32 {
        let view = layout::group_view(&body, header, i).map_err(|e| e.to_string())?;
        if &body[view.start..view.end] != raw {
            return Err("transfer source group copy".into());
        }
    }
    Ok(body)
}
fn import(
    args: &Args,
    catalog: &StrictCatalog,
    provider: &Minio,
    metadata: bool,
    prefix: bool,
    ring: bool,
) -> Result<Vec<Json>> {
    let db = Connection::open_with_flags(
        args.old().join("producer.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let save = catalog.begin_save()?;
    let mut orders = BTreeMap::new();
    let mut assets = Vec::new();
    let source_orders = if metadata {
        vec![7, 8]
    } else if prefix {
        vec![1, 2]
    } else {
        vec![1]
    };
    for original in source_orders {
        let bytes: Vec<u8> = db
            .query_row(
                "SELECT data FROM object_packs WHERE pack_id=?1",
                [original],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let hash = digest(&bytes);
        let domain = if metadata {
            PlacementDomain::Metadata
        } else {
            PlacementDomain::FilePayload
        };
        if !metadata {
            let dir = args.root.join("provider_objects");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join(format!("{}.pack", hex(&hash)));
            write(&path, &bytes)?;
            assets.push(model::file_asset(&args.root, &path)?);
            provider.put(&hash, &bytes)?;
        }
        let order =
            catalog.register_body(domain, save, hash, metadata.then_some(bytes.as_slice()))?;
        orders.insert(original, order);
    }
    if metadata {
        let (first,count,pack,group,hash):(u32,u32,i64,u32,Vec<u8>)=db.query_row("SELECT first_ordinal,count,pack_id,group_number,digest FROM metadata_value_groups WHERE pack_id=7",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|e|e.to_string())?;
        if catalog.reserve_ordinals(save, count as usize)? != first {
            return Err("sealed ordinal start".into());
        }
        catalog.insert_groups(
            catalog.capture(Some(save))?,
            save,
            &[ValueGroupRow {
                first_ordinal: first,
                count: count as usize,
                pack_id: orders[&pack],
                group_number: group as usize,
                digest: layerfs_content::ObjectId::from_bytes(&hash).map_err(|e| e.to_string())?,
            }],
        )?;
    }
    let names = if metadata {
        vec!["pool0"]
    } else if prefix {
        vec!["whole0", "whole1"]
    } else {
        vec!["whole0"]
    };
    let domain = if metadata {
        PlacementDomain::Metadata
    } else {
        PlacementDomain::FilePayload
    };
    let usage = if metadata {
        LogicalUse::MetadataGraph
    } else {
        LogicalUse::RegularFileGraph
    };
    for name in &names {
        let id = model::fixture_id(args, name)?;
        let (role,length,pack,group,record):(u8,u32,i64,u32,u32)=db.query_row("SELECT object_role,canonical_length,pack_id,group_number,record_number FROM objects WHERE object_id=?1",[id.as_bytes().as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|e|e.to_string())?;
        let base = if *name == "whole1" {
            Some(PhysicalBase {
                id: model::fixture_id(args, "whole0")?,
                domain,
                body: orders[&1],
            })
        } else {
            None
        };
        catalog.register(
            catalog.capture(Some(save))?,
            save,
            &[Registration {
                location: ObjectLocation {
                    object_id: id,
                    role: ObjectRole::from_code(role).map_err(|e| e.to_string())?,
                    canonical_length: length as usize,
                    pack_id: orders[&pack],
                    group_number: group as usize,
                    record_number: record as usize,
                },
                domain,
                logical_use: usage,
                references: vec![],
                base,
            }],
        )?;
    }
    if ring {
        let id = model::fixture_id(args, "whole0")?;
        for page in 0..64 {
            let rows = (page * 128..(page + 1) * 128)
                .map(|slot| CandidateRow {
                    slot,
                    stamp: u64::from(slot) + 1,
                    id,
                    signature: [0x53; 32],
                })
                .collect::<Vec<_>>();
            catalog.stage_candidates(catalog.capture(Some(save))?, save, domain, &rows)?;
        }
    }
    catalog.finish_storage(save)?;
    catalog.publish(save)?;
    for name in names {
        let mut reader = ReadOwner::new()?;
        let access = Access {
            catalog,
            provider,
            scope: catalog.capture(None)?,
            logical_use: usage,
        };
        let got = reader.read(
            &access,
            &model::capacities()?,
            &[model::fixture_id(args, name)?],
        )?;
        if got != vec![args.canonical(name)?] {
            return Err("prepared master independent canonical mismatch".into());
        }
    }
    Ok(assets)
}
pub fn prepare(args: &Args) -> Result<Json> {
    if args.store.exists() {
        return Err("fresh preparation SQL path required".into());
    }
    std::fs::create_dir_all(&args.root).map_err(|e| e.to_string())?;
    if let Some(parent) = args.store.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let provider = model::provider()?;
    provider.create_bucket()?;
    let catalog = StrictCatalog::create(&args.store)?;
    let row = args.row()?;
    let mut assets = match row {
        "metadata-local" => import(args, &catalog, &provider, true, false, false)?,
        "small-exact-reuse" => import(args, &catalog, &provider, false, false, false)?,
        "payload-prefix-read" => import(args, &catalog, &provider, false, true, false)?,
        "candidate-hydration" => import(args, &catalog, &provider, false, false, true)?,
        "small-new-full" => vec![],
        _ => return Err("prepare only one representative per declared master".into()),
    };
    let body = transfer(args)?;
    let transfer_path = args.root.join("transfer.pack");
    write(&transfer_path, &body)?;
    write(
        &args.root.join("transfer.sha256"),
        hex(&digest(&body)).as_bytes(),
    )?;
    assets.push(model::file_asset(&args.root, &transfer_path)?);
    assets.push(model::file_asset(
        &args.root,
        &args.root.join("transfer.sha256"),
    )?);
    let scope = catalog.capture(None)?;
    drop(catalog);
    let observer = Connection::open_with_flags(&args.store, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let unresolved: i64 = observer
        .query_row(
            "SELECT count(*) FROM saves WHERE status IN(0,1,3)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if unresolved != 0 {
        return Err("master unresolved save custody".into());
    }
    let ready: i64 = observer
        .query_row("SELECT count(*) FROM bodies WHERE ready=1", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    drop(observer);
    assets.push(model::file_asset(&args.root, &args.store)?);
    Ok(object([
        ("assets", Json::Array(assets)),
        ("publication", scope.publication.into()),
        ("ready_bodies", (ready as u64).into()),
        ("unresolved_saves", 0u64.into()),
        ("master_closed", true.into()),
        ("transfer_body_bytes", body.len().into()),
        ("transfer_groups", 32u64.into()),
        (
            "transfer_version",
            u64::from(layout::VERSION_NATIVE_STORED).into(),
        ),
        ("transfer_page_calls", body.len().div_ceil(8192).into()),
        (
            "metrics",
            object([("provider", model::provider_metrics(&provider)?)]),
        ),
    ]))
}
pub fn setup_provider(args: &Args) -> Result<Json> {
    let provider = model::provider()?;
    provider.create_bucket()?;
    let dir = args.root.join("provider_objects");
    let mut objects = 0u64;
    let mut bytes = 0u64;
    if dir.is_dir() {
        let mut paths = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .map(|p| p.map(|p| p.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            let hash = model::parse_digest(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("provider asset name")?,
            )?;
            let body = std::fs::read(path).map_err(|e| e.to_string())?;
            if digest(&body) != hash {
                return Err("provider asset digest mismatch".into());
            }
            provider.put(&hash, &body)?;
            objects += 1;
            bytes += body.len() as u64;
        }
    }
    Ok(object([
        ("provider_copy_objects", objects.into()),
        ("provider_copy_bytes", bytes.into()),
        ("copy_method", "sealed master asset byte copy".into()),
        (
            "metrics",
            object([("provider", model::provider_metrics(&provider)?)]),
        ),
        ("provider_custody", "OWNED_RETAINED".into()),
    ]))
}
