use super::json::{object, Json};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use phase6_live_probe::{
    minio::{digest, hex, Minio},
    strict_read::ReadOwner,
    strict_writer::Writer,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
pub type Result<T> = std::result::Result<T, String>;
pub struct Args {
    pub phase: String,
    pub case: String,
    pub root: PathBuf,
    pub evidence: PathBuf,
    pub store: PathBuf,
    pub out: PathBuf,
    pub perf: Option<PathBuf>,
}
impl Args {
    pub fn parse() -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut args = std::env::args().skip(1);
        while let Some(k) = args.next() {
            if !k.starts_with("--") {
                return Err("expected named CLI flag".into());
            }
            let v = args.next().ok_or("missing CLI value")?;
            if values.insert(k, v).is_some() {
                return Err("duplicate CLI flag".into());
            }
        }
        let mut take = |k: &str| values.remove(k).ok_or_else(|| format!("missing {k}"));
        let phase = take("--phase")?;
        let case = take("--case")?;
        let root = take("--root")?.into();
        let evidence = take("--evidence")?.into();
        let store = take("--store")?.into();
        let out = take("--out")?.into();
        let perf = values.remove("--perf").map(PathBuf::from);
        if !values.is_empty() {
            return Err("unknown CLI flag".into());
        }
        let a = Self {
            phase,
            case,
            root,
            evidence,
            store,
            out,
            perf,
        };
        a.row()?;
        Ok(a)
    }
    pub fn row(&self) -> Result<&str> {
        let row = self
            .case
            .strip_prefix("sp1-speed-v1-")
            .ok_or("case prefix")?;
        if ![
            "metadata-local",
            "metadata-native",
            "small-new-full",
            "small-exact-reuse",
            "payload-prefix-write",
            "payload-full-read",
            "payload-prefix-read",
            "candidate-hydration",
            "metadata-transfer",
        ]
        .contains(&row)
        {
            return Err("unregistered case".into());
        }
        Ok(row)
    }
    pub fn old(&self) -> PathBuf {
        self.evidence.join("old-producer-v1")
    }
    pub fn canonical(&self, name: &str) -> Result<Vec<u8>> {
        std::fs::read(self.old().join(format!("{name}.canonical"))).map_err(|e| e.to_string())
    }
    pub fn argument(&self, name: &str) -> Result<FinalizedObject> {
        FinalizedObject::new(ObjectRole::WholeFile, self.canonical(name)?)
            .map_err(|e| e.to_string())
    }
}
pub fn fixture_id(args: &Args, name: &str) -> Result<ObjectId> {
    let t = std::fs::read_to_string(args.old().join("objects.tsv")).map_err(|e| e.to_string())?;
    let line = t
        .lines()
        .skip(1)
        .find(|l| l.split('\t').next() == Some(name))
        .ok_or("sealed fixture name missing")?;
    parse_id(line.split('\t').nth(1).ok_or("fixture ID")?)
}
pub fn parse_id(text: &str) -> Result<ObjectId> {
    let hash = parse_digest(text)?;
    ObjectId::from_bytes(&hash).map_err(|e| e.to_string())
}
pub fn parse_digest(text: &str) -> Result<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return Err("digest width".into());
    }
    let mut hash = [0; 32];
    for (i, x) in hash.iter_mut().enumerate() {
        *x = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(hash)
}
pub fn provider() -> Result<Minio> {
    let var = |n| std::env::var(n).map_err(|_| format!("private provider prerequisite {n}"));
    Ok(Minio {
        authority: var("SP1_MINIO_AUTHORITY")?,
        bucket: var("SP1_MINIO_BUCKET")?,
        access: var("SP1_MINIO_ACCESS")?,
        secret: var("SP1_MINIO_SECRET")?,
        stats: Arc::new(Mutex::new(Default::default())),
    })
}
pub fn capacities() -> Result<StorageCapacities> {
    StorageCapacities::from_policy(StoragePolicy::frozen_default()).map_err(|e| e.to_string())
}
pub fn file_asset(root: &Path, path: &Path) -> Result<Json> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(object([
        (
            "path",
            path.strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned()
                .into(),
        ),
        ("sha256", hex(&digest(&b)).into()),
        ("size", b.len().into()),
    ]))
}
#[cfg(unix)]
pub fn cpu() -> Result<(u64, u64)> {
    let mut v = std::mem::MaybeUninit::<libc::rusage>::uninit();
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, v.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let v = unsafe { v.assume_init() };
    let ns = |t: libc::timeval| -> Result<u64> {
        let seconds = u64::try_from(t.tv_sec).map_err(|_| "negative CPU seconds")?;
        let micros = u64::try_from(t.tv_usec).map_err(|_| "negative CPU microseconds")?;
        seconds
            .checked_mul(1_000_000_000)
            .and_then(|n| micros.checked_mul(1000).and_then(|m| n.checked_add(m)))
            .ok_or_else(|| "CPU clock overflow".into())
    };
    Ok((ns(v.ru_utime)?, ns(v.ru_stime)?))
}
#[cfg(not(unix))]
pub fn cpu() -> Result<(u64, u64)> {
    Err("required getrusage CPU clock unavailable on this platform".into())
}
pub struct Clock {
    start: Instant,
    cpu: (u64, u64),
}
impl Clock {
    pub fn start() -> Result<Self> {
        let cpu = cpu()?;
        Ok(Self {
            start: Instant::now(),
            cpu,
        })
    }
    pub fn stop(self) -> Result<Json> {
        let ns = self.start.elapsed().as_nanos() as u64;
        let c = cpu()?;
        Ok(object([
            ("operation_ns", ns.into()),
            (
                "cpu_user_ns",
                c.0.checked_sub(self.cpu.0)
                    .ok_or("CPU user clock decreased")?
                    .into(),
            ),
            (
                "cpu_system_ns",
                c.1.checked_sub(self.cpu.1)
                    .ok_or("CPU system clock decreased")?
                    .into(),
            ),
        ]))
    }
}
pub fn provider_metrics(p: &Minio) -> Result<Json> {
    let s = p.statistics()?;
    Ok(object([
        ("put_calls", s.put_calls.into()),
        ("get_calls", s.get_calls.into()),
        ("put_requested_bytes", s.put_requested_bytes.into()),
        ("get_received_bytes", s.get_received_bytes.into()),
    ]))
}
pub fn writer_metrics(w: &Writer) -> Json {
    let c = &w.counts;
    let d = &w.delta;
    let p = &w.pool_counts;
    object([
        (
            "writer",
            object([
                ("objects", c.objects.into()),
                ("exact_reuses", c.exact_reuses.into()),
                ("payload_packs", c.payload_packs.into()),
                ("metadata_packs", c.metadata_packs.into()),
                ("payload_bytes", c.payload_bytes.into()),
                ("metadata_bytes", c.metadata_bytes.into()),
                ("private_group_seals", c.private_group_seals.into()),
                ("peak_private_objects", c.peak_private_objects.into()),
                ("peak_private_canonical", c.peak_private_canonical.into()),
            ]),
        ),
        (
            "delta",
            object([
                ("prepared_full", d.prepared_full.into()),
                ("trials", d.trials.into()),
                ("prefix_selected", d.prefix_selected.into()),
                ("full_losses", d.full_losses.into()),
                ("no_candidate", d.no_candidate.into()),
                ("absent_candidates", d.absent_candidates.into()),
                ("ineligible_candidates", d.ineligible_candidates.into()),
                ("work_exceeded", d.work_exceeded.into()),
            ]),
        ),
        (
            "pool",
            object([
                ("leaves", p.leaves.into()),
                ("reused_values", p.reused_values.into()),
                ("new_values", p.new_values.into()),
                ("groups", p.groups.into()),
                ("delta_leaves", p.delta_leaves.into()),
                ("full_leaves", p.full_leaves.into()),
                ("trials", p.trials.into()),
                ("work_exceeded", p.work_exceeded.into()),
            ]),
        ),
    ])
}
pub fn read_metrics(r: &ReadOwner) -> Json {
    let c = &r.counters;
    let p = &c.pooled;
    object([
        ("objects", c.objects.into()),
        ("edges", c.edges.into()),
        ("encoded_bytes", c.encoded_bytes.into()),
        ("canonical_bytes", c.canonical_bytes.into()),
        ("max_depth", c.max_depth.into()),
        ("group_decodes", c.group_decodes.into()),
        (
            "pooled",
            object([
                ("leaf_requests", p.leaf_requests.into()),
                ("chain_edges", p.chain_edges.into()),
                ("physical_record_calls", p.physical_record_calls.into()),
                ("physical_group_decodes", p.physical_group_decodes.into()),
                (
                    "physical_group_decoded_bytes",
                    p.physical_group_decoded_bytes.into(),
                ),
                (
                    "physical_group_cache_hits",
                    p.physical_group_cache_hits.into(),
                ),
                ("value_group_decodes", p.value_group_decodes.into()),
                ("pack_fetches", p.pack_fetches.into()),
                ("pack_bytes", p.pack_bytes.into()),
            ]),
        ),
    ])
}
