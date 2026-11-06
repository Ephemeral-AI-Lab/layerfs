//! `machine.connect`: an ssh host.
use super::bundle::bundle;
use super::create::{finish, new_name};
use super::record::{MachineRecord, CONNECT};
use super::spec::{self as record, refusal, update as update_record};
use super::status::home_id;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::context::Context;
use layerfs_home::home::{self, lock, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_platform::{self as adapters, Platform};

/// `machine.connect(name, host, { user, port, identity, known_hosts, root })`.
pub fn connect(context: &mut Context<'_>, args: &SdkValue, related: Related) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("").to_owned();
    let host = args.get_text("host").unwrap_or("");
    let known_hosts = match args.get_text("known_hosts") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".ssh/known_hosts"))
            .ok_or_else(|| refused("InvalidInput", "known_hosts is unset and so is HOME"))?,
    };
    let probe = adapters::probe(Platform::Ssh, &record::tools(&home)?);
    if !probe.ready {
        return Err(refusal(adapters::MachineError::missing(
            "ssh is not usable here",
            probe.missing,
        )));
    }
    let bundle = bundle()?;
    let id = home_id(&home)?;
    let ssh = SdkValue::map([
        ("host", SdkValue::text(host)),
        (
            "user",
            SdkValue::text(args.get_text("user").unwrap_or("root")),
        ),
        ("port", SdkValue::Int(args.get_int("port").unwrap_or(22))),
        (
            "identity",
            args.get("identity").cloned().unwrap_or_default(),
        ),
        (
            "known_hosts",
            SdkValue::text(known_hosts.display().to_string()),
        ),
        (
            "root",
            SdkValue::text(args.get_text("root").unwrap_or(adapters::DATA)),
        ),
    ]);
    // the claimed record holds the connect's `op` until it is recorded
    let op = Op::begin(CONNECT, "identify", SdkValue::Null)?;
    let record = MachineRecord::from_record(
        &name,
        SdkValue::map([
            ("name", SdkValue::text(&name)),
            ("platform", SdkValue::text("ssh")),
            ("ssh", ssh),
            ("created", SdkValue::Int(home::now_ms())),
        ]),
    )
    .with_op(&op);
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, &name)?;
        record.write(&home)?;
        // the accepted host key is checked strictly against the caller's file
        let copied = std::fs::copy(
            &known_hosts,
            paths::machine_dir(&home, &name).join("known_hosts"),
        );
        match copied
            .map_err(|e| refused("InvalidInput", format!("{}: {e}", known_hosts.display())))
            .and_then(|_| record::spec(&home, &id, &record))
        {
            Ok(spec) => spec,
            Err(refusal) => {
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
                return Err(refusal);
            }
        }
    };
    // a second machine on the same host is one kernel twice
    let identity = match adapters::host_identity(&spec) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
            return Err(refusal(error));
        }
    };
    let op = op.at("connect");
    {
        let _lock = lock::take(&home)?;
        for other in paths::names(&home.join("machine")).map_err(failed)? {
            let same = MachineRecord::read(&home, &other)?
                .and_then(|r| r.host_id().map(|h| h == identity));
            if other != name && same == Some(true) {
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
                return Err(refused(
                    "Exists",
                    format!("machine {other} is the same host"),
                ));
            }
        }
        update_record(&home, &name, |r| r.with_host_id(&identity).with_op(&op))?;
    }
    finish(&home, &name, op, adapters::connect(&spec, &bundle), related)
}
