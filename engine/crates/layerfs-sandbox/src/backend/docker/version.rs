//! The Engine API version a client speaks: the daemon's, up to the newest this client targets.
use super::{
    http::{failure, Mode},
    json::{once, Json},
    Docker,
};
use crate::{ErrorDiagnostic, RuntimeError, WireFailure};
/// The oldest Engine API minor version (1.N) this client speaks: Engine 28.0.
/// Before it, uploading the daemon fails (`getent unable to find entry "0:0"`:
/// the archive's `copyUIDGID` cannot resolve the container's `0:0` user).
const OLDEST: u32 = 48;
/// The newest Engine API minor version this client speaks.
const NEWEST: u32 = 54;
impl Docker {
    /// The minor version every request of this client and its clones uses. The
    /// first request reads the daemon's with one unversioned `GET /version`; a
    /// daemon older than [`OLDEST`] is refused before the request is sent. A
    /// daemon that does not answer it readably gets [`NEWEST`], as before
    /// negotiation, and its own refusal of that version reaches the caller.
    pub(super) fn minor(&self) -> Result<u32, Box<WireFailure>> {
        if let Some(minor) = self.minor.get() {
            return Ok(*minor);
        }
        let minor = match self.daemon_minor() {
            Some(daemon) if daemon < OLDEST => return Err(refusal(daemon)),
            Some(daemon) => daemon.min(NEWEST),
            None => NEWEST,
        };
        Ok(*self.minor.get_or_init(|| minor))
    }
    /// One read of the daemon's newest API minor version; never resent or retried.
    fn daemon_minor(&self) -> Option<u32> {
        let mut response = self
            .transmit(
                "GET",
                "/version",
                Mode::Close,
                "application/json",
                0,
                |_| Ok(()),
            )
            .ok()?;
        if response.status != 200 {
            return None;
        }
        let mut bits = 0;
        let mut minor = None;
        let mut j = Json::new(&mut response.body);
        j.object(|j, key| {
            if key.equals("ApiVersion") {
                once(&mut bits, 1)?;
                let version = j.string(true)?;
                minor = version
                    .text()
                    .strip_prefix("1.")
                    .and_then(|minor| minor.parse().ok())
                    .filter(|_| !version.overflow);
            } else {
                j.skip(1)?;
            }
            Ok(())
        })
        .ok()?;
        j.finish().ok()?;
        minor
    }
}
fn refusal(daemon: u32) -> Box<WireFailure> {
    let message = format!(
        "Engine API 1.{daemon} is older than the 1.{OLDEST} to 1.{NEWEST} this client speaks"
    )
    .into_bytes();
    let mut refused = failure(
        false,
        0,
        None,
        RuntimeError::Unsupported("Engine API version"),
        None,
    );
    refused.error_body = Some(ErrorDiagnostic {
        observed_bytes: message.len() as u64,
        prefix: message,
        complete: true,
        read_error: None,
    });
    refused
}
