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
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::Storage;
use workload::{
    history::{Corpus, Row},
    json,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 6 {
        return Err(
            "verify_history CORPUS DB RECEIPT history-stride{10,3,1} complete|probe".into(),
        );
    }
    let row = Row::from_id(&args[4]).ok_or("history selection")?;
    let probe = match args[5].as_str() {
        "probe" => true,
        "complete" => false,
        _ => return Err("operation mode".into()),
    };
    let value = json::parse(&std::fs::read_to_string(&args[3])?)?;
    let child = value
        .get("run")
        .and_then(|run| run.get("child"))
        .ok_or("missing producer child receipt")?;
    let count = if probe { 1 } else { row.states() };
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
    let corpus = Corpus::open(&std::path::PathBuf::from(&args[1]), row)?;
    let config = retained::config();
    let handles = Handles::open_read_only(
        PersistenceConfig::sqlite(&args[2]),
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
    println!("{{\"status\":\"{}\",\"states\":{},\"custody_states\":{},\"paths\":{},\"sampled_content_paths\":{},\"authenticated_bytes\":{},\"sample_policy\":\"every-tenth-content-path-and-final\",\"independent_root_pins\":\"NOT_CHECKED\",\"admission\":\"NOT_RUN\"}}",if probe{"DIAGNOSTIC"}else{"CHECKED"},count,custody,paths,sampled,bytes);
    Ok(())
}
