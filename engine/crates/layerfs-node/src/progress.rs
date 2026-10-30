//! Live progress of the runtime's whole-Store jobs (#506 E4). A push, pull,
//! clone or import call that names its runtime job (`job`) gets a progress
//! cell for the call, registered by that id while the call runs; the
//! native transfer reports its phases and bytes to it (at most every 250 ms
//! for bytes), and `runtime.sync.progress { job }` copies its sample. The
//! call is not changed otherwise: one attempt, no thread, no cancel.
use crate::job::Outcome;
use crate::registry::lock;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_service::remote::storage::progress::{self, Cell, Sample, INTERVAL};
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

static JOBS: LazyLock<Mutex<BTreeMap<String, Arc<Cell>>>> = LazyLock::new(Default::default);

/// Runs `work` reporting to the cell of the call's `job`, when it names one.
pub(crate) fn observed<T>(
    args: &SdkValue,
    work: impl FnOnce() -> Result<T, SdkRefusal>,
) -> Result<T, SdkRefusal> {
    let Some(job) = args.get_text("job").map(str::to_owned) else {
        return work();
    };
    let cell = Arc::new(Cell::new(INTERVAL));
    lock(&JOBS)?.insert(job.clone(), cell.clone());
    let result = progress::observe(cell, work);
    if let Ok(mut jobs) = lock(&JOBS) {
        jobs.remove(&job);
    }
    result
}

/// Starts `phase` of the call's job, if it has one (the binding's own steps).
pub(crate) fn begin(phase: &'static str) {
    progress::begin(phase, None);
}

fn shown(sample: &Sample) -> SdkValue {
    let number = |n: Option<u64>| n.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64));
    SdkValue::map([
        ("phase", sample.phase.map_or(SdkValue::Null, SdkValue::text)),
        ("bytes", number(sample.bytes)),
        ("total", number(sample.total)),
        (
            "pieces",
            sample.pieces.map_or(SdkValue::Null, |(done, of)| {
                SdkValue::map([
                    ("done", SdkValue::Int(done as i64)),
                    ("of", SdkValue::Int(of as i64)),
                ])
            }),
        ),
    ])
}

/// `runtime.sync.progress { home, job }`: `{ phase, bytes, total, pieces }`
/// of the job's running call, or null when none runs in this process.
pub(crate) fn get(args: &SdkValue) -> Outcome {
    let job = crate::registry::text(args, "job")?;
    let cell = lock(&JOBS)?.get(job).cloned();
    Ok(cell.map_or(SdkValue::Null, |cell| shown(&cell.sample())))
}
