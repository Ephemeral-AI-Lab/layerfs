//! SDK-driven host provisioning and persistent real-daemon benchmark control.
mod args;
mod events;
mod fixture;
mod runtime;
mod session;

use args::Args;
use events::Events;
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 15)] as char);
    }
    text
}

fn main() -> Result<()> {
    let args = Args::parse()?;
    let mut events = Events::new(&args.receipt)?;
    let scope = hex(&args.observation_scope);
    events.fields(
        "observation_scope",
        None,
        "fresh public per-channel scope; no daemon retained preference",
        &[("scope", scope)],
    )?;
    events.value("selection", &format!("mode={} image={} uid={} gid={} construction_workers=1 admission_eligible=false exploratory=true", args.mode, args.image, args.uid, args.gid))?;
    let result = match args.mode.as_str() {
        "provision" => fixture::provision(&args, &mut events),
        "serve" => session::serve(&args, &mut events),
        _ => Err("mode must be provision or serve".into()),
    };
    match result {
        Ok(()) => events.value("finished", "known successful harness completion"),
        Err(error) => {
            events.value(
                "failed",
                &format!("original_failure={error}; no replay or automatic teardown"),
            )?;
            Err(error)
        }
    }
}
