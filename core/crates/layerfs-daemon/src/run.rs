//! Native process/connection assembly; filesystem semantics stay in Workspace.
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{connect, connect_until},
        pipe,
    },
    contract::*,
};
use layerfs_workspace::{OperationDelivery, WorkspaceHost};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
    time::{Duration, Instant},
};

struct ConnectionConfig {
    address: SocketAddr,
    selector: u32,
    private: [u8; 32],
    server: [u8; 32],
}
impl ConnectionConfig {
    fn from_env() -> Result<Self, Failure> {
        Ok(Self {
            address: crate::config::env("LAYERFS_ENDPOINT")?
                .to_socket_addrs()?
                .next()
                .ok_or(Code::InvalidInput)?,
            selector: crate::config::env("LAYERFS_SELECTOR")?
                .parse()
                .map_err(|_| Code::InvalidInput)?,
            private: pipe::key(&crate::config::env("LAYERFS_PRIVATE_KEY")?)?,
            server: pipe::key(&crate::config::env("LAYERFS_SERVER_KEY")?)?,
        })
    }
}

/// Run the configured process. No arguments retain the headless frame route.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let launch = crate::config::workspace(std::env::args().skip(1).collect())?;
    if launch.is_some() && !cfg!(target_os = "linux") {
        return Err(Failure::from(Code::Unsupported).into());
    }
    let connection = ConnectionConfig::from_env()?;
    let telemetry = crate::config::telemetry(2);
    let Some(launch) = launch else {
        let mut client = Client::new(connect(
            connection.address,
            connection.selector,
            &connection.private,
            &connection.server,
        )?)?;
        crate::headless::run(&mut client, &telemetry)?;
        return Ok(());
    };
    // Block before mount/transport threads inherit the mask. The main thread
    // owns shutdown and ordinary session draining, never an async signal handler.
    let mut signals = nix::sys::signal::SigSet::empty();
    signals.add(nix::sys::signal::Signal::SIGINT);
    signals.add(nix::sys::signal::Signal::SIGTERM);
    signals.thread_block()?;
    let delivery: OperationDelivery = Arc::new(move |request, input, output, deadline| {
        // Every call is one attempt. The existing server closes on any failure,
        // including PathNotFound; a later independent lookup needs a new session.
        let result = telemetry
            .recorder()
            .run(request.id, request.operation.label(), |_| {
                let mut client = Client::new(connect_until(
                    connection.address,
                    connection.selector,
                    &connection.private,
                    &connection.server,
                    deadline,
                )?)?;
                client.call_until(request, input, output, deadline)
            });
        telemetry.publish(result.1);
        result.0
    });
    let host = WorkspaceHost::new(launch.config, delivery)?;
    let workspace = host.attach(launch.attach, Instant::now() + Duration::from_secs(10))?;
    let mut mount = match layerfs_fuse::mount(&workspace, Instant::now() + Duration::from_secs(10))
    {
        Ok(mount) => mount,
        Err(error) => {
            // A failed mount may still own resources. close_clean refuses that
            // state; report both observations without guessing that it detached.
            if let Err(cleanup) = workspace.close_clean() {
                pipe::diagnostic(&format!("mount cleanup retained: {cleanup}\n"));
            }
            return Err(error.into());
        }
    };
    pipe::diagnostic(&format!(
        "workspace ready {}\n",
        workspace.mount_path().display()
    ));
    signals.wait()?;
    mount.unmount(Instant::now() + Duration::from_secs(10))?;
    workspace.close_clean()?;
    pipe::diagnostic("workspace closed\n");
    Ok(())
}
