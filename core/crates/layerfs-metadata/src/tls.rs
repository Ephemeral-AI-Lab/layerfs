//! Verified native TLS and decrypted protocol observation; no downgrade.
use crate::{
    config::{PgConfig, TlsProfile},
    wire::{Observed, Trace},
};
use layerfs_storage::port::MetadataError;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_postgres::tls::{MakeTlsConnect, TlsConnect};
pub(crate) struct Connector {
    inner: postgres_native_tls::TlsConnector,
    trace: Arc<Trace>,
}
pub(crate) fn connector(config: &PgConfig, trace: Arc<Trace>) -> Result<Connector, MetadataError> {
    let TlsProfile::Verified {
        ca_certificates,
        server_name,
    } = &config.tls
    else {
        return Err(MetadataError::Malformed);
    };
    let mut builder = native_tls::TlsConnector::builder();
    builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
    if let Some(path) = ca_certificates {
        let info = std::fs::metadata(path).map_err(|_| MetadataError::Malformed)?;
        if !info.is_file() || info.len() > 1024 * 1024 {
            return Err(MetadataError::Malformed);
        }
        let bytes = std::fs::read(path).map_err(|_| MetadataError::Malformed)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| MetadataError::Malformed)?;
        let mut certificates = 0;
        for block in text.split_inclusive("-----END CERTIFICATE-----") {
            if block.trim().is_empty() {
                continue;
            }
            if !block
                .trim_start()
                .starts_with("-----BEGIN CERTIFICATE-----")
                || !block.ends_with("-----END CERTIFICATE-----")
            {
                return Err(MetadataError::Malformed);
            }
            builder.add_root_certificate(
                native_tls::Certificate::from_pem(block.as_bytes())
                    .map_err(|_| MetadataError::Malformed)?,
            );
            certificates += 1;
        }
        if certificates == 0 {
            return Err(MetadataError::Malformed);
        }
    }
    let builder = builder.build().map_err(|_| MetadataError::Malformed)?;
    let mut factory = postgres_native_tls::MakeTlsConnector::new(builder);
    let inner = <postgres_native_tls::MakeTlsConnector as MakeTlsConnect<
        Observed<tokio::net::TcpStream>,
    >>::make_tls_connect(&mut factory, server_name)
    .map_err(|_| MetadataError::Malformed)?;
    Ok(Connector { inner, trace })
}
impl<S: AsyncRead + AsyncWrite + Unpin + Send + 'static> TlsConnect<S> for Connector {
    type Stream = Observed<postgres_native_tls::TlsStream<S>>;
    type Error = native_tls::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Stream, Self::Error>> + Send>>;
    fn connect(self, stream: S) -> Self::Future {
        Box::pin(async move {
            let stream =
                <postgres_native_tls::TlsConnector as TlsConnect<S>>::connect(self.inner, stream)
                    .await?;
            Ok(Observed::new(stream, self.trace, false, true))
        })
    }
}
