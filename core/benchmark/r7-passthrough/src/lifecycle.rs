//! Actual all-loop readiness and retained original receiver/join outcomes.
use crate::state::Passthrough;
use fuser::{Session, SessionOutcome, SessionPhase, SessionSnapshot};
use std::{
    any::Any,
    fmt,
    io::{self, Write},
    sync::Mutex,
    thread,
    time::Duration,
};

pub fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
pub fn emit(value: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{value}")?;
    stdout.flush()
}
fn observation(event: &str, value: SessionSnapshot) -> String {
    format!("{{\"event\":{},\"phase\":{},\"configured\":{},\"created\":{},\"entered\":{},\"exited\":{},\"joined\":{}}}",
        quote(event), quote(&format!("{:?}", value.phase)), value.configured,
        value.created, value.entered, value.exited, value.joined)
}

#[derive(Debug)]
struct RunFailure {
    ready: SessionSnapshot,
    final_state: SessionSnapshot,
    // io::Error requires Sync; this sole bounded carrier preserves original
    // receiver panic payloads as well as startup and cleanup errors.
    original: Mutex<SessionOutcome>,
    output_error: Option<io::Error>,
}
impl fmt::Display for RunFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "original passthrough session failure: ready={:?} final={:?} outcome={:?} output={:?}",
            self.ready, self.final_state, self.original, self.output_error
        )
    }
}
impl std::error::Error for RunFailure {}

struct OwnerPanic(Mutex<Box<dyn Any + Send>>);
impl fmt::Debug for OwnerPanic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("original session-owner panic payload retained")
    }
}
impl fmt::Display for OwnerPanic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keep the original rather than flattening it into a string.
        let _retained = &self.0;
        formatter.write_str("session owner panicked; receiver joins unestablished")
    }
}
impl std::error::Error for OwnerPanic {}

pub fn serve(session: Session<Passthrough>) -> io::Result<()> {
    let runner = session.into_runner();
    let monitor = runner.monitor();
    let owner = match thread::Builder::new()
        .name("r7-passthrough-session".into())
        .spawn(move || runner.run())
    {
        Ok(owner) => owner,
        Err(error) => {
            // The unstarted closure dropped the descriptor. No detach is
            // attempted; the external owner retains the successful mount.
            let _ = emit(&format!("{{\"event\":\"startup_failure\",\"mounted\":true,\"connection_disposition\":\"unstarted runner dropped\",\"original_error\":{}}}", quote(&error.to_string())));
            return Err(error);
        }
    };
    // This is the existing L readiness observation bound, not a command timeout.
    let ready = monitor.wait_ready(Duration::from_secs(5));
    let is_ready = ready.phase == SessionPhase::Serving
        && ready.configured == 2
        && ready.created == 2
        && ready.entered == 2
        && ready.exited == 0;
    let mut output_error = emit(&observation(
        if is_ready {
            "ready"
        } else {
            "startup_not_ready"
        },
        ready,
    ))
    .err();
    // Main is the lifecycle observer. No extra watcher or receive loop is added.
    // Failure or an expired readiness wait retains every created loop until the
    // caller's single explicit detach permits the original joins to complete.
    let mut observed = monitor.snapshot();
    while !matches!(
        observed.phase,
        SessionPhase::Stopping | SessionPhase::Joined
    ) {
        observed = monitor.wait_for_change(observed.revision, Duration::MAX);
    }
    let stopped_output = emit(&observation("stopping", observed)).err();
    if output_error.is_none() {
        output_error = stopped_output;
    }
    let outcome = match owner.join() {
        Ok(outcome) => outcome,
        Err(original) => {
            let _ = emit(&observation("owner_panicked", monitor.snapshot()));
            return Err(io::Error::other(OwnerPanic(Mutex::new(original))));
        }
    };
    let final_state = monitor.snapshot();
    let emitted = emit(&format!(
        "{{\"event\":\"joined\",\"clean\":{},\"snapshot\":{},\"original_outcome\":{}}}",
        outcome.is_clean(),
        observation("final", final_state),
        quote(&format!("{outcome:?}"))
    ))
    .err();
    if output_error.is_none() {
        output_error = emitted;
    }
    if is_ready
        && outcome.is_clean()
        && final_state.phase == SessionPhase::Joined
        && final_state.created == 2
        && final_state.entered == 2
        && final_state.exited == 2
        && final_state.joined == 2
        && output_error.is_none()
    {
        Ok(())
    } else {
        Err(io::Error::other(RunFailure {
            ready,
            final_state,
            original: Mutex::new(outcome),
            output_error,
        }))
    }
}
