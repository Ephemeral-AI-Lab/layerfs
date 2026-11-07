//! `machine.connect`: an ssh host.
use super::bundle::bundle;
use super::create::{finish, new_name};
use super::record::{MachineRecord, CONNECT};
use super::spec::{self as record, refusal, update as update_record};
use super::status::home_id;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, lock, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_platform::{self as adapters, Platform};
use std::path::{Path, PathBuf};

/// A `machine.connect` request. The name stays text: it is checked under
/// the home lock, after the host's tools.
pub struct MachineConnect {
    /// The new machine's name.
    pub name: String,
    /// The ssh host.
    pub host: String,
    /// Its user; absent, `root`.
    pub user: Option<String>,
    /// Its port; absent, 22.
    pub port: Option<i64>,
    /// The path of the ssh identity file.
    pub identity: Option<String>,
    /// The caller's known_hosts file; absent, `$HOME/.ssh/known_hosts`.
    pub known_hosts: Option<PathBuf>,
    /// Its data root; absent, the adapters' default.
    pub root: Option<String>,
}

/// `machine.connect(name, host, { user, port, identity, known_hosts, root })`.
pub fn connect(home: &Path, request: &MachineConnect, related: Related) -> Outcome {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    let host = request.host.as_str();
    let known_hosts = match &request.known_hosts {
        Some(path) => path.clone(),
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
            SdkValue::text(request.user.as_deref().unwrap_or("root")),
        ),
        ("port", SdkValue::Int(request.port.unwrap_or(22))),
        (
            "identity",
            request
                .identity
                .as_deref()
                .map(SdkValue::text)
                .unwrap_or_default(),
        ),
        (
            "known_hosts",
            SdkValue::text(known_hosts.display().to_string()),
        ),
        (
            "root",
            SdkValue::text(request.root.as_deref().unwrap_or(adapters::DATA)),
        ),
    ]);
    // the claimed record holds the connect's `op` until it is recorded
    let op = Op::begin(CONNECT, "identify", SdkValue::Null)?;
    let record = MachineRecord::from_record(
        name,
        SdkValue::map([
            ("name", SdkValue::text(name)),
            ("platform", SdkValue::text("ssh")),
            ("ssh", ssh),
            ("created", SdkValue::Int(home::now_ms())),
        ]),
    )
    .with_op(&op);
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, name)?;
        record.write(&home)?;
        // the accepted host key is checked strictly against the caller's file
        let copied = std::fs::copy(
            &known_hosts,
            paths::machine_dir(&home, name).join("known_hosts"),
        );
        match copied
            .map_err(|e| refused("InvalidInput", format!("{}: {e}", known_hosts.display())))
            .and_then(|_| record::spec(&home, &id, &record))
        {
            Ok(spec) => spec,
            Err(refusal) => {
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, name));
                return Err(refusal);
            }
        }
    };
    // a second machine on the same host is one kernel twice
    let identity = match adapters::host_identity(&spec) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, name));
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
                let _ = std::fs::remove_dir_all(paths::machine_dir(&home, name));
                return Err(refused(
                    "Exists",
                    format!("machine {other} is the same host"),
                ));
            }
        }
        update_record(&home, name, |r| r.with_host_id(&identity).with_op(&op))?;
    }
    finish(&home, name, op, adapters::connect(&spec, &bundle), related)
}
