//! Separate readonly full-byte/history proof; no SDK operations or guessed adoption.
use crate::{
    metadata_catalog::LocatorDb, minio::Minio, objects::Reader, proof_plan::Plan,
    scenario::Scenario,
};
use layerfs_content::{filesystem::FilesystemRootId, FilesystemRead, LogicalPath, ObjectId};
use layerfs_history::{
    catalog::HistoryCatalog,
    identity::{BranchId, CommitId},
    sqlite,
};
use layerfs_telemetry::timer::Timing;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
pub fn verify(out: &Path, config: &Path, scenario: Option<&Scenario>) -> Result<(), String> {
    let start = Instant::now();
    let plan = Plan::read(&out.join("proof-plan.bin"))?;
    if plan.scenario != scenario.map(|s| s.identity)
        || plan.roots.len() != scenario.map_or(2, |s| s.steps.len())
    {
        return Err("proof scenario binding/cardinality".into());
    }
    let text = std::fs::read_to_string(config).map_err(|e| e.to_string())?;
    let fields: Vec<_> = text.lines().collect();
    if fields.len() != 4 {
        return Err("proof provider config".into());
    }
    let s3 = Minio {
        authority: fields[0].into(),
        bucket: fields[1].into(),
        access: fields[2].into(),
        secret: fields[3].into(),
        stats: Arc::new(Mutex::new(Default::default())),
    };
    let reader = Reader::new(
        s3.clone(),
        Arc::new(LocatorDb::open_read_only(&out.join("objects.sqlite"), s3)?),
    );
    let history = sqlite::open_read_only(
        &out.join("history.sqlite"),
        format!("phase6-live-{}", fields[1]).as_bytes(),
        layerfs_sandbox::random::<32>().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(s) = scenario {
        for (root, step) in plan.roots.iter().zip(&s.steps) {
            let (files, bytes) = crate::scenario::verify(&reader, *root, &step.manifest)?;
            eprintln!("P6_MANIFEST step={} files={files} bytes={bytes}", step.name);
        }
    } else {
        let mut expected = vec![0; 4096];
        expected[17..23].copy_from_slice(b"phase6");
        for (i, root) in plan.roots.iter().enumerate() {
            if i == 1 {
                expected[100..106].copy_from_slice(b"SECOND")
            }
            let mut fs = FilesystemRead::new(
                &reader,
                FilesystemRootId(ObjectId::from_bytes(root).map_err(|e| e.to_string())?),
            )
            .map_err(|e| e.to_string())?;
            let path = LogicalPath::new("data").map_err(|e| e.to_string())?;
            let value = fs.resolve(&path).map_err(|e| e.to_string())?.value;
            let mut bytes = Vec::new();
            let (r, _) = Timing::disabled("independent semantic proof", |t| {
                layerfs_content::read_all_bounded(
                    &reader,
                    value.content_root,
                    4096,
                    &mut bytes,
                    t.child("read"),
                )
            });
            r.map_err(|e| e.to_string())?;
            if bytes != expected
                || fs.read_portable(&path).map_err(|e| e.to_string())?.mode != 0o644
            {
                return Err("literal smoke byte/mode oracle".into());
            }
            std::fs::write(out.join(format!("publication-{i}.data")), bytes)
                .map_err(|e| e.to_string())?;
        }
    }
    let snapshot = history
        .branch_snapshot(BranchId::from_bytes(plan.branch).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .ok_or("proof Branch absent")?;
    if snapshot.effective_root.as_bytes() != plan.roots.last().ok_or("proof roots absent")?
        || snapshot.branch.head_commit.map(|h| h.to_bytes()) != plan.head
    {
        return Err("proof current head/root".into());
    }
    let mut head = plan.head;
    for expected in plan.commits.iter().rev() {
        let actual = history
            .commit(
                CommitId::from_bytes(head.ok_or("proof head absent")?)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?
            .ok_or("proof history absent")?;
        if actual.id.to_bytes() != expected.id
            || actual.root.as_bytes() != &expected.root
            || actual.parent.map(|p| p.to_bytes()) != expected.parent
        {
            return Err("proof history parent/root/id".into());
        }
        head = actual.parent.map(|p| p.to_bytes());
    }
    if head.is_some() {
        return Err("proof earlier history".into());
    }
    let ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::write(out.join("proof.json"),format!("{{\"schema\":1,\"status\":\"PASS\",\"proof_ms\":{ms},\"canonical_reference\":\"NOT_RUN\",\"physical_resources\":\"NOT_RUN\",\"catalogs\":\"readonly\"}}\n")).map_err(|e|e.to_string())?;
    println!("readonly semantic/history proof PASS; proof_ms={ms}");
    Ok(())
}
