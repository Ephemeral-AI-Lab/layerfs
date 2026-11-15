//! Switches for the worker's fault tests (layerfs-project#177, #179 E):
//! compiled only with the `worker-fault` cargo feature, which no shipped
//! build turns on and no environment variable stands in for. A test hands
//! one [`Faults`] to [`serve_with`](super::serve_with): the failures that
//! worker then meets, and shorter waits so a test fits its time limit.
use super::Waits;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// What the next request of one operation meets before it runs.
#[derive(Clone, Debug)]
pub enum OpFault {
    /// A panic, before any event.
    Panic,
    /// One event, then a panic.
    PanicAfterEvent,
    /// A wait of this long; the operation then runs.
    Hang(Duration),
    /// `count` events of `bytes` bytes each; the operation then runs.
    Events {
        /// How many events.
        count: usize,
        /// The bytes each one carries.
        bytes: usize,
    },
}

/// One worker's switches and what a test reads back from it.
#[derive(Debug)]
pub struct Faults {
    waits: Waits,
    accepts: Mutex<VecDeque<i32>>,
    spawns: AtomicUsize,
    ops: Mutex<Vec<(String, OpFault)>>,
    lines: Mutex<Vec<String>>,
    serving: AtomicUsize,
}

impl Default for Faults {
    fn default() -> Self {
        Self {
            waits: Waits::SHIPPED,
            accepts: Mutex::default(),
            spawns: AtomicUsize::new(0),
            ops: Mutex::default(),
            lines: Mutex::default(),
            serving: AtomicUsize::new(0),
        }
    }
}

/// These tables hold plain values changed in one step.
fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// One connection being served; its drop ends the count.
pub(super) struct Serving(Arc<Faults>);

impl Drop for Serving {
    fn drop(&mut self) {
        self.0.serving.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Faults {
    /// The whole hello is read within `wait`.
    pub fn hello_wait(mut self, wait: Duration) -> Self {
        self.waits.hello = wait;
        self
    }

    /// A send that makes no progress for `wait` fails.
    pub fn write_wait(mut self, wait: Duration) -> Self {
        self.waits.write = wait;
        self
    }

    /// The caller's close is waited for at most `wait` after its answer.
    pub fn close_wait(mut self, wait: Duration) -> Self {
        self.waits.close = wait;
        self
    }

    /// The pause after a failed accept.
    pub fn accept_pause(mut self, wait: Duration) -> Self {
        self.waits.accept_pause = wait;
        self
    }

    /// At most one log line of one kind of accept or thread failure in `wait`.
    pub fn log_every(mut self, wait: Duration) -> Self {
        self.waits.log_every = wait;
        self
    }

    /// The next accepts fail with these OS error numbers, in order, in
    /// place of accepting.
    pub fn fail_accepts(&self, errors: &[i32]) {
        held(&self.accepts).extend(errors);
    }

    /// The next `count` connections get no thread.
    pub fn fail_spawns(&self, count: usize) {
        self.spawns.fetch_add(count, Ordering::SeqCst);
    }

    /// The next request of operation `op` meets `fault`, once.
    pub fn on(&self, op: &str, fault: OpFault) {
        held(&self.ops).push((op.to_owned(), fault));
    }

    /// The lines the worker logged so far, without the `layerfs worker: `
    /// each starts with on stderr.
    pub fn lines(&self) -> Vec<String> {
        held(&self.lines).clone()
    }

    /// How many connections are being served now.
    pub fn serving_now(&self) -> usize {
        self.serving.load(Ordering::SeqCst)
    }

    pub(super) fn waits(&self) -> Waits {
        self.waits
    }

    pub(super) fn logged(&self, line: &str) {
        held(&self.lines).push(line.to_owned());
    }

    pub(super) fn accept_error(&self) -> Option<std::io::Error> {
        (held(&self.accepts).pop_front()).map(std::io::Error::from_raw_os_error)
    }

    pub(super) fn spawn_error(&self) -> Option<std::io::Error> {
        let taken = |left: usize| left.checked_sub(1);
        (self.spawns)
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, taken)
            .ok()
            .map(|_| std::io::Error::from_raw_os_error(libc::EAGAIN))
    }

    pub(super) fn serving(self: &Arc<Self>) -> Serving {
        self.serving.fetch_add(1, Ordering::SeqCst);
        Serving(self.clone())
    }

    /// What a request of `op` meets before it runs; its events go to
    /// `events`, as the operation's own do.
    pub(super) fn before(
        &self,
        op: &str,
        events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
    ) {
        let fault = {
            let mut ops = held(&self.ops);
            let Some(at) = ops.iter().position(|(named, _)| named == op) else {
                return;
            };
            ops.remove(at).1
        };
        let event = |bytes: usize| SdkValue::map([("fault", SdkValue::Bytes(vec![0; bytes]))]);
        match fault {
            OpFault::Panic => panic!("the worker-fault switch of {op}"),
            OpFault::PanicAfterEvent => {
                let _ = events(event(1));
                panic!("the worker-fault switch of {op}, after an event");
            }
            OpFault::Hang(wait) => std::thread::sleep(wait),
            OpFault::Events { count, bytes } => {
                for _ in 0..count {
                    let _ = events(event(bytes));
                }
            }
        }
    }
}
