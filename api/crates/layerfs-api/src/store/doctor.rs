//! `store.doctor`: one Store's checks without the whole-Store data scan,
//! then a shallow check of each of its projects. A PostgreSQL Store is also
//! probed from each machine that has a Sandbox of it, and from `--machine`.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::record::{StoreKind, StoreRecord};

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
        if record.and_then(|record| record.kind().ok()) == Some(StoreKind::Sqlite) {
            return Err(refused(
                "InvalidInput",
                format!(
                    "--machine {machine} probes a PostgreSQL Store's connection; {store} is a sqlite Store, which only its own machine reaches"
                ),
            ));
        }
    }
    let options = Options {
        machine,
        ..Options::default()
    };
    answer::run(context, Kind::Store, store, options)
}
