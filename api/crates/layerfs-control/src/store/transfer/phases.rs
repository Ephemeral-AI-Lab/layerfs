//! `store.move`: a sqlite Store's file from its machine to another, one
//! phase at a time (check, snapshot, send, verify, install, record, retire).
use crate::common::context::Context;
use crate::common::home::{self, with_field};
use crate::common::outcome::{refused, Outcome};
use crate::common::place::{self, Executor, Start};
use crate::machine::record::MachineRecord;
use crate::store::family::{ended, idle, in_progress, of_store, progress, record_of, settle, step};
use crate::store::record::StoreRef;
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `store.move`: the Store's file from its machine to `to`.
pub(crate) fn move_store(context: &mut Context<'_>, name: &str, to: &str) -> Outcome {
    let home = context.home.clone();
    crate::common::names::MachineName::parse(to)?;
    // check: names, the target machine, the Store idle
    progress(context, "check", 0, 0);
    let from = {
        let _lock = home::lock::take(&home)?;
        let record = record_of(&home, name)?;
        if let StoreRef::Postgres { .. } = record.store_ref()? {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it does not move"),
            ));
        }
        if MachineRecord::read(&home, to)?.is_none() {
            return Err(refused("NotFound", format!("no machine named {to}")));
        }
        let from = record.machine()?;
        if from == to {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is already on {to}"),
            ));
        }
        idle(&home, name, &record)?;
        let at = in_progress(
            "move",
            vec![("from", SdkValue::text(&from)), ("to", SdkValue::text(to))],
        );
        record.with("in_progress", at).write(&home)?;
        from
    };
    let result =
        at("check", settle(&home, name)).and_then(|_lease| phases(context, name, &from, to));
    if let Err((phase, refusal)) = &result {
        // past install both copies exist: a record phase that failed (its
        // lock wait expired, say) leaves the outcome open
        let uncertain = refusal.code == "Unknown" || *phase == "record";
        ended(&home, "store.move", name, phase, refusal, uncertain);
    }
    result.map_err(|(phase, mut refusal)| {
        refusal.message = format!("store move, {phase}: {}", refusal.message);
        refusal.detail = with_field(refusal.detail, "phase", SdkValue::text(phase));
        refusal
    })
}

type Phased<T> = std::result::Result<T, (&'static str, SdkRefusal)>;

fn at<T>(phase: &'static str, result: Result<T>) -> Phased<T> {
    result.map_err(|refusal| (phase, refusal))
}

fn phases(context: &mut Context<'_>, name: &str, from: &str, to: &str) -> Phased<SdkValue> {
    let home = context.home.clone();
    let mut source = at("check", place::on_machine(&home, from, Start::OnDemand))?;
    let mut target = at("check", place::on_machine(&home, to, Start::OnDemand))?;
    for executor in [&mut source, &mut target] {
        if let Executor::Worker(worker) = executor {
            at("check", worker.ready())?;
        }
    }
    let exists = at(
        "check",
        step(&home, &mut target, "stat", of_store(name, vec![])),
    )?;
    if matches!(exists.get("exists"), Some(SdkValue::Bool(true))) {
        return Err((
            "check",
            refused(
                "Exists",
                format!("machine {to} already holds a Store file named {name}"),
            ),
        ));
    }
    // snapshot
    progress(context, "snapshot", 0, 0);
    let snapshot = at(
        "snapshot",
        step(&home, &mut source, "snapshot", of_store(name, vec![])),
    )?;
    let total = snapshot.get_int("bytes").unwrap_or(0) as u64;
    // send, in pieces through this process
    let mut offset = 0u64;
    progress(context, "send", 0, total);
    loop {
        let piece = at(
            "send",
            step(
                &home,
                &mut source,
                "read",
                of_store(
                    name,
                    vec![
                        ("offset", SdkValue::Int(offset as i64)),
                        ("len", SdkValue::Int(CHUNK as i64)),
                    ],
                ),
            ),
        )?;
        let bytes = match piece.get("bytes") {
            Some(SdkValue::Bytes(bytes)) => bytes.clone(),
            _ => Vec::new(),
        };
        if bytes.is_empty() && offset > 0 {
            break;
        }
        let len = bytes.len() as u64;
        at(
            "send",
            step(
                &home,
                &mut target,
                "write",
                of_store(
                    name,
                    vec![
                        ("offset", SdkValue::Int(offset as i64)),
                        ("bytes", SdkValue::Bytes(bytes)),
                    ],
                ),
            ),
        )?;
        offset += len;
        progress(context, "send", offset, total);
        if len < CHUNK {
            break;
        }
    }
    // verify, install
    progress(context, "verify", 0, 0);
    let digest: Vec<(&str, SdkValue)> = ["bytes", "sha256", "heads"]
        .into_iter()
        .map(|field| (field, snapshot.get(field).cloned().unwrap_or_default()))
        .collect();
    at(
        "verify",
        step(&home, &mut target, "verify", of_store(name, digest)),
    )?;
    progress(context, "install", 0, 0);
    at(
        "install",
        step(&home, &mut target, "install", of_store(name, vec![])),
    )?;
    // record: the Store is on `to` from here on
    progress(context, "record", 0, 0);
    let last = SdkValue::map([
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("at", SdkValue::Int(home::now_ms())),
        ("bytes", SdkValue::Int(total as i64)),
        (
            "sha256",
            snapshot.get("sha256").cloned().unwrap_or_default(),
        ),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]);
    at(
        "record",
        (|| {
            let _lock = home::lock::take(&home)?;
            let record = record_of(&home, name)?.with("machine", SdkValue::text(to));
            let record = record.with("last_move", last.clone());
            record.with("in_progress", SdkValue::Null).write(&home)
        })(),
    )?;
    // retire the source's copy
    progress(context, "retire", 0, 0);
    at(
        "retire",
        step(&home, &mut source, "retire", of_store(name, vec![])),
    )?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("bytes", SdkValue::Int(total as i64)),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]))
}
