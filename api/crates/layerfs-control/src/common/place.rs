//! Where a Store call runs (H §7.2, §7.7): in this process, or in the worker
//! of the machine that holds the Store.
use crate::common::home::{paths, read_record};
use crate::common::outcome::refused;
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// The worker of `machine`, started first if it is stopped, or `None` when
/// its Stores open in this process (H §7.2, step 6).
#[cfg(feature = "machine")]
pub(crate) fn endpoint(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::machine::worker::Endpoint>, SdkRefusal> {
    crate::machine::running(home, machine)
}

/// Without the machine adapters (the guest worker's build): `native` is in
/// process; any other machine's worker is the one its `worker.json` records.
#[cfg(not(feature = "machine"))]
pub(crate) fn endpoint(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::machine::worker::Endpoint>, SdkRefusal> {
    if machine == "native" {
        return Ok(None);
    }
    recorded(home, machine)
}

/// The worker of `machine` as its records say now, starting nothing: `None`
/// when its Stores open in this process, `Unavailable` when it has no
/// worker recorded. A recorded worker that is gone fails when called.
pub(crate) fn endpoint_now(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::machine::worker::Endpoint>, SdkRefusal> {
    if machine == "native" {
        return Ok(None);
    }
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::{HostOs, Platform, Route};
        let record = read_record(&paths::machine_record(home, machine))?
            .ok_or_else(|| refused("NotFound", format!("no machine named {machine}")))?;
        let kind = record.get_text("platform").and_then(Platform::parse);
        if kind.map(|kind| kind.route(HostOs::current())) == Some(Route::InProcess) {
            return Ok(None);
        }
    }
    recorded(home, machine)
}

/// The worker `machine`'s `worker.json` records.
fn recorded(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::machine::worker::Endpoint>, SdkRefusal> {
    let Some(value) = read_record(&paths::worker_record(home, machine))? else {
        return Err(refused(
            "Unavailable",
            format!("machine {machine} is not running"),
        ));
    };
    let address = value
        .get_text("address")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            refused(
                "Failed",
                format!("machine {machine}'s worker.json has no address"),
            )
        })?;
    Ok(Some(crate::machine::worker::Endpoint {
        address,
        secret: value.get_text("secret").unwrap_or("").to_owned(),
    }))
}
