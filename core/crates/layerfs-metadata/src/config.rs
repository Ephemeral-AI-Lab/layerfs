//! Explicit endpoint, binding, verified TLS and bounded operation inputs.
use layerfs_storage::port::MetadataError;
use std::{fmt, path::PathBuf, time::Duration};
/// The selected transport profile, with no opportunistic downgrade.
#[derive(Clone, Debug)]
pub enum TlsProfile {
    /// Explicit local service profile, without TLS.
    LocalPlain,
    /// Certificate-chain and hostname verification using system/provider roots.
    Verified {
        /// Optional PEM CA bundle added to the system trust roots.
        ca_certificates: Option<PathBuf>,
        /// Verified name, normally the endpoint host; useful with an explicit tunnel.
        server_name: String,
    },
}
/// One PostgreSQL service/schema connection profile.
#[derive(Clone)]
pub struct PgConfig {
    /// Single configured host; exactly one resolved address is attempted.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Existing database.
    pub database: String,
    /// LayerFS schema namespace, an ASCII identifier of at most 63 bytes.
    pub schema: String,
    /// Login identity, redacted by Debug.
    pub user: String,
    /// Authentication input, redacted by Debug.
    pub password: String,
    /// Explicit local or verified TLS profile.
    pub tls: TlsProfile,
    /// Whole connection bound, including DNS, TLS and authentication.
    pub connect_timeout: Duration,
    /// Whole wire-operation bound; expiration closes the driver and is uncertain.
    pub request_timeout: Duration,
    /// Server statement bound, passed in startup options rather than a SET query.
    pub statement_timeout: Duration,
}
impl fmt::Debug for PgConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgConfig")
            .field("endpoint", &"[configured]")
            .field("database", &self.database)
            .field("schema", &self.schema)
            .field("credentials", &"[redacted]")
            .field("tls", &self.tls)
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("statement_timeout", &self.statement_timeout)
            .finish()
    }
}
impl PgConfig {
    /// Reads the explicit service inputs; remote/cloud callers select verified TLS.
    pub fn from_env() -> Result<Self, MetadataError> {
        let required = |name| std::env::var(name).map_err(|_| MetadataError::Malformed);
        let host = required("LAYERFS_PG_HOST")?;
        let tls = match required("LAYERFS_PG_TLS")?.as_str() {
            "disabled" => TlsProfile::LocalPlain,
            "verified" => TlsProfile::Verified {
                ca_certificates: std::env::var_os("LAYERFS_PG_CA_CERTIFICATES").map(PathBuf::from),
                server_name: std::env::var("LAYERFS_PG_SERVER_NAME")
                    .unwrap_or_else(|_| host.clone()),
            },
            _ => return Err(MetadataError::Malformed),
        };
        let result = Self {
            host,
            port: required("LAYERFS_PG_PORT")?
                .parse()
                .map_err(|_| MetadataError::Malformed)?,
            database: required("LAYERFS_PG_DATABASE")?,
            schema: required("LAYERFS_PG_SCHEMA")?,
            user: required("LAYERFS_PG_USER")?,
            password: required("LAYERFS_PG_PASSWORD")?,
            tls,
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(2),
            statement_timeout: Duration::from_secs(1),
        };
        result.check()?;
        Ok(result)
    }
    pub(crate) fn check(&self) -> Result<(), MetadataError> {
        if self.host.is_empty()
            || self.host.chars().any(char::is_control)
            || self.port == 0
            || self.database.is_empty()
            || self.user.is_empty()
            || self.password.is_empty()
            || self.schema.is_empty()
            || self.schema.len() > 63
            || !self
                .schema
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !self.schema.as_bytes()[0].is_ascii_lowercase()
            || self.connect_timeout.is_zero()
            || self.request_timeout.is_zero()
            || self.statement_timeout.is_zero()
            || self.statement_timeout > self.request_timeout
        {
            return Err(MetadataError::Malformed);
        }
        if let TlsProfile::Verified { server_name, .. } = &self.tls {
            if server_name.is_empty() || server_name.chars().any(char::is_control) {
                return Err(MetadataError::Malformed);
            }
        }
        Ok(())
    }
    pub(crate) fn sql(&self, template: &str) -> String {
        template.replace("${schema}", &format!("\"{}\"", self.schema))
    }
    pub(crate) fn driver(&self) -> tokio_postgres::Config {
        let mut config = tokio_postgres::Config::new();
        config
            .user(&self.user)
            .password(&self.password)
            .dbname(&self.database)
            .application_name("layerfs-metadata")
            .options(format!(
                "-c statement_timeout={} -c TimeZone=UTC",
                self.statement_timeout.as_millis()
            ));
        config.ssl_mode(match self.tls {
            TlsProfile::LocalPlain => tokio_postgres::config::SslMode::Disable,
            TlsProfile::Verified { .. } => tokio_postgres::config::SslMode::Require,
        });
        config
    }
}
