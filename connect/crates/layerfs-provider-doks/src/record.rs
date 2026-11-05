//! The provider label and route facts a DOKS host writes beside the machine
//! record.  Core still owns `machine/<name>/record.json`; the adapter never
//! reads or writes the home.  `kind` is the isolation class only; the
//! provider name is data, and the constructor core routes by is `platform`.

use crate::env::DoksEnv;
use layerfs_sandbox::{ExecMode, IsolationClass, KernelScope};

/// This adapter's provider name.
pub const PROVIDER: &str = "doks";
/// The machine constructor the adapter reuses.
pub const PLATFORM: &str = "ssh";

/// The values a host merges into machine `name`'s record for a DOKS machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFields {
    /// `doks`, this adapter's data label.
    pub provider: &'static str,
    /// `ssh`, the built-in constructor.
    pub platform: &'static str,
    /// `remote`, the isolation class (`Machine.kind`).
    pub kind: &'static str,
    /// `remote`, the same class under its #551 name.
    pub isolation_class: &'static str,
    /// `worker`.
    pub exec_mode: &'static str,
    /// `separate-host`.
    pub kernel: &'static str,
    pub namespace: String,
    pub pod: String,
    pub pvc: String,
    pub data_root: String,
    pub ssh_host: String,
    pub ssh_user: String,
    pub ssh_port: u16,
}

impl RecordFields {
    /// The fields as name/value text, in record order, for a host to merge.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("provider", self.provider.to_owned()),
            ("platform", self.platform.to_owned()),
            ("kind", self.kind.to_owned()),
            ("isolation_class", self.isolation_class.to_owned()),
            ("exec_mode", self.exec_mode.to_owned()),
            ("kernel", self.kernel.to_owned()),
            ("namespace", self.namespace.clone()),
            ("pod", self.pod.clone()),
            ("pvc", self.pvc.clone()),
            ("data_root", self.data_root.clone()),
            ("ssh_host", self.ssh_host.clone()),
            ("ssh_user", self.ssh_user.clone()),
            ("ssh_port", self.ssh_port.to_string()),
        ]
    }
}

/// The record fields a DOKS machine declares, from the parsed environment.
pub fn record_fields(env: &DoksEnv) -> RecordFields {
    RecordFields {
        provider: PROVIDER,
        platform: PLATFORM,
        kind: IsolationClass::Remote.as_str(),
        isolation_class: IsolationClass::Remote.as_str(),
        exec_mode: ExecMode::Worker.as_str(),
        kernel: KernelScope::SeparateHost.as_str(),
        namespace: env.namespace.clone(),
        pod: env.pod.clone(),
        pvc: env.pvc.clone(),
        data_root: env.data_root.clone(),
        ssh_host: env.ssh_host.clone(),
        ssh_user: env.ssh_user.clone(),
        ssh_port: env.ssh_port,
    }
}
