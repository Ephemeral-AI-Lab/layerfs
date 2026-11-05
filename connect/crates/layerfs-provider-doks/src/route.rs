//! The route a DOKS machine takes: the built-in ssh machine's [`Spec`], not
//! a new platform.  A caller passes the spec to [`layerfs_machine::connect`],
//! the same public call the `ssh` machine has; the adapter adds no branch.
//!
//! [`Spec`]: layerfs_machine::Spec

use crate::env::DoksEnv;
use crate::error::Error;
use layerfs_machine::{Bundle, Lasting, MachineError, Platform, Running, Spec, Tools};
use std::path::PathBuf;

/// A DOKS machine `name` as the built-in ssh adapter sees it.
///
/// `kind` is [`Platform::Ssh`]: core routes it through the existing ssh
/// machine, and no vendor enum is involved.  The provider label travels
/// separately, in [`crate::record_fields`].
pub fn spec(name: &str, work_dir: PathBuf, tools: Tools, env: &DoksEnv) -> Spec {
    Spec {
        name: name.to_owned(),
        kind: Platform::Ssh,
        vm_name: vm_name(env),
        cpus: 0,
        memory: 0,
        disk: 0,
        image: None,
        os_image: None,
        nested: false,
        host: None,
        ssh: Some(env.ssh_spec()),
        state: Lasting::default(),
        worker: None,
        work_dir,
        tools,
    }
}

/// [`spec`] from the process environment.
pub fn spec_from_env(name: &str, work_dir: PathBuf, tools: Tools) -> Result<Spec, Error> {
    let env = DoksEnv::from_env()?;
    Ok(spec(name, work_dir, tools, &env))
}

/// Connects the DOKS machine through the built-in ssh adapter.  This is the
/// public machine call; nothing in core knows the provider's name.
pub fn connect(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    layerfs_machine::connect(spec, bundle)
}

fn vm_name(env: &DoksEnv) -> String {
    let mut name: String = format!("doks-{}-{}", env.namespace, env.pod)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.truncate(63);
    while name.ends_with('-') {
        name.pop();
    }
    name
}
