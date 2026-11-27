//! `store.doctor`: one Store's checks without the whole-Store data scan,
//! then a shallow check of each of its projects. A PostgreSQL Store is
//! checked from this computer, then from each machine attached to it, or
//! from `--machine` alone (which must be attached).
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, opt, Call, PROGRESS};
use crate::dispatch::Context;
use crate::doctor::{self, FORM};
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;
use {layerfs_protocol::StoreKind, layerfs_store::admin::record::store::StoreRecord};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "doctor",
    &[a("store", Name)],
    &[opt("machine", Name), PROGRESS],
    "check a Store, then each of its projects",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = args.get_text("store").unwrap_or("");
    let machine = args.get_text("machine").map(str::to_owned);
    if let Some(machine) = &machine {
        // a sqlite Store lives on one machine: no other dials it
        let record = StoreRecord::read(&context.home, store)?;
        match record.as_ref().and_then(|record| record.kind().ok()) {
            Some(StoreKind::Sqlite) => {
                return Err(refused(
                    "InvalidInput",
                    format!(
                        "--machine {machine} probes a PostgreSQL Store's connection; {store} is a sqlite Store, which only its own machine reaches"
                    ),
                ))
            }
            Some(StoreKind::Postgresql)
                if !record.as_ref().is_some_and(|r| r.is_attached(machine)) =>
            {
                return Err(layerfs_store::admin::with_users::not_attached(store, machine));
            }
            _ => {}
        }
    }
    let options = Options {
        machine,
        ..Options::default()
    };
    doctor::run(context, Kind::Store, store, options)
}
