//! `store get --with live` (call surface 2.0, review fixes 1 and 23): a
//! read of the Store where it lives, starting nothing: a sqlite Store's
//! file through its machine (in this process for `native`, else its
//! worker), a PostgreSQL Store's server from this computer.
//!
//! The read gives the state: `blocked` first, with nothing asked, while a
//! sqlite Store's machine is not ready; `broken` `missing` when no file is
//! where the record says, `broken` `unreachable` when the server does not
//! answer, `busy` or `broken` `unresolved` from a compaction's own record
//! (read through the machine; a run live there is `busy` even when this
//! home's record holds it unresolved after a lost reply, spec v19),
//! `unknown` (with when LayerFS last knew) when the machine does not
//! answer, else the records-only state. The record is written only when what the read
//! found wrong ([`state::SEEN`]) changed, which moves the home's
//! generation; `observed_at` goes to the observation file, which does not.
//! A backup or a restore whose owner is gone with its outcome unknown is
//! settled here: a backup changes nothing in the Store and a restore
//! installs by one rename, so the read drops the `op` and writes what was
//! tried to the link's `attempt` ([`crate::get::ATTEMPT`]).
use crate::get::ATTEMPT;
use crate::intent;
use crate::place;
use crate::record::{self, Placement, StoreRecord, StoreRef};
use crate::sqlite::family::step_on;
use crate::sqlite::transfer::step::StoreStep;
use crate::state;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{now_ms, observed, paths, with_field};
use layerfs_home::op::{self, Standing};
use layerfs_home::state::{Reason, State};
use layerfs_persistence::compact_metrics::{self, CompactStatusState};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What a live read found.
pub struct Seen {
    /// The Store's record after the read (written when its state changed).
    pub record: StoreRecord,
    /// Its state as the read found it.
    pub state: State,
    /// When it was last observed: now when the Store answered.
    pub observed: Option<i64>,
    /// The `live` section: `{ holders, phases, behind, server_version,
    /// latency_ms }`.
    pub section: SdkValue,
}

/// What the Store's own place answered.
#[derive(Default)]
struct Found {
    /// What is wrong with it, if anything.
    wrong: Option<Reason>,
    /// Its machine or server did not answer.
    unanswered: bool,
    /// The compaction record beside it, if any (`report::compact_status`).
    compaction: SdkValue,
    /// Its history heads as it is, when read.
    heads: Option<String>,
    /// The PostgreSQL server's version and latency.
    server_version: SdkValue,
    latency_ms: SdkValue,
}

/// A sqlite Store's file through its machine: whether it is there, the
/// compaction record beside it and its heads. A machine whose worker does
/// not answer leaves the read unanswered; a worker too old to read heads
/// leaves them unknown.
fn sqlite(home: &Path, record: &StoreRecord, store_ref: &StoreRef) -> Found {
    let name = record.name();
    let mut on = match place::plan(home, store_ref) {
        Ok(on) => on,
        Err(_) => {
            return Found {
                unanswered: true,
                ..Found::default()
            }
        }
    };
    let stat = match step_on(home, &mut on, name, StoreStep::Stat) {
        Ok(stat) => stat,
        Err(_) => {
            return Found {
                unanswered: true,
                ..Found::default()
            }
        }
    };
    if stat.get("exists") != Some(&SdkValue::Bool(true)) {
        return Found {
            wrong: Some(Reason::Missing),
            ..Found::default()
        };
    }
    let compaction = step_on(home, &mut on, name, StoreStep::CompactOp).unwrap_or_default();
    let heads = step_on(home, &mut on, name, StoreStep::Heads).ok();
    let heads = heads.and_then(|value| value.get_text("heads").map(str::to_owned));
    Found {
        compaction,
        heads,
        ..Found::default()
    }
}

/// A PostgreSQL Store's server from this computer: its version and the
/// latency of one query, and its compaction's own row.
fn postgresql(record: &StoreRecord) -> Found {
    let status = crate::postgresql::connect::status(record);
    if status.get("reachable") != Some(&SdkValue::Bool(true)) {
        return Found {
            wrong: Some(Reason::Unreachable),
            ..Found::default()
        };
    }
    Found {
        compaction: crate::postgresql::connect::compact_status(record),
        server_version: status.get("serverVersion").cloned().unwrap_or_default(),
        latency_ms: status.get("latencyMs").cloned().unwrap_or_default(),
        ..Found::default()
    }
}

/// The state a compaction's own record gives the Store, if it holds one:
/// `busy` while its run lives, `broken` `unresolved` once one left it.
fn compacting(record: &StoreRecord, compaction: &SdkValue) -> Option<State> {
    match compaction.get_text("state")? {
        "busy" => Some(State::Busy {
            name: "compact".to_owned(),
            phase: compaction.get_text("phase").unwrap_or("").to_owned(),
            started_ms: compaction.get_int("started"),
        }),
        "unresolved" => Some(state::broken(record, Reason::Unresolved)),
        _ => None,
    }
}

/// Settles the backup or restore a gone owner left on Store `store` with
/// its outcome unknown: drops the `op` and writes what was tried to the
/// bucket's link as its `attempt`. `record` is the record as read; it is
/// read again under the Store's lock only when it holds such an operation.
/// Answers the record as it now is.
fn settle(home: &Path, store: &str, record: StoreRecord) -> Result<StoreRecord> {
    let unsettled = |record: &StoreRecord| {
        record.op().is_some_and(|op| {
            op.standing() == Standing::Unresolved && intent::settled_by_read(op.name())
        })
    };
    // read first without the lock: a read that finds nothing to settle
    // takes no lock and writes nothing
    if !unsettled(&record) {
        return Ok(record);
    }
    let _store = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    let Some(op) = record.op().filter(|_| unsettled(&record)) else {
        return Ok(record);
    };
    let mut settled = record.without(op::FIELD);
    let bucket = op.args().get_text("bucket").map(str::to_owned);
    if let Some((bucket, link)) = bucket.and_then(|b| settled.link(&b).map(|link| (b, link))) {
        let tried = SdkValue::map([
            ("call", SdkValue::text(intent::word(op.name()))),
            ("at", SdkValue::Int(now_ms())),
            (
                "error",
                SdkValue::text(format!(
                    "its outcome is not known: its process ended in phase {}",
                    op.phase()
                )),
            ),
        ]);
        let fields = with_field(link.to_record().clone(), ATTEMPT, tried);
        let link = crate::backup::record::StoreLink::from_record(fields);
        settled = settled.with_link(&bucket, &link);
    }
    settled.write(home)?;
    Ok(settled)
}

/// Writes what the read found wrong to Store `store`'s record when it
/// changed (`seen`: the reason word, none when the Store was found well);
/// `record` is the record as read, read again under the Store's lock only
/// when it differs. Answers the record as it now is.
fn remember(
    home: &Path,
    store: &str,
    record: StoreRecord,
    seen: Option<&str>,
) -> Result<StoreRecord> {
    if record.text(state::SEEN) == seen {
        return Ok(record);
    }
    let _store = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    if record.text(state::SEEN) == seen {
        return Ok(record);
    }
    let record = match seen {
        Some(word) => record.with(state::SEEN, SdkValue::text(word)),
        None => record.without(state::SEEN),
    };
    record.write(home)?;
    Ok(record)
}

/// Reads Store `record` where it lives (see the module's documentation).
pub fn read(home: &Path, record: StoreRecord, machines: state::Machines) -> Result<Seen> {
    let store = record.name().to_owned();
    let (holders, _) = crate::lease::read_holders(home, &store)?;
    let holders = holders.iter().map(|pid| SdkValue::Int(i64::from(*pid)));
    let holders = SdkValue::List(holders.collect());
    // what the records say first: a Store whose machine is not ready is
    // blocked, whatever its own record holds, and nothing is asked of a
    // machine that is not running
    let recorded = match state::placed(&record) {
        Some(machine) => match machines(home, machine)?.blocks("machine", machine) {
            Some(blocked) => blocked,
            None => state::recorded(home, &record, machines)?,
        },
        None => state::recorded(home, &record, machines)?,
    };
    let found = match (&recorded, record.store_ref()) {
        (State::Blocked(_), _) | (_, Err(_)) => None,
        // a 1.1.0 Store whose machine its record does not tell: nowhere to read
        (
            _,
            Ok(StoreRef::Sqlite {
                machine: Placement::Unknown,
                ..
            }),
        ) => None,
        (_, Ok(StoreRef::Postgres { .. })) => Some(postgresql(&record)),
        (_, Ok(store_ref)) => Some(sqlite(home, &record, store_ref)),
    };
    let section = |found: Option<&Found>, behind: SdkValue| {
        SdkValue::map([
            ("holders", holders.clone()),
            (
                "phases",
                found
                    .and_then(|found| found.compaction.get("phases").cloned())
                    .unwrap_or_default(),
            ),
            ("behind", behind),
            (
                "server_version",
                found.map_or(SdkValue::Null, |f| f.server_version.clone()),
            ),
            (
                "latency_ms",
                found.map_or(SdkValue::Null, |f| f.latency_ms.clone()),
            ),
        ])
    };
    let Some(found) = found else {
        let observed = observed::read(&paths::store_record(home, &store));
        let section = section(None, SdkValue::Null);
        return Ok(Seen {
            record,
            state: recorded,
            observed,
            section,
        });
    };
    if found.unanswered {
        let observed = observed::last_known(&paths::store_record(home, &store));
        return Ok(Seen {
            record,
            state: State::Unknown {
                observed_ms: observed,
            },
            observed,
            section: section(Some(&found), SdkValue::Null),
        });
    }
    // the Store answered: a gone owner's backup or restore is settled,
    // and what is wrong with it is remembered when it changed
    let now = now_ms();
    let _ = observed::write(&paths::store_record(home, &store), now);
    let record = settle(home, &store, record)?;
    let compaction = compacting(&record, &found.compaction);
    // a compaction's own record is observed by its bounded state label,
    // never by a Store name or phase path
    match &compaction {
        Some(State::Busy { .. }) => compact_metrics::record_status(CompactStatusState::Busy),
        Some(_) => compact_metrics::record_status(CompactStatusState::Unresolved),
        None => {}
    }
    let left = matches!(compaction, Some(State::Broken { .. }));
    let wrong = found.wrong.or(left.then_some(Reason::Unresolved));
    let record = remember(home, &store, record, wrong.map(Reason::word))?;
    // a run live where the Store lives wins over this home's record of it
    let state = match (state::held(&record), compaction) {
        (_, Some(busy @ State::Busy { .. })) => busy,
        (Some(held), _) => held,
        (None, _) => state::recorded(home, &record, machines)?,
    };
    let behind = match (&record.store_ref(), &found.heads) {
        (Ok(StoreRef::Postgres { .. }), _) | (_, None) => SdkValue::Null,
        (_, Some(heads)) => {
            let links = record.links();
            let behind = links
                .iter()
                .filter(|(_, link)| link.heads().as_deref() != Some(heads));
            SdkValue::List(behind.map(|(bucket, _)| SdkValue::text(bucket)).collect())
        }
    };
    Ok(Seen {
        record,
        state,
        observed: Some(now),
        section: section(Some(&found), behind),
    })
}
