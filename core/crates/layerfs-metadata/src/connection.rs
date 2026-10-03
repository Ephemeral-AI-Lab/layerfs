//! One I/O worker, one selected address, complete protocol/auth and hard deadlines.
use crate::{
    client::Message,
    config::{PgConfig, TlsProfile},
    error, tls,
    wire::{Observed, Trace},
};
use layerfs_storage::port::MetadataError;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        mpsc::{Receiver, SyncSender},
        Arc,
    },
};
use tokio::{net::TcpStream, runtime::Builder, time::timeout};
type Driver = Pin<Box<dyn Future<Output = Result<(), postgres::Error>> + Send>>;
async fn connect(
    config: &PgConfig,
    trace: Arc<Trace>,
) -> Result<(tokio_postgres::Client, Driver), MetadataError> {
    let address = tokio::net::lookup_host((config.host.as_str(), config.port))
        .await
        .map_err(|_| MetadataError::Uncertain)?
        .next()
        .ok_or(MetadataError::Malformed)?;
    let socket = TcpStream::connect(address)
        .await
        .map_err(|_| MetadataError::Uncertain)?;
    socket
        .set_nodelay(true)
        .map_err(|_| MetadataError::Uncertain)?;
    let plain = matches!(config.tls, TlsProfile::LocalPlain);
    let socket = Observed::new(socket, Arc::clone(&trace), true, plain);
    let config_driver = config.driver();
    let result = match config.tls {
        TlsProfile::LocalPlain => {
            let (client, connection) = config_driver
                .connect_raw(socket, tokio_postgres::NoTls)
                .await
                .map_err(|error| error::database(error, false))?;
            (client, Box::pin(connection) as Driver)
        }
        TlsProfile::Verified { .. } => {
            let connector = tls::connector(config, Arc::clone(&trace))?;
            let (client, connection) = config_driver
                .connect_raw(socket, connector)
                .await
                .map_err(|error| error::database(error, false))?;
            (client, Box::pin(connection) as Driver)
        }
    };
    trace.start().map_err(|_| MetadataError::Uncertain)?;
    Ok(result)
}
pub(crate) fn run(
    config: PgConfig,
    requests: Receiver<Message>,
    startup: SyncSender<Result<(), MetadataError>>,
    trace: Arc<Trace>,
) {
    let runtime = match Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(_) => {
            let _ = startup.send(Err(MetadataError::Uncertain));
            return;
        }
    };
    let connected = runtime.block_on(async {
        timeout(config.connect_timeout, connect(&config, Arc::clone(&trace))).await
    });
    let (client, driver) = match connected {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            let _ = startup.send(Err(error));
            return;
        }
        Err(_) => {
            let _ = startup.send(Err(MetadataError::Uncertain));
            return;
        }
    };
    let task = runtime.spawn(driver);
    if startup.send(Ok(())).is_err() {
        task.abort();
        return;
    }
    let mut usable = true;
    while let Ok(message) = requests.recv() {
        let Message::Run(job) = message else {
            break;
        };
        if !usable {
            let _ = job.reply.send(Err(MetadataError::Uncertain));
            continue;
        }
        let bindings = job
            .params
            .iter()
            .map(|param| param.binding())
            .collect::<Vec<_>>();
        let result = runtime.block_on(async {
            timeout(config.request_timeout, async {
                if job.bootstrap {
                    client.batch_execute(&job.sql).await.map(|_| Vec::new())
                } else {
                    client.query_typed(&job.sql, &bindings).await
                }
            })
            .await
        });
        let result = match result {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => Err(error::database(error, job.mutating)),
            Err(_) => Err(MetadataError::Uncertain),
        };
        if matches!(
            result,
            Err(MetadataError::Uncertain | MetadataError::Malformed)
        ) {
            usable = false;
            task.abort();
            runtime.block_on(tokio::task::yield_now());
        }
        if job.reply.send(result).is_err() {
            usable = false;
            task.abort();
            runtime.block_on(tokio::task::yield_now());
        }
    }
    task.abort();
}
