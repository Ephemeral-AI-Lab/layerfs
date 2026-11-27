//! The steps of a PostgreSQL connection, as the Store doctor records them
//! (`store.connection.<step>`) and a reach refusal names them (`step`).
//! The one place that decides which step a connection the server or the
//! network refused stopped at: the doctor's connection check and every
//! status or reach check's connect both ask it.

/// One step of a PostgreSQL connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionStep {
    /// The address, its options or the saved login, before any dial.
    Configuration,
    /// Name resolution and the socket: no server answered.
    Transport,
    /// The TLS the address asks for.
    Tls,
    /// The server refused the login.
    Authentication,
    /// The server has no such database.
    Database,
    /// The Store's schema and the role's privileges in it.
    Schema,
    /// The Store's id in the schema.
    Identity,
}

impl ConnectionStep {
    /// Its word: `configuration`, `transport`, `tls`, `authentication`,
    /// `database`, `schema` or `identity`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Transport => "transport",
            Self::Tls => "tls",
            Self::Authentication => "authentication",
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Identity => "identity",
        }
    }

    /// The doctor's observation id for it, `store.connection.<word>`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Configuration => "store.connection.configuration",
            Self::Transport => "store.connection.transport",
            Self::Tls => "store.connection.tls",
            Self::Authentication => "store.connection.authentication",
            Self::Database => "store.connection.database",
            Self::Schema => "store.connection.schema",
            Self::Identity => "store.connection.identity",
        }
    }

    /// The step a connection that was not made stopped at, by the
    /// SQLSTATE the server answered it with (`None`: no server answered):
    /// class 28 is the login, `3D000` the database, anything else the
    /// transport. The driver makes DNS, TCP and TLS as one step, so a TLS
    /// failure is the transport's here.
    pub fn of_refused_connection(sqlstate: Option<&str>) -> Self {
        match sqlstate {
            Some(code) if code.starts_with("28") => Self::Authentication,
            Some("3D000") => Self::Database,
            _ => Self::Transport,
        }
    }
}
