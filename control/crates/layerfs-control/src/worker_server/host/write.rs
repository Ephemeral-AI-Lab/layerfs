//! Host worker file writes: admission, the streamed Store write, and the
//! custody of outcomes their caller may not learn (§8.5).
//!
//! A write is admitted like any ordinary call and holds its pool thread
//! while its input streams. An engine-uncertain outcome (`Unknown`) is kept
//! on disk before its answer is sent, and an answer that cannot be
//! delivered is kept after the attempt; both survive drain, stop, crash and
//! replacement and are reported by `machine.worker.retained`.
use super::{log, HostServer};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::StreamReader;
use std::os::unix::net::UnixStream;

impl HostServer {
    /// Admits and runs one `store.write` request whose input follows on
    /// `socket`. Nothing is resent; a lost input commits nothing.
    pub(in crate::worker_server) fn dispatch_write(
        &self,
        args: &SdkValue,
        socket: &UnixStream,
        request_id: &str,
        admitted: &mut bool,
    ) -> Outcome {
        {
            let mut activity = self.lock();
            if activity.draining {
                let mut error = refused("Unavailable", "the host worker is draining");
                error.detail = SdkValue::map([("state", SdkValue::text("draining"))]);
                return Err(error);
            }
            activity.admitted += 1;
            *admitted = true;
        }
        let mut reader = StreamReader::new(socket);
        let outcome = layerfs_store::host_stream::serve(&self.home, args, &mut reader);
        if outcome.as_ref().is_err_and(|error| error.code == "Unknown") {
            self.keep(request_id, "uncertain", args, &outcome);
        }
        outcome
    }

    /// Keeps a write's outcome whose answer could not be written.
    pub(in crate::worker_server) fn undelivered(
        &self,
        request_id: &str,
        args: &SdkValue,
        outcome: &Outcome,
    ) {
        self.keep(request_id, "undelivered", args, outcome);
    }

    fn keep(&self, request_id: &str, kind: &str, args: &SdkValue, outcome: &Outcome) {
        if let Err(error) =
            layerfs_store::host_write::keep(&self.home, request_id, kind, args, outcome)
        {
            log(&format!(
                "could not keep the {kind} outcome of a write: {}",
                error.message
            ));
        }
    }
}
