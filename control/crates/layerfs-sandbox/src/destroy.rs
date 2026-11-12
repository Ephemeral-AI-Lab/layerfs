//! `sandbox.destroy`: delete only a Sandbox's data plane, for an invisible
//! holder (a copied mount namespace) that exec records cannot see (#460).
//! Unlike `sandbox.close`, it asks the daemon nothing and settles no
//! command: the runtime is stopped and deleted and the folder goes with it.
use super::direct;
use super::family::named;
use super::record::{self, SandboxRecord};
use crate::host::Call;
use crate::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::op::Op;
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `sandbox.destroy(name)`.
#[derive(Clone, Debug, Default)]
pub struct DestroyRequest {
    /// The Sandbox's name.
    pub name: String,
}

/// How a destroy ended.
pub enum Destroyed {
    /// Its runtime stopped and its data plane deleted: what its side answered.
    Stopped(SdkValue),
    /// Its runtime was gone already; its record went too.
    WasGone,
}

/// Deletes only Sandbox `name`'s data plane. Its record and `op` intent
/// follow the same rules as `sandbox.close`; the daemon is never asked for a
/// Ready receipt and no command is settled, so an invisible holder does not
/// block it.
pub fn destroy(
    home: &Path,
    events: &mut Events<'_>,
    request: &DestroyRequest,
) -> Result<Destroyed> {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    if let Some(container) = named(&home, name)?.retired_container(&home)? {
        return Err(super::reopen::retired(name, &container));
    }
    let op = Op::begin("sandbox.destroy", "destroy", SdkValue::Null)?;
    let record = {
        let _lock = home::lock::take(&home)?;
        let _sandbox = record::lock(&home, name)?;
        let Some(record) = SandboxRecord::read(&home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        record.free()?;
        let record = record.with_op(&op);
        record.write(&home)?;
        record
    };
    let destroyed = on_machine(&home, events, &record, Call::Destroy, crate::dispatch::call);
    let _lock = home::lock::take(&home)?;
    // on `native` the Sandbox's own side removed the folder already
    let dir = record.dir(&home);
    let _sandbox = match dir.exists() {
        true => Some(record::lock(&home, name)?),
        false => None,
    };
    match destroyed {
        Ok(value) => {
            direct::release(&home, &record);
            super::create::remove_folder(&dir)?;
            Ok(Destroyed::Stopped(value))
        }
        // a Sandbox already gone loses its record too
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            direct::release(&home, &record);
            super::create::remove_folder(&dir)?;
            Ok(Destroyed::WasGone)
        }
        // a certain failure leaves the Sandbox as it was
        Err(refusal) => {
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.without_op().write(&home)?;
            }
            Err(refusal)
        }
    }
}
