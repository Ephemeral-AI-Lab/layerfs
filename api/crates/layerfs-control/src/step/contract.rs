//! The step-runtime contract (#555): one exec request, its durable id, the
//! status it can come to, the receipt a retry reads and the answer a cancel
//! gives. None of these types holds a process, a daemon or a Store handle:
//! the launcher port starts the work and the durable wait store keeps the
//! entry, so the contract stays a launcher-owned helper outside the Sandbox
//! compute plane.
use crate::common::outcome::refused;
use layerfs_bridge::control::SdkRefusal;

/// Longest idempotency key the runtime accepts, in bytes.
pub const MAXIMUM_KEY_BYTES: usize = 128;

/// One command to run asynchronously.
///
/// `key` is the caller's idempotency key: a retried request carries the same
/// key and never runs a second copy. `timeout_ms` is the longest the command
/// may run before it is `TimedOut`; `None` means no deadline, and `Some(0)`
/// is already due at start (useful for an explicit immediate deadline).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecRequest {
    /// The caller's idempotency key.
    pub key: String,
    /// The command to run.
    pub command: String,
    /// The run deadline in milliseconds, or `None` for no deadline.
    pub timeout_ms: Option<u64>,
}

impl ExecRequest {
    /// `key` and `command` as one request, checked before any effect.
    ///
    /// The key is 1 to [`MAXIMUM_KEY_BYTES`] bytes and the command is not
    /// empty; a request outside that is refused `InvalidInput`.
    pub fn new(
        key: impl Into<String>,
        command: impl Into<String>,
        timeout_ms: Option<u64>,
    ) -> Result<Self, SdkRefusal> {
        let key = key.into();
        let command = command.into();
        if key.is_empty() || key.len() > MAXIMUM_KEY_BYTES {
            return Err(refused(
                "InvalidInput",
                format!(
                    "an idempotency key is 1..={MAXIMUM_KEY_BYTES} bytes, not {} bytes",
                    key.len()
                ),
            ));
        }
        if key.contains('\0') {
            return Err(refused(
                "InvalidInput",
                "an idempotency key has no NUL byte",
            ));
        }
        if command.is_empty() {
            return Err(refused("InvalidInput", "a step command is not empty"));
        }
        if timeout_ms.is_some_and(|ms| ms > i64::MAX as u64) {
            return Err(refused(
                "InvalidInput",
                "a step deadline is at most i64::MAX milliseconds",
            ));
        }
        Ok(Self {
            key,
            command,
            timeout_ms,
        })
    }
}

/// One exec's durable identity, `exec-<16 lowercase hex>`.
///
/// The id is a string because it crosses process boundaries; a cancelled
/// wait can be addressed again by any later launcher, unlike an
/// `AbortSignal`. It names one JSON file, so only this exact shape parses.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecId(String);

impl ExecId {
    /// A fresh id.
    pub fn mint() -> Result<Self, SdkRefusal> {
        let bytes = layerfs_bridge::duplex::generate_keypair()
            .map_err(crate::common::outcome::failed)?
            .public;
        Ok(Self(format!(
            "exec-{}",
            layerfs_bridge::sdk_values::hex(&bytes[..8])
        )))
    }

    /// `text` as an id, or `InvalidInput`.
    pub fn parse(text: &str) -> Result<Self, SdkRefusal> {
        let rest = text.strip_prefix("exec-").ok_or_else(|| bad_id(text))?;
        if rest.len() != 16
            || !rest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(bad_id(text));
        }
        Ok(Self(text.to_owned()))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn bad_id(text: &str) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!("an exec id is exec-<16 lowercase hex digits>, not {text:?}"),
    )
}

impl std::fmt::Display for ExecId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What one exec has come to.
///
/// `Running` is the only status a later call can move on from. `Unknown`
/// means no durable entry names the id; `Unresolved` means an entry exists
/// but its outcome cannot be known (the command may have run, and it is
/// recorded, never replayed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecStatus {
    /// The launcher started it and no terminal outcome was observed yet.
    Running,
    /// It ended with exit code 0.
    Succeeded {
        /// The exit code.
        exit_code: i32,
    },
    /// It ended with a non-zero exit code.
    Failed {
        /// The exit code.
        exit_code: i32,
    },
    /// A cancellation was recorded before its deadline.
    Cancelled,
    /// Its deadline passed first. This is not a cancellation.
    TimedOut,
    /// A durable entry exists, but its outcome cannot be known.
    Unresolved,
    /// No durable entry exists for this id.
    Unknown,
}

impl ExecStatus {
    /// Whether no later call can move this status (everything but
    /// `Running`).
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// The exit code of `Succeeded` and `Failed`.
    pub fn exit_code(self) -> Option<i32> {
        match self {
            Self::Succeeded { exit_code } | Self::Failed { exit_code } => Some(exit_code),
            _ => None,
        }
    }

    /// The status as a call's result text: `running`, `succeeded`,
    /// `failed`, `cancelled`, `timed_out`, `unresolved` or `unknown`.
    pub fn as_text(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded { .. } => "succeeded",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Unresolved => "unresolved",
            Self::Unknown => "unknown",
        }
    }

    /// The status text the durable entry keeps.
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded { .. } => "succeeded",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timedout",
            Self::Unresolved => "unresolved",
            Self::Unknown => "unknown",
        }
    }

    /// The status a durable entry's `status` and `exit` fields mean.
    pub(crate) fn from_wire(status: &str, exit: Option<i32>) -> Result<Self, SdkRefusal> {
        let needs_exit = || match exit {
            Some(code) => Ok(code),
            None => Err(refused(
                "Failed",
                format!("a step entry says {status} with no exit code"),
            )),
        };
        match status {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded {
                exit_code: needs_exit()?,
            }),
            "failed" => Ok(Self::Failed {
                exit_code: needs_exit()?,
            }),
            "cancelled" => Ok(Self::Cancelled),
            "timedout" => Ok(Self::TimedOut),
            "unresolved" => Ok(Self::Unresolved),
            "unknown" => Ok(Self::Unknown),
            other => Err(refused(
                "Failed",
                format!("a step entry has an unknown status {other:?}"),
            )),
        }
    }
}

/// What `exec_async` answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Started {
    /// The durable id every later call uses.
    pub exec_id: ExecId,
    /// Whether an entry already named this idempotency key, so nothing was
    /// started a second time.
    pub reused: bool,
    /// The status as the start observed it: `Running` for a new exec, or
    /// the existing entry's status for a retry.
    pub status: ExecStatus,
}

/// One exec's durable receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecReceipt {
    /// The durable id.
    pub exec_id: ExecId,
    /// The idempotency key it was started with.
    pub key: String,
    /// The command it runs.
    pub command: String,
    /// Its deadline in milliseconds, or `None`.
    pub timeout_ms: Option<u64>,
    /// Its status.
    pub status: ExecStatus,
    /// When it started, in milliseconds since the Unix epoch.
    pub started: i64,
    /// When its entry last moved, in milliseconds since the Unix epoch.
    pub updated: i64,
}

/// What `cancel` answers. `Cancelled` and `TimedOut` are never the same
/// value: a deadline that passed first stays a timeout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelOutcome {
    /// The cancellation was recorded before the deadline.
    Cancelled,
    /// The deadline had already passed, so this was a timeout, not a
    /// cancellation.
    TimedOut,
    /// No cancellation was established: no entry, an entry already
    /// finished, or an outcome that cannot be known.
    Unknown,
}
