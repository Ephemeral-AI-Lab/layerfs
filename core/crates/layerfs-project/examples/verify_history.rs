//! Separate read-only retained-history corpus verifier.
//! A partial probe is always DIAGNOSTIC; this vehicle alone is not admission.
#![allow(dead_code)]
#[path = "history_support/producer.rs"]
mod producer;
#[path = "history_support/retained.rs"]
mod retained;
#[path = "history_support/support.rs"]
mod support;
#[path = "history_support/verify.rs"]
mod verify;
#[path = "history_support/workload.rs"]
mod workload;
use layerfs_content::ObjectId;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::Storage;
use workload::{
    history::{Corpus, Row},
    json,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !matches!(args.len(), 6..=8) {
        return Err(
            "verify_history CORPUS DB RECEIPT history-stride{10,3,1} complete|probe|probe-transition [durable|disposable] [independent-root-pins.json]".into(),
        );
    }
    let row = Row::from_id(&args[4]).ok_or("history selection")?;
    let probe_states = match args[5].as_str() {
        "probe" => Some(1),
        "probe-transition" => Some(2),
        "complete" => None,
        _ => return Err("operation mode".into()),
    };
    let selected_profile = match args.get(6).map(String::as_str).unwrap_or("durable") {
        "durable" => SqlitePersistenceProfile::Durable,
        "disposable" => SqlitePersistenceProfile::Disposable,
        _ => return Err("explicit durable/disposable profile required".into()),
    };
    let value = json::parse(&std::fs::read_to_string(&args[3])?)?;
    let child = value
        .get("run")
        .and_then(|run| run.get("child"))
        .ok_or("missing producer child receipt")?;
    let count = probe_states.unwrap_or_else(|| row.states());
    if child.get("selected_states").and_then(json::Value::as_i64) != Some(row.states() as i64)
        || child.get("states").and_then(json::Value::as_i64) != Some(count as i64)
    {
        return Err("producer state count mismatch".into());
    }
    let roots = child
        .get("roots")
        .and_then(json::Value::as_array)
        .ok_or("missing roots")?
        .iter()
        .map(|root| {
            let bytes =
                workload::digest::unhex(root.as_str().ok_or("root text")?).ok_or("root hex")?;
            ObjectId::from_bytes(&bytes).map_err(|_| "root identity")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if roots.len() != count {
        return Err("root count mismatch".into());
    }
    let profile_identity = match selected_profile {
        SqlitePersistenceProfile::Durable => "sqlite-wal-full-macos-fullfsync-v1",
        SqlitePersistenceProfile::Disposable => "sqlite-memory-off-macos-v1",
    };
    if child.get("profile_identity").and_then(json::Value::as_str) != Some(profile_identity) {
        return Err("producer effective profile identity mismatch".into());
    }
    let mut independent_pins = false;
    if let Some(path) = args.get(7) {
        let pins = json::parse(&std::fs::read_to_string(path)?)?;
        if pins.get("kind").and_then(json::Value::as_str) != Some("matched-phase4.5-root-pins-v1")
            || pins.get("source_commit").and_then(json::Value::as_str)
                != Some("7edddbdb8e8512627aed0ed42533ef099d802384")
        {
            return Err("independent reference provenance missing".into());
        }
        if pins.get("row").and_then(json::Value::as_str) != Some(row.id()) {
            return Err("independent root-pin selection mismatch".into());
        }
        let expected = pins
            .get("roots")
            .and_then(json::Value::as_array)
            .ok_or("independent roots missing")?;
        if expected.len() != row.states() {
            return Err("independent root-pin count mismatch".into());
        }
        for (root, pin) in roots.iter().zip(expected) {
            if pin.as_str() != Some(workload::digest::hex(root.as_bytes()).as_str()) {
                return Err("independent retained root mismatch".into());
            }
        }
        independent_pins = true;
    }
    if probe_states.is_none() {
        if !independent_pins {
            return Err("complete proof requires independent root pins".into());
        }
        let (bytes, objects) = match row {
            Row::Stride10 => (380_559_460, 51_689),
            Row::Stride3 => (589_480_854, 73_447),
            Row::Stride1 => (871_337_620, 104_618),
        };
        if child.get("canonical_bytes").and_then(json::Value::as_i64) != Some(bytes)
            || child.get("canonical_objects").and_then(json::Value::as_i64) != Some(objects)
        {
            return Err("complete canonical census mismatch".into());
        }
    }
    let corpus = Corpus::open(&std::path::PathBuf::from(&args[1]), row)?;
    let config = retained::config();
    let handles = Handles::open_read_only(
        PersistenceConfig::sqlite(&args[2]).with_sqlite_profile(selected_profile),
        &config.binding_key,
        config.cursor_key,
    )?;
    let custody = retained::verify(&handles.history, producer::scope_of(row), &roots)?;
    let storage = Storage::new(handles.storage.clone())?;
    let reader = storage.reader()?;
    let mut reuse = verify::Reuse::default();
    let mut paths = 0;
    let mut sampled = 0;
    let mut bytes = 0;
    for (position, root) in roots.iter().enumerate() {
        let oracle = corpus.oracle(&corpus.states()[position])?;
        if oracle.is_empty() {
            return Err("empty oracle".into());
        }
        let (p, s, b) = reuse.check(&reader, handles.storage.as_ref(), &oracle, *root)?;
        paths += p;
        sampled += s;
        bytes += b;
    }
    println!("{{\"status\":\"{}\",\"states\":{},\"custody_states\":{},\"paths\":{},\"sampled_content_paths\":{},\"authenticated_bytes\":{},\"sample_policy\":\"every-tenth-content-path-and-final\",\"independent_root_pins\":\"{}\",\"admission\":\"NOT_RUN\"}}",if probe_states.is_some(){"DIAGNOSTIC"}else{"CHECKED"},count,custody,paths,sampled,bytes,if independent_pins {"CHECKED"} else {"NOT_CHECKED"});
    Ok(())
}
